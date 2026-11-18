//! RayX's input map changed from many threads at once: each applies the user's document at the
//! revision it read, and one of those at a revision wins. Whatever the interleaving, the service
//! publishes its states in the order it changed them (never an older revision after a newer),
//! the state it publishes last is its latest one, what a subscriber heard last is that state,
//! and the store holds the document that is current.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::{Arc, Barrier, Mutex};
use std::thread;

use futures::executor::block_on;
use rayx_app::di::StartupInitializationContext;
use rayx_app::services::input_map::{
    InputMapApplyError, InputMapDocument, InputMapEntry, InputMapService, InputMapSource,
    InputMapState,
};
use rayx_app::services::storage::KeyValueStore;
use rayx_app::testing::FakeKeyValueStore;
use rayx_app::{AppBuilder as _, RayXAppBuilder};
use rayx_services_input_map::{
    INPUT_MAP_STORAGE_BUCKET, INPUT_MAP_USER_DOCUMENT_KEY, InputMapAppBuilderExt as _,
};

const THREADS: usize = 6;
const APPLIES: usize = 40;

/// RayX's input map over a store in memory, started.
fn start() -> (Arc<dyn InputMapService>, Arc<dyn KeyValueStore>) {
    let store: Arc<dyn KeyValueStore> = FakeKeyValueStore::new();
    let mut builder = RayXAppBuilder::new();
    builder
        .services_mut()
        .add_singleton_as::<dyn KeyValueStore>(Arc::clone(&store));
    let services = builder.add_input_map(|_| {}).build().unwrap();
    block_on(
        services
            .startup_registry()
            .initialize(StartupInitializationContext::new(services.clone())),
    )
    .unwrap();
    (
        services.required_service::<dyn InputMapService>().unwrap(),
        store,
    )
}

/// A user's document binding `ctrl-<n>` to `worker<w>`.
fn document(worker: usize, n: usize) -> InputMapDocument {
    InputMapDocument::new(
        InputMapSource::User,
        0,
        vec![InputMapEntry::keyboard(
            "",
            format!("ctrl-{n}"),
            format!("worker{worker}"),
            None,
        )],
    )
}

#[test]
fn concurrent_applies_leave_the_latest_state_published_and_stored() {
    for _ in 0..20 {
        let (input_map, store) = start();
        let heard = Arc::new(Mutex::new(None::<Arc<InputMapState>>));
        let older = Arc::new(Mutex::new(Vec::new()));
        let (last, out_of_order) = (Arc::clone(&heard), Arc::clone(&older));
        let _subscription = input_map.input_map().subscribe(move |state| {
            let mut last = last.lock().unwrap();
            if let Some(before) = last
                .as_ref()
                .filter(|before| before.revision >= state.revision)
            {
                out_of_order
                    .lock()
                    .unwrap()
                    .push((before.revision, state.revision));
            }
            *last = Some(Arc::clone(state));
        });

        let barrier = Arc::new(Barrier::new(THREADS));
        let handles: Vec<_> = (0..THREADS)
            .map(|worker| {
                let input_map = Arc::clone(&input_map);
                let barrier = Arc::clone(&barrier);
                thread::spawn(move || {
                    barrier.wait();
                    let mut applied = 0;
                    for n in 0..APPLIES {
                        let revision = input_map.revision();
                        match block_on(input_map.apply_user_document(revision, document(worker, n)))
                        {
                            Ok(document) => {
                                assert_eq!(document.revision, revision + 1);
                                applied += 1;
                            }
                            Err(InputMapApplyError::RevisionConflict { .. }) => {}
                            Err(error) => panic!("{error}"),
                        }
                    }
                    applied
                })
            })
            .collect();
        let applied: u64 = handles.into_iter().map(|h| h.join().unwrap()).sum();

        assert_eq!(
            *older.lock().unwrap(),
            [],
            "states published after a newer one"
        );
        let published = input_map.input_map().get();
        assert_eq!(
            published.revision, applied,
            "one revision per document applied"
        );
        assert_eq!(published.user.revision, published.revision);
        let heard_last = heard.lock().unwrap().clone().unwrap();
        assert_eq!(
            heard_last, published,
            "a subscriber's last state is the cell's"
        );
        let stored: InputMapDocument = serde_json::from_slice(
            &block_on(store.get(
                INPUT_MAP_STORAGE_BUCKET.into(),
                INPUT_MAP_USER_DOCUMENT_KEY.into(),
            ))
            .unwrap()
            .unwrap(),
        )
        .unwrap();
        assert_eq!(
            stored, published.user,
            "the store holds the current document"
        );
        assert_eq!(block_on(input_map.load()).unwrap(), published.user);
    }
}
