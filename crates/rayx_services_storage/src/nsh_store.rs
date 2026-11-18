//! The app's key-value storage on NSH: a [`KeyValueStore`] over NSH's `Storage` (`nsh/storage`),
//! which keeps strings in the device's storage (a file in the app's data directory, the
//! browser's `localStorage`), 5 MB in all, shared with the app's TSX.
//!
//! NSH's storage is reachable only where the surface's engine runs Rust code (a component, an
//! effect, a task), on its engine thread. Every call of the store reads or writes it there: at
//! once where it is reachable (the call answers at once), else through the surface's engine
//! runner, from any thread (the call answers once the engine ran it). The store keeps no copy of
//! the entries: NSH shares one storage's entries between every engine (window, surface) given
//! the same backend, each operation serialized, so the surfaces over one storage, and the app's
//! TSX, read each other's writes as they are made, and a surface over another storage (another
//! backend, a copy of the first) keeps its own. NSH's backend keeps every change itself, so
//! nothing waits to be written as the app quits.

use std::sync::Arc;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use futures::FutureExt as _;
use futures::future::{BoxFuture, LocalBoxFuture};
use nsh_core::prelude::runtime_entered;
use nsh_core::storage::Storage;
use rayx_app::di::{
    AsyncInitializable, StartupInitializable, StartupInitializationContext,
    StartupInitializationResult, StartupInitializationState, StartupServiceType,
};
use rayx_app::services::storage::{KeyValueStore, Result, StorageCapabilities, StorageError};
use rayx_app_nsh::EngineRunner;

/// The prefix of every key RayX keeps in NSH's storage: `rayx/<bucket>/<key>`, with `%` and `/`
/// of the bucket escaped, so other keys of the app (its TSX's own) stay apart.
pub const STORAGE_PREFIX: &str = "rayx/";

/// Values are text in NSH's storage: UTF-8 as it is after `t`, other bytes in base64 after `b`.
fn encode(value: &[u8]) -> String {
    match std::str::from_utf8(value) {
        Ok(text) => format!("t{text}"),
        Err(_) => format!("b{}", STANDARD.encode(value)),
    }
}

fn decode(key: &str, text: &str) -> Result<Vec<u8>> {
    let corrupt =
        |message: String| StorageError::Backend(format!("{key} in NSH's storage: {message}"));
    match text.split_at_checked(1) {
        Some(("t", value)) => Ok(value.as_bytes().to_vec()),
        Some(("b", value)) => STANDARD
            .decode(value)
            .map_err(|error| corrupt(format!("not base64: {error}"))),
        _ => Err(corrupt("not a value RayX wrote".into())),
    }
}

/// The prefix of `bucket`'s keys.
fn bucket_prefix(bucket: &str) -> String {
    format!(
        "{STORAGE_PREFIX}{}/",
        bucket.replace('%', "%25").replace('/', "%2F")
    )
}

/// What the store needs from NSH: its storage, reachable while the engine runs Rust code.
pub(crate) trait Backend: Send + Sync {
    /// Whether NSH's storage is reachable on this thread now.
    fn reachable(&self) -> bool;
    /// Every key, in order.
    fn keys(&self) -> Vec<String>;
    fn get(&self, key: &str) -> Option<String>;
    /// Fails when the storage would grow past its quota.
    fn set(&self, key: &str, value: &str) -> std::result::Result<(), String>;
    fn remove(&self, key: &str);
}

/// NSH's storage of the surface the engine runs.
struct NshStorage;

impl Backend for NshStorage {
    fn reachable(&self) -> bool {
        runtime_entered()
    }

    fn keys(&self) -> Vec<String> {
        Storage::get_all_keys()
    }

    fn get(&self, key: &str) -> Option<String> {
        Storage::get_item(key)
    }

    fn set(&self, key: &str, value: &str) -> std::result::Result<(), String> {
        Storage::set_item(key, value).map_err(|full| full.to_string())
    }

    fn remove(&self, key: &str) {
        Storage::remove_item(key);
    }
}

