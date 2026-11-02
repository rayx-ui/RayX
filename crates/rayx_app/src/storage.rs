//! The app's key-value storage on NSH: a [`KeyValueStore`] over NSH's `Storage` (`nsh/storage`),
//! which keeps strings in the device's storage (a file in the app's data directory, the
//! browser's `localStorage`), 5 MB in all, shared with the app's TSX.
//!
//! NSH's storage is reachable only while the engine runs Rust code (a component, an effect, a
//! task), and a class's synchronous calls from TSX run outside it, as does any other thread. So
//! the store keeps a mirror of its entries, loaded on the engine thread at startup: reads come
//! from it on any thread, a write made where NSH's storage is reachable goes through at once (and
//! fails when the storage is full), and one made elsewhere waits in the mirror until a task on the
//! engine thread writes it, or the next call where the storage is reachable does.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, MutexGuard};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use futures::channel::mpsc::{UnboundedSender, unbounded};
use futures::future::LocalBoxFuture;
use futures::{FutureExt as _, StreamExt as _};
use nsh_core::prelude::{Owner, spawn, try_global};
use nsh_core::storage::Storage;
use rayx_core::di::{
    AppService, AsyncInitializable, StartupInitializable, StartupInitializationContext,
    StartupInitializationResult, StartupInitializationState, StartupServiceType,
};
use rayx_runtime::services::storage::{KeyValueStore, Result, StorageCapabilities, StorageError};

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
    state: Mutex<State>,
    /// Wakes the task that writes [`State::pending`] on the engine thread, once it runs.
    flusher: Mutex<Option<UnboundedSender<()>>>,
    initialization: StartupInitializationState,
}

/// The app's [`KeyValueStore`] on NSH: values in NSH's storage under [`STORAGE_PREFIX`],
/// persistent wherever NSH keeps the app's storage. `add_nsh_services` registers it, so on NSH
/// `add_storage()` keeps it and the app's data outlives the process.
#[derive(Clone)]
pub struct NshKeyValueStore {
    shared: Arc<Shared>,
}

impl Default for NshKeyValueStore {
    fn default() -> Self {
        Self::new()
    }
}

impl AppService for NshKeyValueStore {}

impl NshKeyValueStore {
    /// The store of the surface the engine runs it on.
    pub fn new() -> Self {
        Self::over(Box::new(NshStorage))
    }

    pub(crate) fn over(backend: Box<dyn Backend>) -> Self {
        Self {
            shared: Arc::new(Shared {
                backend,
                state: Mutex::default(),
                flusher: Mutex::default(),
                initialization: StartupInitializationState::default(),
            }),
        }
    }

    fn state(&self) -> Result<MutexGuard<'_, State>> {
        self.shared.state.lock().map_err(|_| StorageError::Poisoned)
    }

    /// Where NSH's storage is reachable: loads the mirror once and writes what waits. A waiting
    /// value the storage refuses (full) is dropped from the mirror, which takes the storage's
    /// value again; the first such refusal is the error.
    pub fn flush(&self) -> Result<()> {
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

    /// The mirror, flushed and loaded; an error where it has not loaded yet.
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

    /// Writes `change` at once where NSH's storage is reachable, else leaves it waiting.
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
        if let Ok(flusher) = self.shared.flusher.lock()
            && let Some(flusher) = flusher.as_ref()
        {
            let _ = flusher.unbounded_send(());
        }
        Ok(())
    }

    /// Starts the task that writes waiting values on the engine thread; it lives as long as the
    /// surface. Call it where the engine runs Rust code.
    fn start_flusher(&self) {
        let (wake, mut woken) = unbounded::<()>();
        if let Ok(mut flusher) = self.shared.flusher.lock() {
            *flusher = Some(wake);
        }
        let store = self.clone();
        Owner::detached().run(move || {
            drop(spawn(async move {
                while woken.next().await.is_some() {
                    let _ = store.flush();
                }
            }));
        });
    }
}

impl KeyValueStore for NshKeyValueStore {
    fn capabilities(&self) -> StorageCapabilities {
        StorageCapabilities {
            persistent: true,
            secure: false,
        }
    }

    fn get(&self, bucket: &str, key: &str) -> Result<Option<Vec<u8>>> {
        let full = format!("{}{key}", bucket_prefix(bucket));
        let state = self.loaded()?;
        state
            .values
            .get(&full)
            .map(|value| decode(&full, value))
            .transpose()
    }

    fn set(&self, bucket: &str, key: &str, value: Vec<u8>) -> Result<()> {
        let full = format!("{}{key}", bucket_prefix(bucket));
        let state = self.loaded()?;
        self.write(state, full, Some(encode(&value)))
    }

