//! The services kept in the app's key-value store require one and register none: the input map
//! and persistent history fail the build without a `dyn KeyValueStore`, naming how to add one,
//! and start over the app's own store (no startup service), after it.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::Arc;

use futures::executor::block_on;
use rayx_app::di::StartupInitializationContext;
use rayx_app::testing::FakeKeyValueStore;
use rayx_app::{AppBuilder as _, RayXAppBuilder};
use rayx_runtime::services::history::{HistoryAppBuilderExt as _, HistoryStore};
use rayx_runtime::services::input_map::{
    INPUT_MAP_STORAGE_BUCKET, InputMapAppBuilderExt as _, InputMapDocument, InputMapService,
    InputMapSource,
};
use rayx_runtime::services::storage::KeyValueStore;

#[test]
fn without_a_store_the_build_names_what_to_add() {
    let Err(error) = RayXAppBuilder::new()
        .add_input_map(|_| {})
        .add_persistent_history()
        .build()
    else {
        panic!("the build fails");
    };
    let error = error.to_string();
    assert!(error.contains("InputMapService"), "{error}");
    assert!(error.contains("HistoryStore"), "{error}");
    assert!(error.contains("KeyValueStore"), "{error}");
    assert!(error.contains("add_storage()"), "{error}");
    assert!(error.contains("rayx_services_storage"), "{error}");
}

#[test]
fn neither_registers_a_store() {
    let mut builder = RayXAppBuilder::new()
        .add_input_map(|_| {})
        .add_persistent_history();
    assert!(
        !builder
            .services_mut()
            .contains_service::<dyn KeyValueStore>()
    );
}

#[test]
fn over_the_app_s_own_store_they_start() {
    let store = FakeKeyValueStore::new();
    block_on(store.set(
        INPUT_MAP_STORAGE_BUCKET.into(),
        "user-document".into(),
        serde_json::to_vec(&InputMapDocument::new(InputMapSource::User, 3, Vec::new())).unwrap(),
    ))
    .unwrap();
    let mut builder = RayXAppBuilder::new();
    builder
        .services_mut()
        .add_singleton_as::<dyn KeyValueStore>(Arc::clone(&store) as Arc<dyn KeyValueStore>);
    let services = builder
        .add_input_map(|_| {})
        .add_persistent_history()
        .build()
        .unwrap();
    block_on(
        services
            .startup_registry()
            .initialize(StartupInitializationContext::new(services.clone())),
    )
    .unwrap();
    let input_map = services.required_service::<InputMapService>().unwrap();
    assert_eq!(input_map.revision(), 3, "read from the app's store");
    assert!(services.required_service::<dyn HistoryStore>().is_ok());
}