fn unreachable() -> StorageError {
    StorageError::Backend(
        "NSH's storage is reachable on its surface's engine thread: start the app first".into(),
    )
}

fn get_in(backend: &dyn Backend, bucket: &str, key: &str) -> Result<Option<Vec<u8>>> {
    let full = format!("{}{key}", bucket_prefix(bucket));
    backend
        .get(&full)
        .map(|value| decode(&full, &value))
        .transpose()
}

fn set_in(backend: &dyn Backend, bucket: &str, key: &str, value: &[u8]) -> Result<()> {
    let full = format!("{}{key}", bucket_prefix(bucket));
    backend
        .set(&full, &encode(value))
        .map_err(StorageError::Backend)
}

fn remove_in(backend: &dyn Backend, bucket: &str, key: &str) -> Result<bool> {
    let full = format!("{}{key}", bucket_prefix(bucket));
    let had = backend.get(&full).is_some();
    if had {
        backend.remove(&full);
    }
    Ok(had)
}

fn keys_in(backend: &dyn Backend, bucket: &str) -> Vec<String> {
    let prefix = bucket_prefix(bucket);
    backend
        .keys()
        .into_iter()
        .filter_map(|key| key.strip_prefix(&prefix).map(str::to_owned))
        .collect()
}

fn clear_in(backend: &dyn Backend, bucket: &str) {
    let prefix = bucket_prefix(bucket);
    for key in keys_in(backend, bucket) {
        backend.remove(&format!("{prefix}{key}"));
    }
}

/// RayX's [`KeyValueStore`] on NSH: values in NSH's storage under [`STORAGE_PREFIX`],
/// persistent wherever NSH keeps the app's storage. `add_storage()` registers one per provider
/// as the app's `dyn KeyValueStore`; the stores of the surfaces over one NSH storage share its
/// entries through NSH.
pub(crate) struct NshKeyValueStore {
    backend: Arc<dyn Backend>,
    /// The runner of its surface, which runs its calls on the engine thread from elsewhere.
    runner: Option<EngineRunner>,
}

impl NshKeyValueStore {
    /// The store of NSH's storage of the surface `runner` runs.
    pub(crate) fn new(runner: Option<EngineRunner>) -> Self {
        Self::over(Arc::new(NshStorage), runner)
    }

    /// The store over `backend`.
    pub(crate) fn over(backend: Arc<dyn Backend>, runner: Option<EngineRunner>) -> Self {
        Self { backend, runner }
    }

    /// Whether its surface's NSH storage is reachable here: its engine runs Rust code on this
    /// thread (another surface's engine thread reaches another storage).
    fn here(&self) -> bool {
        self.backend.reachable()
            && self
                .runner
                .as_ref()
                .is_none_or(EngineRunner::is_engine_thread)
    }

    /// Answers with `call` over NSH's storage: at once where it is reachable, else once the
    /// engine runner ran it on the engine thread; an error without one, or once the engine is
    /// gone.
    fn answer<T: Send + 'static>(
        &self,
        call: impl FnOnce(&dyn Backend) -> Result<T> + Send + 'static,
    ) -> BoxFuture<'static, Result<T>> {
        if self.here() {
            return futures::future::ready(call(self.backend.as_ref())).boxed();
        }
        let Some(runner) = &self.runner else {
            return futures::future::ready(Err(unreachable())).boxed();
        };
        let backend = Arc::clone(&self.backend);
        let ran = runner.run(move || {
            let answer = if backend.reachable() {
                call(backend.as_ref())
            } else {
                Err(unreachable())
            };
            async move { answer }
        });
        async move {
            ran.await
                .map_err(|gone| StorageError::Backend(gone.to_string()))?
        }
        .boxed()
    }
}

impl KeyValueStore for NshKeyValueStore {
    fn capabilities(&self) -> StorageCapabilities {
        StorageCapabilities {
            persistent: true,
            secure: false,
        }
    }

