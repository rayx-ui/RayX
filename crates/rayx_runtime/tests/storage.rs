//! The key-value storage service: the in-memory store, buckets of JSON values and registration.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use rayx_core::RayXAppBuilder;
use rayx_runtime::services::storage::{
    InMemoryKeyValueStore, KeyValueStore, StorageAppBuilderExt, StorageError,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Draft {
    title: String,
    words: u32,
}

#[test]
fn buckets_keep_their_values_apart() {
    let store = InMemoryKeyValueStore::new();
    store.set("drafts", "a", b"one".to_vec()).unwrap();
    store.set("settings", "a", b"two".to_vec()).unwrap();
    assert_eq!(
        store.get("drafts", "a").unwrap().as_deref(),
        Some(&b"one"[..])
    );
    assert_eq!(
        store.get("settings", "a").unwrap().as_deref(),
        Some(&b"two"[..])
    );
    assert!(store.remove("drafts", "a").unwrap());
    assert!(!store.remove("drafts", "a").unwrap());
    assert_eq!(store.get("drafts", "a").unwrap(), None);
    assert!(!store.capabilities().persistent);
}

#[test]
fn keys_are_sorted_and_clear_empties_one_bucket() {
    let store = InMemoryKeyValueStore::new();
    for key in ["b", "a", "c"] {
        store.set("letters", key, Vec::new()).unwrap();
    }
    store.set("other", "x", Vec::new()).unwrap();
    assert_eq!(store.keys("letters").unwrap(), ["a", "b", "c"]);
    store.clear("letters").unwrap();
    assert!(store.keys("letters").unwrap().is_empty());
    assert_eq!(store.keys("other").unwrap(), ["x"]);
}

#[test]
fn json_values_round_trip_through_a_bucket() {
    let store: &dyn KeyValueStore = &InMemoryKeyValueStore::new();
    let drafts = store.bucket("drafts");
    let draft = Draft {
        title: "Odds".into(),
        words: 120,
    };
    drafts.set_json("today", &draft).unwrap();
    assert_eq!(drafts.get_json::<Draft>("today").unwrap(), draft);
    assert_eq!(drafts.try_get_json::<Draft>("tomorrow").unwrap(), None);
    assert!(matches!(
        drafts.get_json::<Draft>("tomorrow"),
        Err(StorageError::MissingKey { .. })
    ));
    store.set("drafts", "broken", b"{".to_vec()).unwrap();
    assert!(matches!(
        drafts.get_json::<Draft>("broken"),
        Err(StorageError::Serialization { .. })
    ));
    let read = futures::executor::block_on(async {
        drafts.set_json_async("later", &draft).await.unwrap();
        drafts.try_get_json_async::<Draft>("later").await.unwrap()
    });
    assert_eq!(read, Some(draft));
}

#[test]
fn add_storage_registers_one_shared_in_memory_store() {
    let services = RayXAppBuilder::new().add_storage().build().unwrap();
    let store = services.required_service::<dyn KeyValueStore>().unwrap();
    store.set("b", "k", vec![1]).unwrap();
    let again = services
        .create_scope()
        .required_service::<dyn KeyValueStore>()
        .unwrap();
    assert_eq!(again.get("b", "k").unwrap(), Some(vec![1]));
}

#[test]
fn a_registered_store_wins_over_add_storage() {
    let custom = InMemoryKeyValueStore::new();
    custom.set("b", "k", vec![7]).unwrap();
    let services = RayXAppBuilder::new()
        .with_key_value_store(custom)
        .add_storage()
        .build()
        .unwrap();
    let store = services.required_service::<dyn KeyValueStore>().unwrap();
    assert_eq!(store.get("b", "k").unwrap(), Some(vec![7]));
}
