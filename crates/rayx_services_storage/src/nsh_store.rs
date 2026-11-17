//! The app's key-value storage on NSH: a [`KeyValueStore`] over NSH's `Storage` (`nsh/storage`),
//! which keeps strings in the device's storage (a file in the app's data directory, the
//! browser's `localStorage`), 5 MB in all, shared with the app's TSX.
//!
//! NSH's storage is reachable only while the engine runs Rust code (a component, an effect, a
//! task), and a class's synchronous calls from TSX run outside it, as does any other thread. So
//! the store keeps a mirror of its entries, loaded on the engine thread at startup (or, called
//! before, through the surface's engine runner): reads come from it on any thread, a write made
//! where NSH's storage is reachable goes through at once (and fails when the storage is full),
//! and one made elsewhere waits in the mirror while the engine runner writes it on the engine
//! thread. As the app quits (the app's `dyn SuspensionService` publishes `Destroying`, then
//! `Destroyed`) the store writes what still waits. Every call answers at once once the mirror
//! loaded.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, MutexGuard, Weak};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use futures::FutureExt as _;
use futures::future::{BoxFuture, LocalBoxFuture};
use nsh_core::prelude::try_global;
use nsh_core::storage::Storage;
use rayx_app::ChangeSubscription;
use rayx_app::di::{
    AsyncInitializable, StartupInitializable, StartupInitializationContext,
    StartupInitializationResult, StartupInitializationState, StartupServiceType,
};
use rayx_app::services::storage::{KeyValueStore, Result, StorageCapabilities, StorageError};
use rayx_app::services::suspension::{SuspensionEventKind, SuspensionService};
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
    /// Whether NSH's storage is reachable now.
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

/// A type for `try_global` to probe the reactive runtime with.
#[derive(Default)]
struct Probe;

