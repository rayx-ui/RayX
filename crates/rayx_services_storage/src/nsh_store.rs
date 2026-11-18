//! The app's key-value storage on NSH: a [`KeyValueStore`] over NSH's `Storage` (`nsh/storage`),
//! which keeps strings in the device's storage (a file in the app's data directory, the
//! browser's `localStorage`), 5 MB in all, shared with the app's TSX.
//!
//! NSH's storage is reachable only while the engine runs Rust code (a component, an effect, a
//! task), and a class's synchronous calls from TSX run outside it, as does any other thread. So
//! the store keeps a mirror of its entries, loaded on the engine thread at startup (or, called
//! before, through the surface's engine runner): reads come from it on any thread, a write made
//! where NSH's storage is reachable goes through at once (and fails when the storage is full),
//! and one made elsewhere waits in the mirror while an engine runner writes it on its engine
//! thread. As the app quits (the app's `dyn SuspensionService` publishes `Destroying`, then
//! `Destroyed`) the store writes what still waits. Every call answers at once once the mirror
//! loaded.
//!
//! One store per NSH storage in the process: NSH shares one storage between the surfaces (the
//! windows, the engines) given the same backend, and every provider whose surface's storage it
//! is resolves the same store, one mirror written in order, so the surfaces read each other's
//! writes and what locks on the store (the history's journals) locks across them. A surface
//! finds its storage's store by the id the store keeps in it ([`STORE_ID_KEY`]).

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError, Weak};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use futures::FutureExt as _;
use futures::future::{BoxFuture, LocalBoxFuture};
use nsh_core::prelude::runtime_entered;
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

/// The key, under [`STORAGE_PREFIX`] and in no bucket (a bucket's keys have a `/` after its
/// name), of the id of the process's store over that NSH storage: a surface whose NSH storage
/// holds the id of a store the process keeps shares that store. Written as the first surface
/// over the storage starts, with an id no other run or storage has.
pub(crate) const STORE_ID_KEY: &str = "rayx/.store";

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

/// The store's values over one NSH storage: one mirror, written in order under its lock by
/// whichever surface over the storage reaches it.
struct Shared {
    backend: Arc<dyn Backend>,
    state: Mutex<State>,
    /// The runners of the surfaces over this storage, which write what waits on their engine
    /// thread: the first whose engine still runs.
    runners: Mutex<Vec<EngineRunner>>,
    /// The store's id in NSH's storage ([`STORE_ID_KEY`]); none for one kept apart.
    id: Option<String>,
}

impl Shared {
    fn new(backend: Arc<dyn Backend>, id: Option<String>) -> Arc<Self> {
        Arc::new(Self {
            backend,
            state: Mutex::default(),
            runners: Mutex::default(),
            id,
        })
    }

