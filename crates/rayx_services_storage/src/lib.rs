//! RayX's key-value storage on NSH: RayX's implementations of
//! [`rayx_app::services::storage::KeyValueStore`]. `add_storage()` keeps the app's values in
//! NSH's storage (the device's: a file in the app's data directory, the browser's
//! `localStorage`), persistent and reachable from any thread (its calls to NSH go through the
//! surface's engine runner), written out as the app quits, one store per NSH storage in the
//! process (the surfaces over one storage share it); `add_in_memory_storage()` keeps them
//! in memory for the app's life ([`InMemoryKeyValueStore`]).
//!
//! - Rust: `RayXAppBuilder::new().add_storage()` ([`StorageAppBuilderExt`]), then
//!   `services.required_service::<dyn KeyValueStore>()`.
//! - TS: `new RayXAppBuilder().addStorage()` (or `addInMemoryStorage()`), with the class [`KeyValueStore`] over whichever
//!   `dyn KeyValueStore` is registered (the contract's token).
//! - The app's set: [`register`].

#![cfg_attr(test, allow(clippy::expect_used, clippy::unwrap_used))]

mod builder;
mod memory;
mod nsh_store;
mod storage_ts;

use nsh_core::element::{Registry, TsAlias};
use rayx_app::services::storage::*;
use rayx_app_nsh::ts::ContractRegistryExt as _;

pub use builder::{StorageAppBuilderExt, StorageBuilder};
pub use memory::InMemoryKeyValueStore;
pub use nsh_store::STORAGE_PREFIX;
pub use storage_ts::KeyValueStore;

rayx_app_nsh::contract_nsh!(rayx_app::services::storage::KeyValueStore as KeyValueStoreContract);

/// What a store keeps its values in, in TS.
pub const STORAGE_CAPABILITIES: TsAlias = TsAlias {
    name: "StorageCapabilities",
    ts: "{ persistent: boolean; secure: boolean }",
    doc: "What a key-value store keeps its values in: the JSON of Rust's `StorageCapabilities`.",
};

/// Adds the storage's TS side to an app's set: its class [`KeyValueStore`], the contract a TS
/// implementation follows (the TS interface `KeyValueStoreContract`, [`KeyValueStoreContract`]
/// in Rust), `builder.addStorage()` and `builder.addInMemoryStorage()`.
pub fn register(registry: &mut Registry) {
    registry
        .ts_alias(STORAGE_CAPABILITIES)
        .contract_class::<KeyValueStoreContract, KeyValueStore>()
        .class_extension::<StorageBuilder>();
}
