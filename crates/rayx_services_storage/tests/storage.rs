//! The key-value storage service: the in-memory store, buckets of JSON values and registration
//! (the later of `add_*` and the app's own store wins).

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::Arc;

use futures::executor::block_on;
use rayx_app::di::StartupInitializationContext;
use rayx_app::services::storage::{KeyValueStore, StorageError};
use rayx_app::testing::FakeKeyValueStore;
use rayx_app::{AppBuilder as _, RayXAppBuilder};
use rayx_services_storage::{InMemoryKeyValueStore, StorageAppBuilderExt as _};
use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Draft {
    title: String,
    words: u32,
}

fn get(store: &dyn KeyValueStore, bucket: &str, key: &str) -> Option<Vec<u8>> {
    block_on(store.get(bucket.into(), key.into())).unwrap()
}

fn set(store: &dyn KeyValueStore, bucket: &str, key: &str, value: &[u8]) {
    block_on(store.set(bucket.into(), key.into(), value.to_vec())).unwrap();
}

#[test]
fn buckets_keep_their_values_apart() {
    let store = InMemoryKeyValueStore::new();
    set(&store, "drafts", "a", b"one");
    set(&store, "settings", "a", b"two");
    assert_eq!(get(&store, "drafts", "a").as_deref(), Some(&b"one"[..]));
    assert_eq!(get(&store, "settings", "a").as_deref(), Some(&b"two"[..]));
    assert!(block_on(store.remove("drafts".into(), "a".into())).unwrap());
    assert!(!block_on(store.remove("drafts".into(), "a".into())).unwrap());
    assert_eq!(get(&store, "drafts", "a"), None);
    assert!(!store.capabilities().persistent);
}

#[test]
fn keys_are_sorted_and_clear_empties_one_bucket() {
    let store = InMemoryKeyValueStore::new();
    for key in ["b", "a", "c"] {
        set(&store, "letters", key, b"");
    }
    set(&store, "other", "x", b"");
    assert_eq!(
        block_on(store.keys("letters".into())).unwrap(),
        ["a", "b", "c"]
    );
    block_on(store.clear("letters".into())).unwrap();
    assert!(block_on(store.keys("letters".into())).unwrap().is_empty());
    assert_eq!(block_on(store.keys("other".into())).unwrap(), ["x"]);
}

#[test]
fn json_values_round_trip_through_a_bucket() {
    let store: &dyn KeyValueStore = &InMemoryKeyValueStore::new();
    let drafts = store.bucket("drafts");
    let draft = Draft {
        title: "Ideas".into(),
        words: 120,
    };
    block_on(async {
        drafts.set_json("today", &draft).await.unwrap();
        assert_eq!(drafts.get_json::<Draft>("today").await.unwrap(), draft);
        assert_eq!(
            drafts.try_get_json::<Draft>("tomorrow").await.unwrap(),
            None
        );
        assert!(matches!(
            drafts.get_json::<Draft>("tomorrow").await,
            Err(StorageError::MissingKey { .. })
        ));
        store
            .set("drafts".into(), "broken".into(), b"{".to_vec())
            .await
            .unwrap();
        assert!(matches!(
            drafts.get_json::<Draft>("broken").await,
            Err(StorageError::Serialization { .. })
        ));
        assert_eq!(drafts.keys().await.unwrap(), ["broken", "today"]);
        assert!(drafts.remove("broken").await.unwrap());
        drafts.clear().await.unwrap();
        assert!(drafts.keys().await.unwrap().is_empty());
    });
}

#[test]
fn add_in_memory_storage_registers_one_shared_store() {
    let services = RayXAppBuilder::new()
        .add_in_memory_storage()
        .build()
        .unwrap();
    let store = services.required_service::<dyn KeyValueStore>().unwrap();
    set(store.as_ref(), "b", "k", &[1]);
    let again = services
        .create_scope()
        .required_service::<dyn KeyValueStore>()
        .unwrap();
    assert_eq!(get(again.as_ref(), "b", "k"), Some(vec![1]));
    assert!(!again.capabilities().persistent);
}

#[test]
fn add_storage_registers_the_persistent_store_and_each_replaces_the_other() {
    let services = RayXAppBuilder::new().add_storage().build().unwrap();
    let store = services.required_service::<dyn KeyValueStore>().unwrap();
    assert!(store.capabilities().persistent);
    // Outside an NSH surface there is no storage to load.
    assert!(block_on(store.get("b".into(), "k".into())).is_err());

    let services = RayXAppBuilder::new()
        .add_storage()
        .add_in_memory_storage()
        .build()
        .unwrap();
    // The persistent store's startup went with it: startup runs without NSH.
    block_on(
        services
            .startup_registry()
            .initialize(StartupInitializationContext::new(services.clone())),
    )
    .unwrap();
    let store = services.required_service::<dyn KeyValueStore>().unwrap();
    assert!(!store.capabilities().persistent);

    let services = RayXAppBuilder::new()
        .add_in_memory_storage()
        .add_storage()
        .build()
        .unwrap();
    let store = services.required_service::<dyn KeyValueStore>().unwrap();
    assert!(store.capabilities().persistent);
}

#[test]
fn the_later_registration_of_a_store_wins() {
    let custom: Arc<dyn KeyValueStore> = FakeKeyValueStore::new();
    set(custom.as_ref(), "b", "k", &[7]);

    // `add_*` replaces a store the app registered before it.
    let mut builder = RayXAppBuilder::new();
    builder
        .services_mut()
        .replace_service::<dyn KeyValueStore>(Arc::clone(&custom));
    let services = builder.add_in_memory_storage().build().unwrap();
    let store = services.required_service::<dyn KeyValueStore>().unwrap();
    assert!(!Arc::ptr_eq(&store, &custom));
    assert_eq!(get(store.as_ref(), "b", "k"), None);

    // A store the app registers after it replaces its.
    let mut builder = RayXAppBuilder::new().add_in_memory_storage();
    builder
        .services_mut()
        .replace_service::<dyn KeyValueStore>(Arc::clone(&custom));
    let services = builder.build().unwrap();
    let store = services.required_service::<dyn KeyValueStore>().unwrap();
    assert!(Arc::ptr_eq(&store, &custom));
    assert_eq!(get(store.as_ref(), "b", "k"), Some(vec![7]));
}
