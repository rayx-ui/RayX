use std::sync::{Arc, Mutex};

use futures::FutureExt as _;
use futures::future::LocalBoxFuture;
use rayx_core::ChangeSubscription;
use rayx_core::change::Listeners;
use rayx_core::di::{
    AppService, AsyncInitializable, StartupInitializable, StartupInitializationContext,
    StartupInitializationResult, StartupInitializationState, StartupServiceError,
    StartupServiceType,
};

use super::{InputBinding, InputMapDocument, InputMapEntry, InputMapError, InputMapSource};
use crate::services::storage::{KeyValueStore, StorageError};

/// The storage bucket and key the user's document is kept at.
pub const INPUT_MAP_STORAGE_BUCKET: &str = "rayx.input-map";
const USER_DOCUMENT_KEY: &str = "user-document";

/// What a binding resolved to: the action to run, with its arguments.
#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedInputAction {
    pub action: String,
    pub arguments: Option<serde_json::Value>,
    /// The context of the entry that matched.
    pub context: String,
    /// The layer the entry came from.
    pub source: InputMapSource,
}

/// Why the user's document was not applied.
#[derive(Debug, thiserror::Error)]
pub enum InputMapApplyError {
    #[error("the user's input map is at revision {actual}, not {expected}")]
    RevisionConflict { expected: u64, actual: u64 },
    #[error(transparent)]
    Model(#[from] InputMapError),
    #[error("the user's input map did not serialize: {0}")]
    Serialize(String),
    #[error("the user's input map was not stored: {0}")]
    Storage(#[from] StorageError),
}

type InputMapListener = dyn Fn(&InputMapDocument) + Send + Sync;

/// The app's input map: which action a key sequence or pointer gesture runs in a context.
///
/// Three layers: the framework's and the application's documents (from code and
/// configuration, never stored) and the user's, which the app's [`KeyValueStore`] keeps (in
/// the bucket [`INPUT_MAP_STORAGE_BUCKET`]) and which goes over the other two. A user entry
/// with `unbind` removes the binding the layers below give its context. Contexts are tried
/// innermost first ([`resolve`](Self::resolve)); the context `""` applies everywhere.
#[derive(Clone, AppService)]
#[app_service(startup)]
pub struct InputMapService {
    store: Arc<dyn KeyValueStore>,
    framework: Arc<InputMapDocument>,
    application: Arc<InputMapDocument>,
    user: Arc<Mutex<InputMapDocument>>,
    listeners: Listeners<InputMapListener>,
    initialization: StartupInitializationState,
}

impl InputMapService {
    /// A service over `store` with the framework's and the application's documents; the
    /// user's is read from the store when the app starts.
    pub fn new(
        store: Arc<dyn KeyValueStore>,
        framework: InputMapDocument,
        application: InputMapDocument,
    ) -> Result<Self, InputMapError> {
        if framework.source != InputMapSource::Framework
            || application.source != InputMapSource::Application
        {
            return Err(InputMapError::InvalidDocumentSource);
        }
        framework.validate()?;
        application.validate()?;
        Ok(Self {
            store,
            framework: Arc::new(framework),
            application: Arc::new(application),
            user: Arc::new(Mutex::new(InputMapDocument::new(
                InputMapSource::User,
                0,
                Vec::new(),
            ))),
            listeners: Listeners::default(),
            initialization: StartupInitializationState::default(),
        })
    }

    /// Reads the user's document from the store (at startup, or again later).
    pub async fn load(&self) -> Result<InputMapDocument, InputMapApplyError> {
        let document = match self
            .store
            .get_async(INPUT_MAP_STORAGE_BUCKET, USER_DOCUMENT_KEY)
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
        *self.lock_user() = document.clone();
        self.notify(&document);
        Ok(document)
    }

    /// The user's document.
    pub fn user_document(&self) -> InputMapDocument {
        self.lock_user().clone()
    }

    /// The user's document's revision.
    pub fn revision(&self) -> u64 {
        self.lock_user().revision
    }

    /// The framework's document.
    pub fn framework_document(&self) -> &InputMapDocument {
        &self.framework
    }

    /// The application's document.
    pub fn application_document(&self) -> &InputMapDocument {
        &self.application
    }

    /// Replaces the user's document with `candidate`, when the current one is at
    /// `expected_revision`: the candidate gets the next revision, is validated and stored, and
    /// only then becomes current. A failure leaves the current document as it was.
    pub async fn apply_user_document(
        &self,
        expected_revision: u64,
        mut candidate: InputMapDocument,
    ) -> Result<InputMapDocument, InputMapApplyError> {
        if candidate.source != InputMapSource::User {
            return Err(InputMapError::UserDocumentRequired.into());
        }
        let actual = self.revision();
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
            .set_async(INPUT_MAP_STORAGE_BUCKET, USER_DOCUMENT_KEY, bytes)
            .await?;
        {
            let mut user = self.lock_user();
            if user.revision != expected_revision {
                return Err(InputMapApplyError::RevisionConflict {
                    expected: expected_revision,
                    actual: user.revision,
                });
            }
            *user = candidate.clone();
        }
        self.notify(&candidate);
        Ok(candidate)
    }

    /// The action `binding` runs in the innermost of `contexts` (innermost first) that binds
    /// it, the user's layer over the application's over the framework's; `""` is tried last.
    pub fn resolve(
        &self,
        contexts: &[&str],
        binding: &InputBinding,
    ) -> Option<ResolvedInputAction> {
        let user = self.user_document();
        let layers = [
            (InputMapSource::User, &user),
            (InputMapSource::Application, &*self.application),
            (InputMapSource::Framework, &*self.framework),
        ];
        let global = [""];
        for context in contexts
            .iter()
            .chain(global.iter().filter(|_| !contexts.contains(&"")))
        {
            for (source, document) in layers {
                if let Some(entry) = document
                    .entries
                    .iter()
                    .find(|entry| entry.context == *context && entry.binding == *binding)
                {
                    if entry.unbind {
                        return None;
                    }
                    return Some(ResolvedInputAction {
                        action: entry.action.clone(),
                        arguments: entry.arguments.clone(),
                        context: entry.context.clone(),
                        source,
                    });
                }
            }
        }
        None
    }

    /// The bindings that run `action` (in any context), for showing shortcuts: each layer's,
    /// minus those a higher layer rebinds or unbinds in the same context.
    pub fn bindings_for(&self, action: &str) -> Vec<InputMapEntry> {
        let user = self.user_document();
        let layers = [&user, &*self.application, &*self.framework];
        let mut seen = Vec::<(&str, &InputBinding)>::new();
        let mut found = Vec::new();
        for document in layers {
            for entry in &document.entries {
                let key = (entry.context.as_str(), &entry.binding);
                if seen.contains(&key) {
                    continue;
                }
                seen.push(key);
                if !entry.unbind && entry.action == action {
                    found.push(entry.clone());
                }
            }
        }
        found
    }

    /// Calls `listener` with the user's document whenever it changes, until the subscription
    /// is dropped.
    pub fn subscribe(
        &self,
        listener: impl Fn(&InputMapDocument) + Send + Sync + 'static,
    ) -> ChangeSubscription {
        self.listeners.add(Arc::new(listener))
    }

    fn notify(&self, document: &InputMapDocument) {
        self.listeners.notify(|listener| listener(document));
    }

    fn lock_user(&self) -> std::sync::MutexGuard<'_, InputMapDocument> {
        self.user
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

impl AsyncInitializable for InputMapService {
    fn is_initialized(&self) -> bool {
        self.initialization.is_initialized()
    }

    fn initialize(
        &self,
        _context: StartupInitializationContext,
    ) -> LocalBoxFuture<'static, StartupInitializationResult> {
        let service = self.clone();
        async move {
            service
                .load()
                .await
                .map_err(|error| StartupServiceError::new(error.to_string()))?;
            service.initialization.mark_initialized();
            Ok(())
        }
        .boxed_local()
    }
}

impl StartupInitializable for InputMapService {
    fn startup_service_type(&self) -> StartupServiceType {
        StartupServiceType::of::<Self>()
    }
}
