use std::any::{Any, TypeId};
use std::borrow::Cow;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// A resolved service with its type erased: what a runtime-keyed factory makes and what
/// [`ServiceProvider::service_by_key`](super::ServiceProvider::service_by_key) returns.
pub type ErasedService = Arc<dyn Any + Send + Sync>;

/// What a service is registered and resolved under.
///
/// - [`ServiceKey::Type`]: a Rust type, as every typed registration (`add_singleton::<T>`,
///   `required_service::<T>`) uses.
/// - [`ServiceKey::Runtime`]: a key made at run time ([`RuntimeKey::new`]), for services that are
///   not Rust types (for example a service written in TypeScript).
///
/// The two never compare equal: a runtime key is one key space with the types, not a second
/// container.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum ServiceKey {
    Type(TypeId),
    Runtime(RuntimeKey),
}

impl ServiceKey {
    /// The key of the Rust type `T`.
    pub fn of<T>() -> Self
    where
        T: ?Sized + 'static,
    {
        Self::Type(TypeId::of::<T>())
    }
}

impl From<RuntimeKey> for ServiceKey {
    fn from(key: RuntimeKey) -> Self {
        Self::Runtime(key)
    }
}

impl From<&RuntimeKey> for ServiceKey {
    fn from(key: &RuntimeKey) -> Self {
        Self::Runtime(key.clone())
    }
}

/// A service key made at run time: an opaque id with a display name.
///
/// Every [`RuntimeKey::new`] makes a new key, even for a name used before; keys compare (and
/// hash) by id, and clones are the same key. The name is only for messages: errors and startup
/// failure reports name the service by it.
#[derive(Clone)]
pub struct RuntimeKey(Arc<RuntimeKeyInner>);

struct RuntimeKeyInner {
    id: u64,
    name: Cow<'static, str>,
}

impl RuntimeKey {
    pub fn new(name: impl Into<Cow<'static, str>>) -> Self {
        static NEXT_ID: AtomicU64 = AtomicU64::new(1);
        Self(Arc::new(RuntimeKeyInner {
            id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
            name: name.into(),
        }))
    }

    /// The display name the key was made with.
    pub fn name(&self) -> &str {
        &self.0.name
    }

    pub(crate) fn name_cow(&self) -> Cow<'static, str> {
        self.0.name.clone()
    }
}

impl PartialEq for RuntimeKey {
    fn eq(&self, other: &Self) -> bool {
        self.0.id == other.0.id
    }
}

impl Eq for RuntimeKey {}

impl Hash for RuntimeKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.id.hash(state);
    }
}

impl fmt::Debug for RuntimeKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RuntimeKey")
            .field("name", &self.name())
            .field("id", &self.0.id)
            .finish()
    }
}

impl fmt::Display for RuntimeKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.name())
    }
}