    fn state(&self) -> Result<MutexGuard<'_, State>> {
        self.state.lock().map_err(|_| StorageError::Poisoned)
    }

    fn add_runner(&self, runner: Option<EngineRunner>) {
        if let Some(runner) = runner {
            self.runners
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(runner);
        }
    }

    /// Where NSH's storage is reachable: loads the mirror once and writes what waits. A waiting
    /// value the storage refuses (full) is dropped from the mirror, which takes the storage's
    /// value again; the first such refusal is the error. Elsewhere it does nothing.
    fn flush(&self) -> Result<()> {
        let backend = &self.backend;
        if !backend.reachable() {
            return Ok(());
        }
        let mut state = self.state()?;
        if !state.loaded {
            let mut values = BTreeMap::new();
            for key in backend.keys() {
                if key.starts_with(STORAGE_PREFIX)
                    && key != STORE_ID_KEY
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
        // The id goes back where something removed it (the app's TSX clearing NSH's storage),
        // so a surface starting later still finds this store.
        if let Some(id) = &self.id
            && backend.get(STORE_ID_KEY).is_none()
        {
            let _ = backend.set(STORE_ID_KEY, id);
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

    /// The mirror, flushed where NSH's storage is reachable; an error where it has not loaded.
    fn loaded(&self) -> Result<MutexGuard<'_, State>> {
        self.flush()?;
        let state = self.state()?;
        if !state.loaded {
            return Err(not_loaded());
        }
        Ok(state)
    }

    /// Writes `value` (or removes the key) at once where NSH's storage is reachable, else leaves
    /// it waiting and has an engine runner write it.
    fn write(
        self: &Arc<Self>,
        mut state: MutexGuard<'_, State>,
        key: String,
        value: Option<String>,
    ) -> Result<()> {
        let backend = &self.backend;
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
        self.flush_later();
        Ok(())
    }

    /// Has the first runner whose engine still runs write what waits; runners whose engine
    /// went are dropped. Once every engine is gone the writes wait for nothing more: they stay
    /// in the mirror.
    fn flush_later(self: &Arc<Self>) {
        let mut runners = self.runners.lock().unwrap_or_else(PoisonError::into_inner);
        while let Some(runner) = runners.first() {
            let shared = Arc::downgrade(self);
            let posted = runner.post(move || {
                if let Some(shared) = shared.upgrade() {
                    let _ = shared.flush();
                }
            });
            if posted.is_ok() {
                return;
            }
            runners.remove(0);
        }
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

    fn set_now(self: &Arc<Self>, bucket: &str, key: &str, value: &[u8]) -> Result<()> {
        let full = format!("{}{key}", bucket_prefix(bucket));
        let state = self.loaded()?;
        self.write(state, full, Some(encode(value)))
    }

    fn remove_now(self: &Arc<Self>, bucket: &str, key: &str) -> Result<bool> {
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

    fn clear_now(self: &Arc<Self>, bucket: &str) -> Result<()> {
        for key in self.keys_now(bucket)? {
            self.remove_now(bucket, &key)?;
        }
        Ok(())
    }

    /// Writes what waits as the app quits: when `suspension` publishes `Destroying` (before the
    /// app's own listeners, which subscribed later) and `Destroyed` (after them, so what they
    /// saved as they heard `Destroying` too).
    fn flush_on_quit(self: &Arc<Self>, suspension: &dyn SuspensionService) -> ChangeSubscription {
        let shared = Arc::downgrade(self);
        suspension.events().subscribe(move |event| {
            if matches!(
                event.kind,
                SuspensionEventKind::Destroying | SuspensionEventKind::Destroyed
            ) && let Some(shared) = shared.upgrade()
            {
                let _ = shared.flush();
            }
        })
    }
}

fn not_loaded() -> StorageError {
    StorageError::Backend("NSH's storage loads on the engine's thread: start the app first".into())
}

/// The process's stores over NSH's storages, by the id each keeps in its storage.
struct Registered {
    id: String,
    shared: Weak<Shared>,
    /// The store providers resolve, while one holds it.
    store: Weak<NshKeyValueStore>,
}

fn registry() -> MutexGuard<'static, Vec<Registered>> {
    static REGISTRY: Mutex<Vec<Registered>> = Mutex::new(Vec::new());
    REGISTRY.lock().unwrap_or_else(PoisonError::into_inner)
}

/// An id no other store has, in this run or another: 128 bits of the system's entropy (the
/// page's Web Crypto in a browser), else the wall clock, the process's hash keys and a count.
fn new_id() -> String {
    static MADE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let made = MADE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let mut random = [0u8; 16];
    if getrandom::fill(&mut random).is_ok() {
        let random: String = random.iter().map(|byte| format!("{byte:02x}")).collect();
        return format!("{random}{made:x}");
    }
    use std::hash::{BuildHasher as _, Hasher as _};
    let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
    hasher.write_u64(nsh_core::time::unix_ms().to_bits());
    #[cfg(not(target_arch = "wasm32"))]
    hasher.write_u32(std::process::id());
    hasher.write_u64(made);
    format!("{:016x}{made:x}", hasher.finish())
}

/// Where `backend` (NSH's storage) is reachable, under the registry's lock (so two surfaces
/// starting at once on their engine threads agree): the entry of the store whose id the storage
/// holds, `runner` added to its runners, or a new store's, its id written to the storage, its
/// mirror loaded.
fn join<'a>(
    registry: &'a mut Vec<Registered>,
    backend: &Arc<dyn Backend>,
    runner: Option<EngineRunner>,
) -> (&'a mut Registered, Arc<Shared>) {
    registry.retain(|entry| entry.shared.strong_count() > 0);
    let found = backend.get(STORE_ID_KEY).and_then(|id| {
        registry
            .iter()
            .position(|entry| entry.id == id)
            .and_then(|at| registry[at].shared.upgrade().map(|shared| (at, shared)))
    });
    let (at, shared) = match found {
        Some(found) => found,
        None => {
            let id = new_id();
            let shared = Shared::new(Arc::clone(backend), Some(id.clone()));
            // A storage too full for the id keeps the store to this surface: its flush writes
            // the id once there is room.
            let _ = backend.set(STORE_ID_KEY, &id);
            registry.push(Registered {
                id,
                shared: Arc::downgrade(&shared),
                store: Weak::new(),
            });
            (registry.len() - 1, shared)
        }
    };
    shared.add_runner(runner);
    // Loaded here, on the engine thread: what the surfaces read from now on.
    let _ = shared.flush();
    (&mut registry[at], shared)
}

/// RayX's [`KeyValueStore`] on NSH: values in NSH's storage under [`STORAGE_PREFIX`],
/// persistent wherever NSH keeps the app's storage. `add_storage()` registers it as the app's
/// `dyn KeyValueStore`, the same store for every surface over one NSH storage
/// ([`NshKeyValueStore::resolve`]); [`StorageStartup`] loads its mirror on the engine thread.
pub(crate) struct NshKeyValueStore {
    backend: Arc<dyn Backend>,
    /// The runner of the surface it was made for, which joins it to its storage's values where
    /// it was made away from them.
    runner: Option<EngineRunner>,
    /// Its values, shared with the other surfaces over its NSH storage, once it joined them.
    shared: Arc<OnceLock<Arc<Shared>>>,
}

impl NshKeyValueStore {
    /// The store of the NSH storage of the surface `runner` runs, for a provider of that
    /// surface: on its engine thread (where startup resolves it), the process's one store over
    /// that storage; elsewhere a store that joins that storage's values as it loads.
    pub(crate) fn resolve(runner: Option<EngineRunner>) -> Arc<Self> {
        Self::resolve_over(Arc::new(NshStorage), runner)
    }

    fn resolve_over(backend: Arc<dyn Backend>, runner: Option<EngineRunner>) -> Arc<Self> {
        if !backend.reachable() {
            return Arc::new(Self {
                backend,
                runner,
                shared: Arc::default(),
            });
        }
        let mut registry = registry();
        let (entry, shared) = join(&mut registry, &backend, runner.clone());
        if let Some(store) = entry.store.upgrade() {
            return store;
        }
        let store = Arc::new(Self {
            backend,
            runner,
            shared: Arc::new(OnceLock::from(shared)),
        });
        entry.store = Arc::downgrade(&store);
        store
    }

    /// A store over `backend` kept apart from every other (a test's).
    #[cfg(test)]
    pub(crate) fn over(backend: Box<dyn Backend>, runner: Option<EngineRunner>) -> Self {
        let backend: Arc<dyn Backend> = Arc::from(backend);
        let shared = Shared::new(Arc::clone(&backend), None);
        shared.add_runner(runner.clone());
        Self {
            backend,
            runner,
            shared: Arc::new(OnceLock::from(shared)),
        }
    }

    /// Where NSH's storage is reachable: loads the mirror once and writes what waits
    /// ([`Shared::flush`]). Elsewhere, or before it joined its storage's values, it does nothing.
    #[cfg(test)]
    pub(crate) fn flush(&self) -> Result<()> {
        self.shared.get().map_or(Ok(()), |shared| shared.flush())
    }

    /// Resolves with the values once the mirror loaded: at once where it has or NSH's storage
    /// is reachable, else through the engine runner; an error where neither can load it.
    fn load(&self) -> BoxFuture<'static, Result<Arc<Shared>>> {
        let reachable = self.backend.reachable();
        if let Some(shared) = self.shared.get() {
            let loaded = shared.state().map(|state| state.loaded);
            match loaded {
                Ok(true) => return futures::future::ready(Ok(Arc::clone(shared))).boxed(),
                Ok(false) => {}
                Err(error) => return futures::future::ready(Err(error)).boxed(),
            }
            if reachable {
                let shared = Arc::clone(shared);
                return futures::future::ready(shared.flush().map(|()| shared)).boxed();
            }
        } else if let Some(shared) = self.joined() {
            return futures::future::ready(shared.flush().map(|()| shared)).boxed();
        }
        let Some(runner) = self.runner.clone() else {
            return futures::future::ready(Err(not_loaded())).boxed();
        };
        let store = Self {
            backend: Arc::clone(&self.backend),
            runner: self.runner.clone(),
            shared: Arc::clone(&self.shared),
        };
        let flushed = runner.run(move || {
            let flushed = store
                .joined()
                .ok_or_else(not_loaded)
                .and_then(|shared| shared.flush().map(|()| shared));
            async move { flushed }
        });
        async move {
            flushed
                .await
                .map_err(|gone| StorageError::Backend(gone.to_string()))?
        }
        .boxed()
    }

    /// Its values: joined where NSH's storage is reachable (the values of the process's store
    /// over that storage), none elsewhere until then.
    fn joined(&self) -> Option<Arc<Shared>> {
        if let Some(shared) = self.shared.get() {
            return Some(Arc::clone(shared));
        }
        if !self.backend.reachable() {
            return None;
        }
        let mut registry = registry();
        let shared = self
            .shared
            .get_or_init(|| join(&mut registry, &self.backend, self.runner.clone()).1);
        Some(Arc::clone(shared))
    }

    /// Answers with `call` once the mirror loaded.
    fn answer<T: Send + 'static>(
        &self,
        call: impl FnOnce(&Arc<Shared>) -> Result<T> + Send + 'static,
    ) -> BoxFuture<'static, Result<T>> {
        let load = self.load();
        async move { call(&load.await?) }.boxed()
    }

    /// Writes what waits as the app quits ([`Shared::flush_on_quit`]), for as long as the store
    /// lives.
    #[cfg(test)]
    fn flush_on_quit(&self, suspension: &dyn SuspensionService) {
        if let Some(shared) = self.shared.get() {
            shared.flush_on_quit(suspension).detach();
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
        self.answer(move |shared| shared.get_now(&bucket, &key))
    }

    fn set(&self, bucket: String, key: String, value: Vec<u8>) -> BoxFuture<'_, Result<()>> {
        self.answer(move |shared| shared.set_now(&bucket, &key, &value))
    }

    fn remove(&self, bucket: String, key: String) -> BoxFuture<'_, Result<bool>> {
        self.answer(move |shared| shared.remove_now(&bucket, &key))
    }

    fn keys(&self, bucket: String) -> BoxFuture<'_, Result<Vec<String>>> {
        self.answer(move |shared| shared.keys_now(&bucket))
    }

    fn clear(&self, bucket: String) -> BoxFuture<'_, Result<()>> {
        self.answer(move |shared| shared.clear_now(&bucket))
    }
}