impl Backend for NshStorage {
    fn reachable(&self) -> bool {
        try_global::<Probe>().is_some()
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

/// A write that waits for NSH's storage to be reachable.
enum Pending {
    Set(String),
    Remove(String),
}

#[derive(Default)]
struct State {
    /// Whether the mirror holds what NSH's storage held under [`STORAGE_PREFIX`].
    loaded: bool,
    /// The entries under [`STORAGE_PREFIX`], by their key in NSH's storage, encoded.
    values: BTreeMap<String, String>,
    /// Writes made where NSH's storage was not reachable, in order.
    pending: Vec<Pending>,
}

struct Shared {
    backend: Box<dyn Backend>,
    /// The surface's runner, which reaches NSH's storage on the engine thread (none outside an
    /// app's start, where only the engine thread itself reaches it).
    runner: Option<EngineRunner>,
    state: Mutex<State>,
    /// What the store hears of the app quitting, once it initialized.
    quitting: Mutex<Option<ChangeSubscription>>,
    initialization: StartupInitializationState,
}

/// RayX's [`KeyValueStore`] on NSH: values in NSH's storage under [`STORAGE_PREFIX`],
/// persistent wherever NSH keeps the app's storage. `add_storage()` registers it as the app's
/// `dyn KeyValueStore`, a startup service that loads its mirror on the engine thread.
#[derive(Clone)]
pub(crate) struct NshKeyValueStore {
    shared: Arc<Shared>,
}

impl NshKeyValueStore {
    /// The store of the surface `runner` runs (or of the engine it is made on).
    pub(crate) fn new(runner: Option<EngineRunner>) -> Self {
        Self::over(Box::new(NshStorage), runner)
    }

    pub(crate) fn over(backend: Box<dyn Backend>, runner: Option<EngineRunner>) -> Self {
        Self {
            shared: Arc::new(Shared {
                backend,
                runner,
                state: Mutex::default(),
                quitting: Mutex::default(),
                initialization: StartupInitializationState::default(),
            }),
        }
    }

    fn state(&self) -> Result<MutexGuard<'_, State>> {
        self.shared.state.lock().map_err(|_| StorageError::Poisoned)
    }

    /// Where NSH's storage is reachable: loads the mirror once and writes what waits. A waiting
    /// value the storage refuses (full) is dropped from the mirror, which takes the storage's
    /// value again; the first such refusal is the error. Elsewhere it does nothing.
    pub(crate) fn flush(&self) -> Result<()> {
        let backend = &self.shared.backend;
        if !backend.reachable() {
            return Ok(());
        }
        let mut state = self.state()?;
        if !state.loaded {
            let mut values = BTreeMap::new();
            for key in backend.keys() {
                if key.starts_with(STORAGE_PREFIX)
                    && let Some(value) = backend.get(&key)
                {
                    values.insert(key, value);
                }
            }
            // Writes made before the load win over what was stored.
            for pending in &state.pending {
                match pending {
                    Pending::Set(key) => {
                        if let Some(value) = state.values.get(key) {
                            values.insert(key.clone(), value.clone());
                        }
                    }
                    Pending::Remove(key) => {
                        values.remove(key);
                    }
                }
            }
            state.values = values;
            state.loaded = true;
        }
        let mut refused = None;
        for pending in std::mem::take(&mut state.pending) {
            match pending {
                Pending::Set(key) => {
                    let Some(value) = state.values.get(&key).cloned() else {
                        continue;
                    };
                    if let Err(error) = backend.set(&key, &value) {
                        match backend.get(&key) {
                            Some(stored) => state.values.insert(key.clone(), stored),
                            None => state.values.remove(&key),
                        };
                        refused.get_or_insert(StorageError::Backend(error));
                    }
                }
                Pending::Remove(key) => backend.remove(&key),
            }
        }
        refused.map_or(Ok(()), Err)
    }

    /// Resolves once the mirror loaded: at once where it has or NSH's storage is reachable,
    /// else through the engine runner; an error where neither can load it.
    fn load(&self) -> BoxFuture<'static, Result<()>> {
        let loaded = || -> Result<bool> { Ok(self.state()?.loaded) };
        match loaded() {
            Ok(true) => return futures::future::ready(Ok(())).boxed(),
            Ok(false) => {}
            Err(error) => return futures::future::ready(Err(error)).boxed(),
        }
        if self.shared.backend.reachable() {
            return futures::future::ready(self.flush()).boxed();
        }
        let Some(runner) = self.shared.runner.clone() else {
            return futures::future::ready(Err(StorageError::Backend(
                "NSH's storage loads on the engine's thread: start the app first".into(),
            )))
            .boxed();
        };
        let store = self.clone();
        let flushed = runner.run(move || {
            let flushed = store.flush();
            async move { flushed }
        });
        async move {
            flushed
                .await
                .map_err(|gone| StorageError::Backend(gone.to_string()))?
        }
        .boxed()
    }