    fn get(&self, bucket: String, key: String) -> BoxFuture<'_, Result<Option<Vec<u8>>>> {
        self.answer(move |backend| get_in(backend, &bucket, &key))
    }

    fn set(&self, bucket: String, key: String, value: Vec<u8>) -> BoxFuture<'_, Result<()>> {
        self.answer(move |backend| set_in(backend, &bucket, &key, &value))
    }

    fn remove(&self, bucket: String, key: String) -> BoxFuture<'_, Result<bool>> {
        self.answer(move |backend| remove_in(backend, &bucket, &key))
    }

    fn keys(&self, bucket: String) -> BoxFuture<'_, Result<Vec<String>>> {
        self.answer(move |backend| Ok(keys_in(backend, &bucket)))
    }

    fn clear(&self, bucket: String) -> BoxFuture<'_, Result<()>> {
        self.answer(move |backend| {
            clear_in(backend, &bucket);
            Ok(())
        })
    }
}

/// A provider's start of its store: the startup service `add_storage()` registers (keyed by
/// `dyn KeyValueStore`, which the input map and history initialize after). It reads NSH's
/// storage once on the engine thread, so NSH loads what the device holds as the app starts.
pub(crate) struct StorageStartup {
    store: Arc<NshKeyValueStore>,
    initialization: StartupInitializationState,
}

impl StorageStartup {
    pub(crate) fn new(store: Arc<NshKeyValueStore>) -> Self {
        Self {
            store,
            initialization: StartupInitializationState::default(),
        }
    }
}

impl AsyncInitializable for StorageStartup {
    fn is_initialized(&self) -> bool {
        self.initialization.is_initialized()
    }

    /// Reads NSH's storage (startup runs it on the engine thread).
    fn initialize(
        &self,
        _context: StartupInitializationContext,
    ) -> LocalBoxFuture<'static, StartupInitializationResult> {
        let read = self.store.answer(|backend| {
            backend.get(STORAGE_PREFIX);
            Ok(())
        });
        let initialization = self.initialization.clone();
        async move {
            read.await.map_err(|error| error.to_string())?;
            initialization.mark_initialized();
            Ok(())
        }
        .boxed_local()
    }
}

impl StartupInitializable for StorageStartup {
    fn startup_service_type(&self) -> Option<StartupServiceType> {
        Some(StartupServiceType::of::<dyn KeyValueStore>())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicBool, Ordering};

    /// NSH's storage in memory, reachable when the test says so, with a quota of `limit` bytes
    /// per value.
    #[derive(Clone, Default)]
    struct Fake {
        reachable: Arc<AtomicBool>,
        entries: Arc<Mutex<BTreeMap<String, String>>>,
        limit: Arc<Mutex<Option<usize>>>,
    }

    impl Backend for Fake {
        fn reachable(&self) -> bool {
            self.reachable.load(Ordering::SeqCst)
        }
        fn keys(&self) -> Vec<String> {
            self.entries.lock().unwrap().keys().cloned().collect()
        }
        fn get(&self, key: &str) -> Option<String> {
            self.entries.lock().unwrap().get(key).cloned()
        }
        fn set(&self, key: &str, value: &str) -> std::result::Result<(), String> {
            if self
                .limit
                .lock()
                .unwrap()
                .is_some_and(|limit| value.len() > limit)
            {
                return Err("storage is full".into());
            }
            self.entries
                .lock()
                .unwrap()
                .insert(key.to_string(), value.to_string());
            Ok(())
        }
        fn remove(&self, key: &str) {
            self.entries.lock().unwrap().remove(key);
        }
    }

    fn store(fake: &Fake) -> NshKeyValueStore {
        NshKeyValueStore::over(Arc::new(fake.clone()), None)
    }

    fn reachable(fake: &Fake, on: bool) {
        fake.reachable.store(on, Ordering::SeqCst);
    }

    /// What `call` answered, at once.
    fn now<T>(call: BoxFuture<'_, Result<T>>) -> Result<T> {
        call.now_or_never().expect("the store answers at once")
    }

    fn get(store: &NshKeyValueStore, bucket: &str, key: &str) -> Result<Option<Vec<u8>>> {
        now(store.get(bucket.into(), key.into()))
    }

