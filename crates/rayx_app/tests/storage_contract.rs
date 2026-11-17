//! The storage contract in the core, NSH-free: `rayx_app::testing::FakeKeyValueStore` registered
//! as `dyn KeyValueStore` in a plain provider, its commands answered at once, the bucket's JSON
//! helpers over it, and a failed call in the contract's error type.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::Arc;

use futures::executor::block_on;
use rayx_app::services::ContractFailure;
use rayx_app::services::storage::{KeyValueStore, StorageCapabilities, StorageError};
use rayx_app::testing::FakeKeyValueStore;
use rayx_app::{AppBuilder as _, RayXAppBuilder};
use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Theme {
    mode: String,
}

#[test]
fn the_fake_is_a_store_resolved_by_trait() {
    let mut builder = RayXAppBuilder::new();
    builder
        .services_mut()
        .add_singleton_as::<dyn KeyValueStore>(FakeKeyValueStore::with_capabilities(
            StorageCapabilities {
                persistent: true,
                secure: false,
            },
        ) as Arc<dyn KeyValueStore>);
    let services = builder.build().unwrap();
    let store = services.required_service::<dyn KeyValueStore>().unwrap();
    assert!(store.capabilities().persistent);
    block_on(async {
        store
            .set("prefs".into(), "lang".into(), b"hr".to_vec())
            .await
            .unwrap();
        assert_eq!(
            store.get("prefs".into(), "lang".into()).await.unwrap(),
            Some(b"hr".to_vec())
        );
        assert_eq!(store.keys("prefs".into()).await.unwrap(), ["lang"]);
        assert!(store.remove("prefs".into(), "lang".into()).await.unwrap());
        assert!(!store.remove("prefs".into(), "lang".into()).await.unwrap());
        store.clear("prefs".into()).await.unwrap();
    });
}

#[test]
fn buckets_keep_json_values() {
    let store: Arc<dyn KeyValueStore> = FakeKeyValueStore::new();
    let settings = store.bucket("settings");
    assert_eq!(settings.name(), "settings");
    block_on(async {
        let theme = Theme {
            mode: "dark".into(),
        };
        settings.set_json("theme", &theme).await.unwrap();
        assert_eq!(settings.get_json::<Theme>("theme").await.unwrap(), theme);
        assert_eq!(settings.try_get_json::<Theme>("none").await.unwrap(), None);
        assert!(matches!(
            settings.get_json::<Theme>("none").await,
            Err(StorageError::MissingKey { .. })
        ));
    });
}

#[test]
fn a_failed_call_is_a_storage_error() {
    let error = StorageError::from(ContractFailure {
        contract: "KeyValueStore",
        member: "get",
        message: "threw".into(),
    });
    assert!(matches!(error, StorageError::Failed(_)));
    assert_eq!(error.to_string(), "`KeyValueStore.get`: threw");
}