    /// The mirror, flushed where NSH's storage is reachable; an error where it has not loaded.
    fn loaded(&self) -> Result<MutexGuard<'_, State>> {
        self.flush()?;
        let state = self.state()?;
        if !state.loaded {
            return Err(StorageError::Backend(
                "NSH's storage loads on the engine's thread: start the app first".into(),
            ));
        }
        Ok(state)
    }

    /// Writes `value` (or removes the key) at once where NSH's storage is reachable, else leaves
    /// it waiting and has the engine runner write it.
    fn write(
        &self,
        mut state: MutexGuard<'_, State>,
        key: String,
        value: Option<String>,
    ) -> Result<()> {
        let backend = &self.shared.backend;
        if backend.reachable() {
            match &value {
                Some(value) => backend.set(&key, value).map_err(StorageError::Backend)?,
                None => backend.remove(&key),
            }
            match value {
                Some(value) => state.values.insert(key, value),
                None => state.values.remove(&key),
            };
            return Ok(());
        }
        match value {
            Some(value) => {
                state.values.insert(key.clone(), value);
                state.pending.push(Pending::Set(key));
            }
            None => {
                state.values.remove(&key);
                state.pending.push(Pending::Remove(key));
            }
        }
        drop(state);
        if let Some(runner) = &self.shared.runner {
            let store = self.clone();
            // Once the engine is gone the write waits for nothing more: it stays in the mirror.
            let _ = runner.post(move || {
                let _ = store.flush();
            });
        }
        Ok(())
    }

    fn get_now(&self, bucket: &str, key: &str) -> Result<Option<Vec<u8>>> {
        let full = format!("{}{key}", bucket_prefix(bucket));
        let state = self.loaded()?;
        state
            .values
            .get(&full)
            .map(|value| decode(&full, value))
            .transpose()
    }

    fn set_now(&self, bucket: &str, key: &str, value: &[u8]) -> Result<()> {
        let full = format!("{}{key}", bucket_prefix(bucket));
        let state = self.loaded()?;
        self.write(state, full, Some(encode(value)))
    }

    fn remove_now(&self, bucket: &str, key: &str) -> Result<bool> {
        let full = format!("{}{key}", bucket_prefix(bucket));
        let state = self.loaded()?;
        let had = state.values.contains_key(&full);
        if had {
            self.write(state, full, None)?;
        }
        Ok(had)
    }

    fn keys_now(&self, bucket: &str) -> Result<Vec<String>> {
        let prefix = bucket_prefix(bucket);
        let state = self.loaded()?;
        Ok(state
            .values
            .range(prefix.clone()..)
            .take_while(|(key, _)| key.starts_with(&prefix))
            .map(|(key, _)| key[prefix.len()..].to_string())
            .collect())
    }

    fn clear_now(&self, bucket: &str) -> Result<()> {
        for key in self.keys_now(bucket)? {
            self.remove_now(bucket, &key)?;
        }
        Ok(())
    }

    /// Answers with `call` once the mirror loaded.
    fn answer<T: Send + 'static>(
        &self,
        call: impl FnOnce(&Self) -> Result<T> + Send + 'static,
    ) -> BoxFuture<'static, Result<T>> {
        let load = self.load();
        let store = self.clone();
        async move {
            load.await?;
            call(&store)
        }
        .boxed()
    }

    /// Writes what waits as the app quits: when `suspension` publishes `Destroying` (before the
    /// app's own listeners, which subscribed later) and `Destroyed` (after them, so what they
    /// saved as they heard `Destroying` too).
    fn flush_on_quit(&self, suspension: &dyn SuspensionService) {
        let store: Weak<Shared> = Arc::downgrade(&self.shared);
        let subscription = suspension.events().subscribe(move |event| {
            if matches!(
                event.kind,
                SuspensionEventKind::Destroying | SuspensionEventKind::Destroyed
            ) && let Some(shared) = store.upgrade()
            {
                let _ = NshKeyValueStore { shared }.flush();
            }
        });
        if let Ok(mut quitting) = self.shared.quitting.lock() {
            *quitting = Some(subscription);
        }
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
        self.answer(move |store| store.get_now(&bucket, &key))
    }

    fn set(&self, bucket: String, key: String, value: Vec<u8>) -> BoxFuture<'_, Result<()>> {
        self.answer(move |store| store.set_now(&bucket, &key, &value))
    }

    fn remove(&self, bucket: String, key: String) -> BoxFuture<'_, Result<bool>> {
        self.answer(move |store| store.remove_now(&bucket, &key))
    }

    fn keys(&self, bucket: String) -> BoxFuture<'_, Result<Vec<String>>> {
        self.answer(move |store| store.keys_now(&bucket))
    }

    fn clear(&self, bucket: String) -> BoxFuture<'_, Result<()>> {
        self.answer(move |store| store.clear_now(&bucket))
    }
}

impl AsyncInitializable for NshKeyValueStore {
    fn is_initialized(&self) -> bool {
        self.shared.initialization.is_initialized()
    }

    /// Loads the mirror (startup runs it on the engine thread) and listens for the app quitting
    /// through the app's `dyn SuspensionService`.
    fn initialize(
        &self,
        context: StartupInitializationContext,
    ) -> LocalBoxFuture<'static, StartupInitializationResult> {
        let store = self.clone();
        async move {
            store.load().await.map_err(|error| error.to_string())?;
            if let Some(suspension) = context.services().service::<dyn SuspensionService>() {
                store.flush_on_quit(suspension.as_ref());
            }
            store.shared.initialization.mark_initialized();
            Ok(())
        }
        .boxed_local()
    }
}