    fn set(store: &NshKeyValueStore, bucket: &str, key: &str, value: &[u8]) -> Result<()> {
        now(store.set(bucket.into(), key.into(), value.to_vec()))
    }

    fn keys(store: &NshKeyValueStore, bucket: &str) -> Vec<String> {
        now(store.keys(bucket.into())).unwrap()
    }

    #[test]
    fn values_are_text_under_a_prefix_per_bucket() {
        let fake = Fake::default();
        fake.entries
            .lock()
            .unwrap()
            .insert("theme".into(), "the app's own".into());
        reachable(&fake, true);
        let store = store(&fake);
        set(&store, "prefs", "lang", b"hr").unwrap();
        set(&store, "a/b%", "bytes", &[0, 159, 146, 150]).unwrap();
        let entries = fake.entries.lock().unwrap().clone();
        assert_eq!(entries["rayx/prefs/lang"], "thr");
        assert_eq!(entries["rayx/a%2Fb%25/bytes"], "bAJ+Slg==");
        assert_eq!(entries["theme"], "the app's own", "other keys stay");
        assert_eq!(get(&store, "prefs", "lang").unwrap(), Some(b"hr".to_vec()));
        assert_eq!(
            get(&store, "a/b%", "bytes").unwrap(),
            Some(vec![0, 159, 146, 150])
        );
        assert_eq!(keys(&store, "prefs"), ["lang"]);
        assert!(keys(&store, "theme").is_empty());
        assert!(now(store.remove("prefs".into(), "lang".into())).unwrap());
        assert!(!now(store.remove("prefs".into(), "lang".into())).unwrap());
        now(store.clear("a/b%".into())).unwrap();
        assert_eq!(
            fake.entries.lock().unwrap().keys().collect::<Vec<_>>(),
            ["theme"]
        );
    }

    #[test]
    fn without_an_engine_the_storage_is_unreachable_elsewhere() {
        let fake = Fake::default();
        fake.entries
            .lock()
            .unwrap()
            .insert("rayx/prefs/lang".into(), "tde".into());
        let store = store(&fake);
        assert!(get(&store, "prefs", "lang").is_err());
        assert!(set(&store, "prefs", "lang", b"hr").is_err());
        reachable(&fake, true);
        assert_eq!(get(&store, "prefs", "lang").unwrap(), Some(b"de".to_vec()));
    }

    #[test]
    fn a_full_storage_refuses_the_write() {
        let fake = Fake::default();
        *fake.limit.lock().unwrap() = Some(4);
        reachable(&fake, true);
        let store = store(&fake);
        set(&store, "p", "small", b"ok").unwrap();
        assert!(set(&store, "p", "big", b"too big").is_err());
        assert_eq!(get(&store, "p", "big").unwrap(), None);
        assert!(set(&store, "p", "small", b"too big").is_err());
        assert_eq!(get(&store, "p", "small").unwrap(), Some(b"ok".to_vec()));
    }

    /// The stores over one NSH storage keep no copy: each reads what the storage holds, whoever
    /// changed it (another store, the app's TSX clearing it).
    #[test]
    fn stores_over_one_storage_read_what_it_holds() {
        let fake = Fake::default();
        reachable(&fake, true);
        let first = store(&fake);
        let second = store(&fake);
        set(&first, "prefs", "lang", b"hr").unwrap();
        assert_eq!(get(&second, "prefs", "lang").unwrap(), Some(b"hr".to_vec()));
        fake.entries.lock().unwrap().clear();
        assert_eq!(get(&first, "prefs", "lang").unwrap(), None);
        set(&second, "prefs", "lang", b"de").unwrap();
        assert_eq!(get(&first, "prefs", "lang").unwrap(), Some(b"de".to_vec()));
        assert_eq!(
            fake.entries.lock().unwrap().keys().collect::<Vec<_>>(),
            ["rayx/prefs/lang"],
            "nothing of RayX's own beside the values"
        );
    }
}
