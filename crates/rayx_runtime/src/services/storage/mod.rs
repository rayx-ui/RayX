//! Key-value storage: values as bytes in named buckets, behind the [`KeyValueStore`] trait, with
//! JSON helpers ([`StorageBucket`]). [`InMemoryKeyValueStore`] keeps them for the app's life;
//! a persistent store implements the trait and is registered with
//! [`StorageAppBuilderExt::with_key_value_store`] (on NSH, `rayx_app`'s `NshKeyValueStore`
//! keeps them in NSH's storage).

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
