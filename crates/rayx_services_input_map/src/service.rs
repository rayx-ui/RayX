//! RayX's input map: the framework's and the application's documents from its options and the
//! user's from the app's key-value store, published as [`InputMapState`].

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError, TryLockError};

use futures::FutureExt as _;
use futures::future::{BoxFuture, LocalBoxFuture};
use rayx_app::StateCell;
use rayx_app::di::{
    AsyncInitializable, StartupInitializable, StartupInitializationContext,
    StartupInitializationResult, StartupInitializationState, StartupServiceError,
    StartupServiceType,
};
use rayx_app::services::input_map::{
    InputMapApplyError, InputMapDocument, InputMapError, InputMapService, InputMapSource,
    InputMapState,
};
use rayx_app::services::storage::KeyValueStore;

/// The storage bucket the user's document is kept in.
pub const INPUT_MAP_STORAGE_BUCKET: &str = "rayx.input-map";
/// The key of the user's document in [`INPUT_MAP_STORAGE_BUCKET`].
pub const INPUT_MAP_USER_DOCUMENT_KEY: &str = "user-document";

/// RayX's input map: three layers, the framework's and the application's (from code and
/// configuration, never stored) and the user's, which the app's `dyn KeyValueStore` keeps (in
/// the bucket [`INPUT_MAP_STORAGE_BUCKET`]) and which goes over the other two.
pub(crate) struct RayxInputMap {
    store: Arc<dyn KeyValueStore>,
    /// The state, changed under its lock and published outside it.
    state: Mutex<InputMapState>,
    input_map: StateCell<InputMapState>,
    /// Held across a whole load or apply (its read or write of the store included), so one
    /// follows another and the store holds the document that is current.
    changing: futures::lock::Mutex<()>,
    /// Held by the thread publishing the state ([`RayxInputMap::publish_latest`]).
    publishing: Mutex<()>,
    /// A change was made since the publishing thread read the state.
    stale: AtomicBool,
    initialization: StartupInitializationState,
}

