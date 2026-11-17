//! The key-value store for TSX.

use std::sync::Arc;

use nsh_core::JsonValue;
use nsh_core::prelude::*;
use rayx_runtime::services::storage::KeyValueStore as Store;

use crate::di::provider_ts::ServiceClass;

/// The app's key-value store, for TSX: `services.getRequiredService(KeyValueStore)`, values by
/// bucket and key, as text (`get`, `set`) or JSON (`getJson`, `setJson`). On NSH they persist in
/// NSH's storage, beside the app's own `nsh/storage` entries.
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

#[class]
impl KeyValueStore {
    /// The text at `key` in `bucket`, `null` when there is none.
    pub fn get(&self, bucket: String, key: String) -> Result<Option<String>, String> {
        self.store
            .get(&bucket, &key)
            .map_err(|error| error.to_string())?
            .map(|bytes| {
                String::from_utf8(bytes).map_err(|_| format!("{bucket}/{key} holds no text"))
            })
            .transpose()
    }

    /// Sets the text at `key` in `bucket`.
    pub fn set(&self, bucket: String, key: String, value: String) -> Result<(), String> {
        self.store
            .set(&bucket, &key, value.into_bytes())
            .map_err(|error| error.to_string())
    }

    /// The JSON value at `key` in `bucket`, `null` when there is none.
    pub fn get_json(&self, bucket: String, key: String) -> Result<JsonValue, String> {
        self.store
            .bucket(bucket)
            .try_get_json::<JsonValue>(&key)
            .map(|value| value.unwrap_or(JsonValue::Null))
            .map_err(|error| error.to_string())
    }

    /// Sets `key` in `bucket` to `value` as JSON.
    pub fn set_json(&self, bucket: String, key: String, value: JsonValue) -> Result<(), String> {
        self.store
            .bucket(bucket)
            .set_json(&key, &value)
            .map_err(|error| error.to_string())
    }

    /// Removes `key` from `bucket`; whether it was there.
    pub fn remove(&self, bucket: String, key: String) -> Result<bool, String> {
        self.store
            .remove(&bucket, &key)
            .map_err(|error| error.to_string())
    }

    /// The keys of `bucket`, sorted.
    pub fn keys(&self, bucket: String) -> Result<Vec<String>, String> {
        self.store.keys(&bucket).map_err(|error| error.to_string())
    }

    /// Removes every value of `bucket`.
    pub fn clear(&self, bucket: String) -> Result<(), String> {
        self.store.clear(&bucket).map_err(|error| error.to_string())
    }
}
