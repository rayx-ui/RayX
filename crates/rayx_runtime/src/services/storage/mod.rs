//! Key-value storage: values as bytes in named buckets, behind the [`KeyValueStore`] trait, with
//! JSON helpers ([`StorageBucket`]). This iteration has the in-memory store only
//! ([`InMemoryKeyValueStore`]); a persistent one implements the trait and is registered with
//! [`StorageAppBuilderExt::with_key_value_store`].

mod app_ext;
mod bucket;
mod error;
mod memory;
mod store;

pub use app_ext::StorageAppBuilderExt;
pub use bucket::StorageBucket;
pub use error::{Result, StorageError};
pub use memory::InMemoryKeyValueStore;
pub use store::{KeyValueStore, StorageCapabilities};
