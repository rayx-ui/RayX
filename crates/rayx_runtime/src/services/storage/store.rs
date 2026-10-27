use super::{Result, StorageBucket};
use futures::FutureExt as _;
use futures::future::BoxFuture;
use rayx_core::AppService;

/// What a store keeps its values in.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct StorageCapabilities {
    /// Values outlive the process.
    pub persistent: bool,
    /// Values are encrypted at rest (a keychain, a keystore).
    pub secure: bool,
}

/// A key-value store: values are bytes, under a key in a named bucket. Implementations are
/// shared across threads; the async methods default to the sync ones, and a store whose backend
/// is asynchronous overrides them.
pub trait KeyValueStore: Send + Sync + 'static {
    /// What the store keeps its values in.
    fn capabilities(&self) -> StorageCapabilities;

    /// The value at `key` in `bucket`.
    fn get(&self, bucket: &str, key: &str) -> Result<Option<Vec<u8>>>;

    /// Sets the value at `key` in `bucket`.
    fn set(&self, bucket: &str, key: &str, value: Vec<u8>) -> Result<()>;

    /// Removes the value at `key` in `bucket`; whether there was one.
    fn remove(&self, bucket: &str, key: &str) -> Result<bool>;

    /// The keys in `bucket`, sorted.
    fn keys(&self, bucket: &str) -> Result<Vec<String>>;

    /// Removes every value in `bucket`.
    fn clear(&self, bucket: &str) -> Result<()>;

    /// [`get`](Self::get), awaited.
    fn get_async<'a>(
        &'a self,
        bucket: &'a str,
        key: &'a str,
    ) -> BoxFuture<'a, Result<Option<Vec<u8>>>> {
        async move { self.get(bucket, key) }.boxed()
    }

    /// [`set`](Self::set), awaited.
    fn set_async<'a>(
        &'a self,
        bucket: &'a str,
        key: &'a str,
        value: Vec<u8>,
    ) -> BoxFuture<'a, Result<()>> {
        async move { self.set(bucket, key, value) }.boxed()
    }

    /// [`remove`](Self::remove), awaited.
    fn remove_async<'a>(&'a self, bucket: &'a str, key: &'a str) -> BoxFuture<'a, Result<bool>> {
        async move { self.remove(bucket, key) }.boxed()
    }
}

impl AppService for dyn KeyValueStore {}

impl dyn KeyValueStore {
    /// The bucket `name`, for JSON values.
    pub fn bucket(&self, name: impl Into<String>) -> StorageBucket<'_> {
        StorageBucket::new(self, name.into())
    }
}