    fn remove(&self, bucket: &str, key: &str) -> Result<bool> {
        let full = format!("{}{key}", bucket_prefix(bucket));
        let state = self.loaded()?;
        let had = state.values.contains_key(&full);
        if had {
            self.write(state, full, None)?;
        }
        Ok(had)
    }

    fn keys(&self, bucket: &str) -> Result<Vec<String>> {
        let prefix = bucket_prefix(bucket);
        let state = self.loaded()?;
        Ok(state
            .values
            .range(prefix.clone()..)
            .take_while(|(key, _)| key.starts_with(&prefix))
            .map(|(key, _)| key[prefix.len()..].to_string())
            .collect())
    }

    fn clear(&self, bucket: &str) -> Result<()> {
        for key in self.keys(bucket)? {
            self.remove(bucket, &key)?;
        }
        Ok(())
    }
}

impl AsyncInitializable for NshKeyValueStore {
    fn is_initialized(&self) -> bool {
        self.shared.initialization.is_initialized()
    }

    /// Loads the mirror and starts the task that writes waiting values; startup runs it on the
    /// engine thread.
    fn initialize(
        &self,
        _context: StartupInitializationContext,
    ) -> LocalBoxFuture<'static, StartupInitializationResult> {
        let store = self.clone();
        async move {
            store.flush().map_err(|error| error.to_string())?;
            store.start_flusher();
            store.shared.initialization.mark_initialized();
            Ok(())
        }
        .boxed_local()
    }
}

impl StartupInitializable for NshKeyValueStore {
    fn startup_service_type(&self) -> StartupServiceType {
        StartupServiceType::of::<Self>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
        NshKeyValueStore::over(Box::new(fake.clone()))
    }

    fn reachable(fake: &Fake, on: bool) {
        fake.reachable.store(on, Ordering::SeqCst);
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
        store.set("prefs", "lang", b"hr".to_vec()).unwrap();
        store.set("a/b%", "bytes", vec![0, 159, 146, 150]).unwrap();
        let entries = fake.entries.lock().unwrap().clone();
        assert_eq!(entries["rayx/prefs/lang"], "thr");
        assert_eq!(entries["rayx/a%2Fb%25/bytes"], "bAJ+Slg==");
        assert_eq!(entries["theme"], "the app's own", "other keys stay");
        assert_eq!(store.get("prefs", "lang").unwrap(), Some(b"hr".to_vec()));
        assert_eq!(
            store.get("a/b%", "bytes").unwrap(),
            Some(vec![0, 159, 146, 150])
        );
        assert_eq!(store.keys("prefs").unwrap(), ["lang"]);
        assert!(store.keys("theme").unwrap().is_empty());
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
            store.get("prefs", "lang").is_err(),
            "nothing to read before it loads"
        );
        reachable(&fake, true);
        assert_eq!(store.get("prefs", "lang").unwrap(), Some(b"de".to_vec()));
        reachable(&fake, false);
        assert_eq!(
            store.get("prefs", "lang").unwrap(),
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
        store.set("prefs", "lang", b"hr".to_vec()).unwrap();
        assert!(!store.remove("prefs", "missing").unwrap());
        assert_eq!(
            store.get("prefs", "lang").unwrap(),
            Some(b"hr".to_vec()),
            "read back at once"
        );
        assert!(fake.entries.lock().unwrap().is_empty(), "not written yet");
        reachable(&fake, true);
        store.flush().unwrap();
        assert_eq!(fake.entries.lock().unwrap()["rayx/prefs/lang"], "thr");

        reachable(&fake, false);
        store.clear("prefs").unwrap();
        assert!(store.keys("prefs").unwrap().is_empty());
        reachable(&fake, true);
        // Any call where the storage is reachable writes what waits.
        assert!(store.keys("prefs").unwrap().is_empty());
        assert!(fake.entries.lock().unwrap().is_empty());
    }

    #[test]
    fn a_full_storage_refuses_the_write() {
        let fake = Fake::default();
        *fake.limit.lock().unwrap() = Some(4);
        reachable(&fake, true);
        let store = store(&fake);
        store.set("p", "small", b"ok".to_vec()).unwrap();
        // Where the storage is reachable, the write itself fails.
        assert!(store.set("p", "big", b"too big".to_vec()).is_err());
        assert_eq!(store.get("p", "big").unwrap(), None);
        // Elsewhere it is taken back when it is written.
        reachable(&fake, false);
        store.set("p", "small", b"too big".to_vec()).unwrap();
        reachable(&fake, true);
        assert!(store.flush().is_err());
        assert_eq!(store.get("p", "small").unwrap(), Some(b"ok".to_vec()));
    }
}
