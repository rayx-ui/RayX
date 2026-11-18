//! The key-value store for TSX.

use std::sync::Arc;

use nsh_core::JsonValue;
use nsh_core::prelude::*;
use rayx_app::services::storage::KeyValueStore as Store;
use rayx_app_nsh::ts::ServiceClass;

/// The app's key-value store, for TSX: `services.getRequiredService(KeyValueStore)`, values by
/// bucket and key, as text (`await store.get(..)`, `await store.set(..)`) or JSON (`getJson`,
/// `setJson`), over whichever `dyn KeyValueStore` is registered. With `addStorage()` they persist
/// in NSH's storage, beside the app's own `nsh/storage` entries.
#[class]
pub struct KeyValueStore {
    store: Arc<dyn Store>,
}

impl ServiceClass for KeyValueStore {
    type Service = dyn Store;

    fn wrap(store: Arc<dyn Store>) -> Self {
        Self { store }
    }
}

/// The store `this` wraps.
fn store(this: &Instance<KeyValueStore>) -> Arc<dyn Store> {
    this.with(|this| Arc::clone(&this.store))
}

#[class]
impl KeyValueStore {
    /// The contract this class is the token of (TS types only: `null` at run time).
    #[getter]
    #[ts("(): KeyValueStoreContract | null")]
    pub fn contract(&self) -> Option<bool> {
        None
    }

    /// What the store keeps its values in: `{ persistent, secure }`.
    #[getter]
    #[ts("(): StorageCapabilities")]
    pub fn capabilities(&self) -> JsonValue {
        rayx_app_nsh::ts::json(&self.store.capabilities())
    }

    /// The text at `key` in `bucket`, `null` when there is none.
    pub async fn get(
        this: Instance<Self>,
        bucket: String,
        key: String,
    ) -> Result<Option<String>, String> {
        store(&this)
            .get(bucket.clone(), key.clone())
            .await
            .map_err(|error| error.to_string())?
            .map(|bytes| {
                String::from_utf8(bytes).map_err(|_| format!("{bucket}/{key} holds no text"))
            })
            .transpose()
    }

    /// Sets the text at `key` in `bucket`.
    pub async fn set(
        this: Instance<Self>,
        bucket: String,
        key: String,
        value: String,
    ) -> Result<(), String> {
        store(&this)
            .set(bucket, key, value.into_bytes())
            .await
            .map_err(|error| error.to_string())
    }

    /// The JSON value at `key` in `bucket`, `null` when there is none.
    pub async fn get_json(
        this: Instance<Self>,
        bucket: String,
        key: String,
    ) -> Result<JsonValue, String> {
        store(&this)
            .bucket(bucket)
            .try_get_json::<JsonValue>(&key)
            .await
            .map(|value| value.unwrap_or(JsonValue::Null))
            .map_err(|error| error.to_string())
    }

    /// Sets `key` in `bucket` to `value` as JSON.
    pub async fn set_json(
        this: Instance<Self>,
        bucket: String,
        key: String,
        value: JsonValue,
    ) -> Result<(), String> {
        store(&this)
            .bucket(bucket)
            .set_json(&key, &value)
            .await
            .map_err(|error| error.to_string())
    }

    /// Removes `key` from `bucket`; whether it was there.
    pub async fn remove(this: Instance<Self>, bucket: String, key: String) -> Result<bool, String> {
        store(&this)
            .remove(bucket, key)
            .await
            .map_err(|error| error.to_string())
    }

    /// The keys of `bucket`, sorted.
    pub async fn keys(this: Instance<Self>, bucket: String) -> Result<Vec<String>, String> {
        store(&this)
            .keys(bucket)
            .await
            .map_err(|error| error.to_string())
    }

    /// Removes every value of `bucket`.
    pub async fn clear(this: Instance<Self>, bucket: String) -> Result<(), String> {
        store(&this)
            .clear(bucket)
            .await
            .map_err(|error| error.to_string())
    }
}
