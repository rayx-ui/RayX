//! RayX's input map: the framework's and the application's documents (from its options and an
//! optional asset JSON) and the user's from the app's key-value store, published as
//! [`InputMapState`].

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
use rayx_app::services::ContractFailure;
use rayx_app::services::assets::AssetService;
use rayx_app::services::input_map::{
    INPUT_MAP_SCHEMA_VERSION, InputMapApplyError, InputMapDocument, InputMapEntry, InputMapError,
    InputMapService, InputMapSource, InputMapState,
};
use rayx_app::services::storage::KeyValueStore;
use serde::Deserialize;

/// The storage bucket the user's document is kept in.
pub const INPUT_MAP_STORAGE_BUCKET: &str = "rayx.input-map";
/// The key of the user's document in [`INPUT_MAP_STORAGE_BUCKET`].
pub const INPUT_MAP_USER_DOCUMENT_KEY: &str = "user-document";

fn default_schema_version() -> u32 {
    INPUT_MAP_SCHEMA_VERSION
}

fn default_application_source() -> InputMapSource {
    InputMapSource::Application
}

/// Accepts both a full [`InputMapDocument`] JSON (`{ schema_version, source, revision, entries }`)
/// and a concise `{ entries: [...] }` asset document for the application layer.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AssetInputMapDocument {
    #[serde(default = "default_schema_version")]
    schema_version: u32,
    #[serde(default = "default_application_source")]
    source: InputMapSource,
    #[serde(default)]
    revision: u64,
    #[serde(default)]
    entries: Vec<InputMapEntry>,
}

/// RayX's input map: three layers, the framework's and the application's (from code,
/// configuration, and an optional asset JSON; never stored) and the user's, which the app's
/// `dyn KeyValueStore` keeps (in the bucket [`INPUT_MAP_STORAGE_BUCKET`]) and which goes over
/// the other two.
pub(crate) struct RayxInputMap {
    store: Arc<dyn KeyValueStore>,
    assets: Option<Arc<dyn AssetService>>,
    application_asset: Option<String>,
    configured_application: Vec<InputMapEntry>,
    application_asset_loaded: AtomicBool,
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
    /// A service over `store` with the framework's and the application's documents (plus an
    /// optional application asset path loaded from `assets`); the asset and user documents are
    /// read when the app starts or when [`InputMapService::load`] is called.
    pub(crate) fn new(
        store: Arc<dyn KeyValueStore>,
        assets: Option<Arc<dyn AssetService>>,
        framework: InputMapDocument,
        application: InputMapDocument,
        application_asset: Option<String>,
    ) -> Result<Arc<Self>, InputMapError> {
        if framework.source != InputMapSource::Framework
            || application.source != InputMapSource::Application
        {
            return Err(InputMapError::InvalidDocumentSource);
        }
        framework.validate()?;
        application.validate()?;
        let configured_application = application.entries.clone();
        let state = InputMapState {
            framework,
            application,
            ..InputMapState::default()
        };
        Ok(Arc::new(Self {
            store,
            assets,
            application_asset,
            configured_application,
            application_asset_loaded: AtomicBool::new(false),
            input_map: StateCell::new(state.clone()),
            state: Mutex::new(state),
            changing: futures::lock::Mutex::new(()),
            publishing: Mutex::new(()),
            stale: AtomicBool::new(false),
            initialization: StartupInitializationState::default(),
        }))
    }

    pub(crate) fn has_application_asset(&self) -> bool {
        self.application_asset.is_some()
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

    /// Makes `application` the application's document (under [`changing`](Self::changing)).
    fn set_application(&self, application: InputMapDocument) {
        let mut state = self.lock();
        state.application = application;
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

    /// Loads `self.application_asset` once (under [`changing`](Self::changing)), validates and
    /// normalizes its entries, merges any code/config application entries on top, and updates
    /// `state.application`.
    async fn ensure_application_asset_loaded(&self) -> Result<(), InputMapApplyError> {
        let Some(path) = self.application_asset.as_deref() else {
            return Ok(());
        };
        if self.application_asset_loaded.load(Ordering::Acquire) {
            return Ok(());
        }
        let Some(assets) = self.assets.as_ref() else {
            return Err(InputMapApplyError::Failed(ContractFailure {
                contract: "InputMapService",
                member: "load",
                message: format!(
                    "application asset `{path}` requires a dyn AssetService (`add_assets` / `addAssets`)"
                ),
            }));
        };
        let text = assets.load_text(path.to_owned()).await.map_err(|error| {
            InputMapApplyError::Failed(ContractFailure {
                contract: "InputMapService",
                member: "load",
                message: format!("failed to load application asset `{path}`: {error}"),
            })
        })?;
        let mut document = parse_application_asset(&text)?;
        if !self.configured_application.is_empty() {
            for configured in &self.configured_application {
                let normalized = configured.binding.normalized();
                document.entries.retain(|existing| {
                    !(existing.context == configured.context
                        && existing.binding.normalized() == normalized)
                });
                let mut entry = configured.clone();
                entry.binding = normalized;
                document.entries.push(entry);
            }
            document.validate()?;
        }
        self.set_application(document);
        self.application_asset_loaded.store(true, Ordering::Release);
        Ok(())
    }

    async fn read(&self) -> Result<InputMapDocument, InputMapApplyError> {
        let changing = self.changing.lock().await;
        self.ensure_application_asset_loaded().await?;
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

fn parse_application_asset(text: &str) -> Result<InputMapDocument, InputMapApplyError> {
    let raw: AssetInputMapDocument =
        serde_json::from_str(text).map_err(|error| InputMapError::Corrupt(error.to_string()))?;
    if raw.source != InputMapSource::Application {
        return Err(InputMapError::InvalidDocumentSource.into());
    }
    let mut document = InputMapDocument {
        schema_version: raw.schema_version,
        source: raw.source,
        revision: raw.revision,
        entries: raw.entries,
    };
    document.validate()?;
    for entry in &mut document.entries {
        entry.binding = entry.binding.normalized();
    }
    Ok(document)
}

/// RayX's input map as a startup service under `dyn InputMapService`'s key: it reads any
/// configured application asset (once `dyn AssetService` is ready) and the user's document
/// once the app's `dyn KeyValueStore` is ready.
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
        let mut dependencies = vec![StartupServiceType::of::<dyn KeyValueStore>()];
        if self.0.has_application_asset() {
            dependencies.push(StartupServiceType::of::<dyn AssetService>());
        }
        dependencies
    }
}