impl StartupInitializable for NshKeyValueStore {
    fn startup_service_type(&self) -> Option<StartupServiceType> {
        Some(StartupServiceType::of::<dyn KeyValueStore>())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rayx_app::services::suspension::{SuspensionEvent, SuspensionEventReason};
    use rayx_app::testing::FakeSuspension;
    use std::sync::atomic::{AtomicBool, Ordering};

    /// NSH's storage in memory, reachable when the test says so, with a quota of `limit` bytes.
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
        NshKeyValueStore::over(Box::new(fake.clone()), None)
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
    }

    #[test]
    fn the_mirror_loads_where_the_storage_is_reachable() {
        let fake = Fake::default();
        fake.entries
            .lock()
            .unwrap()
            .insert("rayx/prefs/lang".into(), "tde".into());
        let store = store(&fake);
        assert!(
            get(&store, "prefs", "lang").is_err(),
            "nothing to read before it loads, without an engine to load it on"
        );
        reachable(&fake, true);
        assert_eq!(get(&store, "prefs", "lang").unwrap(), Some(b"de".to_vec()));
        reachable(&fake, false);
        assert_eq!(
            get(&store, "prefs", "lang").unwrap(),
            Some(b"de".to_vec()),
            "the mirror answers elsewhere"
        );
    }

    #[test]
    fn writes_elsewhere_wait_for_the_engine_thread() {
        let fake = Fake::default();
        reachable(&fake, true);
        let store = store(&fake);
        store.flush().unwrap();
        reachable(&fake, false);
        set(&store, "prefs", "lang", b"hr").unwrap();
        assert!(!now(store.remove("prefs".into(), "missing".into())).unwrap());
        assert_eq!(
            get(&store, "prefs", "lang").unwrap(),
            Some(b"hr".to_vec()),
            "read back at once"
        );
        assert!(fake.entries.lock().unwrap().is_empty(), "not written yet");
        reachable(&fake, true);
        store.flush().unwrap();
        assert_eq!(fake.entries.lock().unwrap()["rayx/prefs/lang"], "thr");

        reachable(&fake, false);
        now(store.clear("prefs".into())).unwrap();
        assert!(keys(&store, "prefs").is_empty());
        reachable(&fake, true);
        // Any call where the storage is reachable writes what waits.
        assert!(keys(&store, "prefs").is_empty());
        assert!(fake.entries.lock().unwrap().is_empty());
    }

    #[test]
    fn a_full_storage_refuses_the_write() {
        let fake = Fake::default();
        *fake.limit.lock().unwrap() = Some(4);
        reachable(&fake, true);
        let store = store(&fake);
        set(&store, "p", "small", b"ok").unwrap();
        // Where the storage is reachable, the write itself fails.
        assert!(set(&store, "p", "big", b"too big").is_err());
        assert_eq!(get(&store, "p", "big").unwrap(), None);
        // Elsewhere it is taken back when it is written.
        reachable(&fake, false);
        set(&store, "p", "small", b"too big").unwrap();
        reachable(&fake, true);
        assert!(store.flush().is_err());
        assert_eq!(get(&store, "p", "small").unwrap(), Some(b"ok".to_vec()));
    }

    #[test]
    fn what_waits_is_written_as_the_app_quits() {
        let fake = Fake::default();
        reachable(&fake, true);
        let store = store(&fake);
        store.flush().unwrap();
        let suspension = FakeSuspension::new();
        store.flush_on_quit(suspension.as_ref());
        reachable(&fake, false);
        set(&store, "draft", "text", b"unsaved").unwrap();
        assert!(fake.entries.lock().unwrap().is_empty(), "not written yet");
        // Quitting runs on the engine thread, where NSH's storage is reachable.
        reachable(&fake, true);
        suspension.emit(SuspensionEvent::new(
            SuspensionEventKind::Destroying,
            SuspensionEventReason::HostStopping,
        ));
        assert_eq!(fake.entries.lock().unwrap()["rayx/draft/text"], "tunsaved");
    }
}