impl RayxInputMap {
    /// A service over `store` with the framework's and the application's documents; the user's
    /// is read from the store when the app starts.
    pub(crate) fn new(
        store: Arc<dyn KeyValueStore>,
        framework: InputMapDocument,
        application: InputMapDocument,
    ) -> Result<Arc<Self>, InputMapError> {
        if framework.source != InputMapSource::Framework
            || application.source != InputMapSource::Application
        {
            return Err(InputMapError::InvalidDocumentSource);
        }
        framework.validate()?;
        application.validate()?;
        let state = InputMapState {
            framework,
            application,
            ..InputMapState::default()
        };
        Ok(Arc::new(Self {
            store,
            input_map: StateCell::new(state.clone()),
            state: Mutex::new(state),
            changing: futures::lock::Mutex::new(()),
            publishing: Mutex::new(()),
            stale: AtomicBool::new(false),
            initialization: StartupInitializationState::default(),
        }))
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, InputMapState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Makes `user` the user's document (under [`changing`](Self::changing), so the store and
    /// the state agree); the caller publishes it once it let go of that lock.
    fn set_user(&self, user: InputMapDocument) {
        let mut state = self.lock();
        state.revision = user.revision;
        state.user = user;
    }

    /// Publishes the latest state when it differs from the published one, outside the state's
    /// lock (a subscriber may call the service). One thread publishes at a time and reads the
    /// state when it does, so the published state is never older than one published before it:
    /// a change made while another thread (or a subscriber of this one) publishes is published
    /// right after, by the publishing thread, once it finds the state stale.
    fn publish_latest(&self) {
        self.stale.store(true, Ordering::SeqCst);
        loop {
            let publishing = match self.publishing.try_lock() {
                Ok(guard) => guard,
                Err(TryLockError::Poisoned(poisoned)) => poisoned.into_inner(),
                Err(TryLockError::WouldBlock) => return,
            };
            self.stale.store(false, Ordering::SeqCst);
            let state = self.lock().clone();
            if *self.input_map.get() != state {
                self.input_map.publish(state);
            }
            drop(publishing);
            if !self.stale.load(Ordering::SeqCst) {
                return;
            }
        }
    }

    async fn read(&self) -> Result<InputMapDocument, InputMapApplyError> {
        let changing = self.changing.lock().await;
        let document = match self
            .store
            .get(
                INPUT_MAP_STORAGE_BUCKET.to_owned(),
                INPUT_MAP_USER_DOCUMENT_KEY.to_owned(),
            )
            .await?
        {
            Some(bytes) => {
                let document: InputMapDocument = serde_json::from_slice(&bytes)
                    .map_err(|error| InputMapError::Corrupt(error.to_string()))?;
                if document.source != InputMapSource::User {
                    return Err(InputMapError::UserDocumentRequired.into());
                }
                document.validate()?;
                document
            }
            None => InputMapDocument::new(InputMapSource::User, 0, Vec::new()),
        };
        self.set_user(document.clone());
        drop(changing);
        self.publish_latest();
        Ok(document)
    }

    async fn apply(
        &self,
        expected_revision: u64,
        mut candidate: InputMapDocument,
    ) -> Result<InputMapDocument, InputMapApplyError> {
        if candidate.source != InputMapSource::User {
            return Err(InputMapError::UserDocumentRequired.into());
        }
        let changing = self.changing.lock().await;
        let actual = self.lock().revision;
        if actual != expected_revision {
            return Err(InputMapApplyError::RevisionConflict {
                expected: expected_revision,
                actual,
            });
        }
        candidate.revision = expected_revision + 1;
        candidate.validate()?;
        let bytes = serde_json::to_vec(&candidate)
            .map_err(|error| InputMapApplyError::Serialize(error.to_string()))?;
        self.store
            .set(
                INPUT_MAP_STORAGE_BUCKET.to_owned(),
                INPUT_MAP_USER_DOCUMENT_KEY.to_owned(),
                bytes,
            )
            .await?;
        self.set_user(candidate.clone());
        drop(changing);
        self.publish_latest();
        Ok(candidate)
    }
}

impl InputMapService for RayxInputMap {
    fn input_map(&self) -> &StateCell<InputMapState> {
        &self.input_map
    }

    fn load(&self) -> BoxFuture<'_, Result<InputMapDocument, InputMapApplyError>> {
        self.read().boxed()
    }

    fn apply_user_document(
        &self,
        expected_revision: u64,
        candidate: InputMapDocument,
    ) -> BoxFuture<'_, Result<InputMapDocument, InputMapApplyError>> {
        self.apply(expected_revision, candidate).boxed()
    }
}

/// RayX's input map as a startup service under `dyn InputMapService`'s key: it reads the user's
/// document once the app's `dyn KeyValueStore` is ready.
pub(crate) struct Loading(pub(crate) Arc<RayxInputMap>);

impl AsyncInitializable for Loading {
    fn is_initialized(&self) -> bool {
        self.0.initialization.is_initialized()
    }

    fn initialize(
        &self,
        _context: StartupInitializationContext,
    ) -> LocalBoxFuture<'static, StartupInitializationResult> {
        let service = Arc::clone(&self.0);
        async move {
            service
                .read()
                .await
                .map_err(|error| StartupServiceError::new(error.to_string()))?;
            service.initialization.mark_initialized();
            Ok(())
        }
        .boxed_local()
    }
}

impl StartupInitializable for Loading {
    fn startup_service_type(&self) -> Option<StartupServiceType> {
        Some(StartupServiceType::of::<dyn InputMapService>())
    }

    fn initialization_depends_on(&self) -> Vec<StartupServiceType> {
        vec![StartupServiceType::of::<dyn KeyValueStore>()]
    }
}
