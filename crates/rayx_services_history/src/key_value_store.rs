use crate::journal::HistoryJournalV2;
use crate::store::{HistoryStore, HistoryStoreError};
use futures::FutureExt as _;
use futures::future::{BoxFuture, LocalBoxFuture};
use rayx_app::di::{
    AsyncInitializable, StartupInitializable, StartupInitializationContext,
    StartupInitializationResult, StartupInitializationState, StartupServiceType,
};
use rayx_app::services::history::HistoryScopeId;
use rayx_app::services::storage::KeyValueStore;
use std::collections::HashMap;
use std::future::poll_fn;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError, Weak};
use std::task::{Poll, Waker};

/// The bucket of the app's [`KeyValueStore`] that [`KeyValueHistoryStore`] keeps journals in.
pub const HISTORY_BUCKET: &str = "rayx.history";

/// A [`HistoryStore`] over the app's `dyn KeyValueStore`: each scope's journal is one JSON value
/// in [`HISTORY_BUCKET`], keyed by the scope's id, so history persists wherever the store keeps
/// its values (NSH's storage on a device, through `rayx_services_storage`, or a TS
/// implementation's, which answers in JS's next turn). It initializes after the key-value
/// store.
///
/// A save or a delete checks the scope's stored revision and writes in one step, under the
/// process's one lock of the scope's journal (its bucket and the scope's id), whatever key-value
/// store object it goes through: histories of separate windows, surfaces and app instances in
/// one process over one storage (RayX's NSH store, `add_storage()`, one per provider, over the
/// NSH storage they share) never overwrite or delete a journal another wrote meanwhile.
/// Histories over unrelated stores that use one scope id take turns too. Across processes the
/// step is not atomic: a [`KeyValueStore`] has no compare step.
#[derive(Clone)]
pub struct KeyValueHistoryStore {
    store: Arc<dyn KeyValueStore>,
    initialization: StartupInitializationState,
}

/// The lock of a scope's journal: the one every `KeyValueHistoryStore` of the process holds while
/// it checks the scope's revision and writes, across their awaits, whichever key-value store
/// object it goes through. Other scopes never wait for it.
#[derive(Default)]
struct ScopeLock {
    state: Mutex<ScopeLockState>,
}

#[derive(Default)]
struct ScopeLockState {
    held: bool,
    /// The tasks waiting for it, each woken when it is released.
    waiting: Vec<Waker>,
}

impl ScopeLock {
    /// The lock of `scope`'s journal in `bucket`, one in the process.
    fn of(bucket: &str, scope: &HistoryScopeId) -> Arc<ScopeLock> {
        type Locks = HashMap<(String, String), Weak<ScopeLock>>;
        static LOCKS: OnceLock<Mutex<Locks>> = OnceLock::new();
        let mut locks = LOCKS
            .get_or_init(Mutex::default)
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let key = (bucket.to_owned(), scope.as_str().to_owned());
        if let Some(lock) = locks.get(&key).and_then(Weak::upgrade) {
            return lock;
        }
        // A lock no save or delete holds or waits for any more goes with the next new one.
        locks.retain(|_, lock| lock.strong_count() > 0);
        let lock = Arc::default();
        locks.insert(key, Arc::downgrade(&lock));
        lock
    }

    fn state(&self) -> MutexGuard<'_, ScopeLockState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Holds the lock, once no other holds it, until the guard drops.
    async fn hold(self: Arc<Self>) -> ScopeGuard {
        poll_fn(|context| {
            let mut state = self.state();
            if state.held {
                if !state
                    .waiting
                    .iter()
                    .any(|waker| waker.will_wake(context.waker()))
                {
                    state.waiting.push(context.waker().clone());
                }
                Poll::Pending
            } else {
                state.held = true;
                Poll::Ready(())
            }
        })
        .await;
        ScopeGuard(self)
    }
}

/// A held [`ScopeLock`], released when it drops.
struct ScopeGuard(Arc<ScopeLock>);