/// A provider's start of its store: the startup service `add_storage()` registers (keyed by
/// `dyn KeyValueStore`), one per provider, as the store is one for the surfaces over its NSH
/// storage. It loads the store's mirror on the engine thread and writes what waits as the
/// provider's app quits, heard through the provider's `dyn SuspensionService`.
pub(crate) struct StorageStartup {
    store: Arc<NshKeyValueStore>,
    /// What it hears of the app quitting, once it initialized: for the provider's life.
    quitting: Arc<Mutex<Option<ChangeSubscription>>>,
    initialization: StartupInitializationState,
}

impl StorageStartup {
    pub(crate) fn new(store: Arc<NshKeyValueStore>) -> Self {
        Self {
            store,
            quitting: Arc::default(),
            initialization: StartupInitializationState::default(),
        }
    }
}

impl AsyncInitializable for StorageStartup {
    fn is_initialized(&self) -> bool {
        self.initialization.is_initialized()
    }

    /// Loads the mirror (startup runs it on the engine thread) and listens for the app quitting
    /// through the app's `dyn SuspensionService`.
    fn initialize(
        &self,
        context: StartupInitializationContext,
    ) -> LocalBoxFuture<'static, StartupInitializationResult> {
        let load = self.store.load();
        let quitting = Arc::clone(&self.quitting);
        let initialization = self.initialization.clone();
        async move {
            let shared = load.await.map_err(|error| error.to_string())?;
            if let Some(suspension) = context.services().service::<dyn SuspensionService>() {
                let subscription = shared.flush_on_quit(suspension.as_ref());
                *quitting.lock().unwrap_or_else(PoisonError::into_inner) = Some(subscription);
            }
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

    /// One store per NSH storage in the process: the providers of the surfaces over one storage
    /// resolve one store.
    mod one_store_per_storage {
        use super::*;

        /// A surface over `fake`'s storage, reachable as `fake` says.
        fn resolve(fake: &Fake) -> Arc<NshKeyValueStore> {
            NshKeyValueStore::resolve_over(Arc::new(fake.clone()), None)
        }

        fn get(store: &NshKeyValueStore, key: &str) -> Option<Vec<u8>> {
            super::get(store, "prefs", key).unwrap()
        }

        #[test]
        fn surfaces_over_one_storage_resolve_one_store() {
            let fake = Fake::default();
            reachable(&fake, true);
            let first = resolve(&fake);
            let second = resolve(&fake);
            assert!(Arc::ptr_eq(&first, &second));
            set(&first, "prefs", "lang", b"hr").unwrap();
            reachable(&fake, false);
            assert_eq!(get(&second, "lang"), Some(b"hr".to_vec()));
            assert!(
                fake.entries.lock().unwrap().contains_key(STORE_ID_KEY),
                "the store's id is in the storage"
            );
            assert!(keys(&second, "prefs").iter().all(|key| key == "lang"));
        }

        #[test]
        fn surfaces_over_other_storages_keep_their_own() {
            let one = Fake::default();
            let other = Fake::default();
            reachable(&one, true);
            reachable(&other, true);
            let first = resolve(&one);
            let second = resolve(&other);
            assert!(!Arc::ptr_eq(&first, &second));
            set(&first, "prefs", "lang", b"hr").unwrap();
            assert_eq!(get(&second, "lang"), None);
        }

        #[test]
        fn a_store_made_away_from_its_storage_joins_its_values_as_it_loads() {
            let fake = Fake::default();
            reachable(&fake, true);
            let first = resolve(&fake);
            set(&first, "prefs", "lang", b"hr").unwrap();
            reachable(&fake, false);
            let late = resolve(&fake);
            assert!(get_err(&late), "nothing to read before it loads");
            reachable(&fake, true);
            assert_eq!(get(&late, "lang"), Some(b"hr".to_vec()));
            reachable(&fake, false);
            set(&late, "prefs", "lang", b"de").unwrap();
            assert_eq!(get(&first, "lang"), Some(b"de".to_vec()), "one mirror");
        }

        fn get_err(store: &NshKeyValueStore) -> bool {
            super::get(store, "prefs", "lang").is_err()
        }

        #[test]
        fn surfaces_starting_at_once_resolve_one_store() {
            let fake = Fake::default();
            reachable(&fake, true);
            let barrier = Arc::new(std::sync::Barrier::new(8));
            let stores: Vec<_> = (0..8)
                .map(|_| {
                    let fake = fake.clone();
                    let barrier = Arc::clone(&barrier);
                    std::thread::spawn(move || {
                        barrier.wait();
                        resolve(&fake)
                    })
                })
                .collect::<Vec<_>>()
                .into_iter()
                .map(|thread| thread.join().unwrap())
                .collect();
            assert!(stores.iter().all(|store| Arc::ptr_eq(store, &stores[0])));
        }

        #[test]
        fn the_id_goes_back_where_the_app_cleared_the_storage() {
            let fake = Fake::default();
            reachable(&fake, true);
            let first = resolve(&fake);
            // The app's TSX clears NSH's storage; the store's next call writes its id again.
            fake.entries.lock().unwrap().clear();
            assert!(keys(&first, "prefs").is_empty());
            assert!(Arc::ptr_eq(&resolve(&fake), &first));
        }
    }
}