impl Drop for ScopeGuard {
    fn drop(&mut self) {
        let waiting = {
            let mut state = self.0.state();
            state.held = false;
            std::mem::take(&mut state.waiting)
        };
        // Woken with the lock's state released: a waker may poll its task on the spot (the
        // history's detached saves do), which takes the lock then. Every waiter is woken, so one
        // that was dropped meanwhile loses no other's turn.
        for waker in waiting {
            waker.wake();
        }
    }
}

impl KeyValueHistoryStore {
    /// Journals kept in `store`.
    pub fn new(store: Arc<dyn KeyValueStore>) -> Self {
        Self {
            store,
            initialization: StartupInitializationState::default(),
        }
    }

    async fn load_journal(
        &self,
        scope: &HistoryScopeId,
    ) -> Result<Option<HistoryJournalV2>, HistoryStoreError> {
        self.store
            .get(HISTORY_BUCKET.to_owned(), scope.as_str().to_owned())
            .await
            .map_err(backend)?
            .map(|bytes| {
                HistoryJournalV2::decode(&bytes).map_err(|error| {
                    backend(format!("the journal of {} is {error:?}", scope.as_str()))
                })
            })
            .transpose()
    }
}

fn backend(error: impl std::fmt::Display) -> HistoryStoreError {
    HistoryStoreError::Backend(error.to_string())
}

impl HistoryStore for KeyValueHistoryStore {
    fn load<'a>(
        &'a self,
        scope: &'a HistoryScopeId,
    ) -> BoxFuture<'a, Result<Option<HistoryJournalV2>, HistoryStoreError>> {
        self.load_journal(scope).boxed()
    }

    fn save(
        &self,
        journal: HistoryJournalV2,
        expected: Option<u64>,
    ) -> BoxFuture<'_, Result<(), HistoryStoreError>> {
        async move {
            let _saving = ScopeLock::of(HISTORY_BUCKET, &journal.scope_id)
                .hold()
                .await;
            let current = self
                .load_journal(&journal.scope_id)
                .await?
                .map(|journal| journal.revision);
            if current != expected {
                return Err(HistoryStoreError::Conflict);
            }
            let bytes = journal
                .encode()
                .map_err(|error| backend(format!("the journal did not encode: {error:?}")))?;
            self.store
                .set(
                    HISTORY_BUCKET.to_owned(),
                    journal.scope_id.as_str().to_owned(),
                    bytes,
                )
                .await
                .map_err(backend)
        }
        .boxed()
    }

    fn delete_scope<'a>(
        &'a self,
        scope: &'a HistoryScopeId,
        expected: u64,
    ) -> BoxFuture<'a, Result<bool, HistoryStoreError>> {
        async move {
            let _saving = ScopeLock::of(HISTORY_BUCKET, scope).hold().await;
            let current = self
                .load_journal(scope)
                .await?
                .map(|journal| journal.revision);
            if current != Some(expected) {
                return Ok(false);
            }
            self.store
                .remove(HISTORY_BUCKET.to_owned(), scope.as_str().to_owned())
                .await
                .map(|_| true)
                .map_err(backend)
        }
        .boxed()
    }
}

impl AsyncInitializable for KeyValueHistoryStore {
    fn is_initialized(&self) -> bool {
        self.initialization.is_initialized()
    }

    fn initialize(
        &self,
        _context: StartupInitializationContext,
    ) -> LocalBoxFuture<'static, StartupInitializationResult> {
        let initialization = self.initialization.clone();
        async move {
            initialization.mark_initialized();
            Ok(())
        }
        .boxed_local()
    }
}

impl StartupInitializable for KeyValueHistoryStore {
    fn startup_service_type(&self) -> Option<StartupServiceType> {
        Some(StartupServiceType::of::<dyn HistoryStore>())
    }

    fn initialization_depends_on(&self) -> Vec<StartupServiceType> {
        vec![StartupServiceType::of::<dyn KeyValueStore>()]
    }
}
