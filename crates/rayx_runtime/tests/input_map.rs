//! The input map service: documents, layered resolution, the user's stored document and
//! registration.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::{Arc, Mutex};

use futures::executor::block_on;
use rayx_core::RayXAppBuilder;
use rayx_core::di::StartupInitializationContext;
use rayx_runtime::services::input_map::*;
use rayx_runtime::services::storage::{InMemoryKeyValueStore, KeyValueStore};
use serde_json::json;

fn key(sequence: &str) -> InputBinding {
    InputBinding::Keyboard(sequence.into())
}

fn service(store: Arc<dyn KeyValueStore>) -> InputMapService {
    let framework = InputMapDocument::new(
        InputMapSource::Framework,
        0,
        vec![
            InputMapEntry::keyboard("", "cmd-z", "edit::undo", None),
            InputMapEntry::keyboard("editor", "cmd-d", "editor::duplicate", None),
        ],
    );
    let application = InputMapDocument::new(
        InputMapSource::Application,
        0,
        vec![InputMapEntry::keyboard(
            "editor",
            "cmd-d",
            "editor::select_next",
            None,
        )],
    );
    let service = InputMapService::new(store, framework, application).unwrap();
    block_on(service.load()).unwrap();
    service
}

#[test]
fn document_round_trips_a_contextual_keyboard_binding() {
    let document = InputMapDocument::new(
        InputMapSource::User,
        7,
        vec![InputMapEntry::keyboard(
            "editor",
            "ctrl-k ctrl-c",
            "editor::comment",
            None,
        )],
    );
    let restored: InputMapDocument =
        serde_json::from_str(&serde_json::to_string(&document).unwrap()).unwrap();
    assert_eq!(restored, document);
}

#[test]
fn documents_reject_duplicates_and_malformed_bindings() {
    let duplicate = InputMapDocument::new(
        InputMapSource::User,
        0,
        vec![
            InputMapEntry::keyboard("editor", "ctrl-k", "a", None),
            InputMapEntry::keyboard("editor", "ctrl-k", "b", None),
        ],
    );
    assert_eq!(duplicate.validate(), Err(InputMapError::DuplicateBinding));
    for sequence in ["", "ctrl-", "hyper-k", "ctrl-k  ctrl-c"] {
        let document = InputMapDocument::new(
            InputMapSource::User,
            0,
            vec![InputMapEntry::keyboard("", sequence, "a", None)],
        );
        assert_eq!(
            document.validate(),
            Err(InputMapError::InvalidEntry),
            "{sequence:?}"
        );
    }
    for sequence in ["a", "ctrl--", "cmd-shift-p", "ctrl-k ctrl-c", "f12"] {
        assert!(valid_key_sequence(sequence), "{sequence:?}");
    }
}

#[test]
fn layers_and_contexts_resolve_innermost_and_highest_first() {
    let service = service(Arc::new(InMemoryKeyValueStore::new()));
    let resolved = service
        .resolve(&["editor", "workspace"], &key("cmd-d"))
        .unwrap();
    assert_eq!(resolved.action, "editor::select_next");
    assert_eq!(resolved.source, InputMapSource::Application);
    assert_eq!(
        service.resolve(&["editor"], &key("cmd-z")).unwrap().action,
        "edit::undo"
    );
    assert_eq!(service.resolve(&["workspace"], &key("cmd-d")), None);
}

#[test]
fn the_users_document_rebinds_unbinds_and_is_stored() {
    let store: Arc<dyn KeyValueStore> = Arc::new(InMemoryKeyValueStore::new());
    let service = service(Arc::clone(&store));
    let heard = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&heard);
    let _subscription =
        service.subscribe(move |document| sink.lock().unwrap().push(document.revision));

    let applied = block_on(service.apply_user_document(
        0,
        InputMapDocument::new(
            InputMapSource::User,
            0,
            vec![
                InputMapEntry::keyboard("editor", "cmd-e", "editor::duplicate", None),
                InputMapEntry {
                    unbind: true,
                    ..InputMapEntry::keyboard("", "cmd-z", "edit::undo", None)
                },
            ],
        ),
    ))
    .unwrap();
    assert_eq!(applied.revision, 1);
    assert_eq!(*heard.lock().unwrap(), vec![1]);
    assert_eq!(service.resolve(&["editor"], &key("cmd-z")), None);
    assert_eq!(
        service.resolve(&["editor"], &key("cmd-e")).unwrap().source,
        InputMapSource::User
    );
    let shortcuts: Vec<_> = service
        .bindings_for("editor::duplicate")
        .into_iter()
        .map(|entry| entry.binding)
        .collect();
    assert_eq!(shortcuts, vec![key("cmd-e")]);

    // A stale revision is refused and changes nothing.
    assert!(matches!(
        block_on(service.apply_user_document(
            0,
            InputMapDocument::new(InputMapSource::User, 0, Vec::new())
        )),
        Err(InputMapApplyError::RevisionConflict {
            expected: 0,
            actual: 1
        })
    ));
    assert_eq!(service.user_document(), applied);

    // A new service over the same store reads it back.
    assert_eq!(self::service(store).user_document(), applied);
}

#[test]
fn an_invalid_candidate_leaves_the_document_and_the_store_unchanged() {
    let store: Arc<dyn KeyValueStore> = Arc::new(InMemoryKeyValueStore::new());
    let service = service(Arc::clone(&store));
    let invalid = InputMapDocument::new(
        InputMapSource::User,
        0,
        vec![InputMapEntry::keyboard("", "", "a", None)],
    );
    assert!(block_on(service.apply_user_document(0, invalid)).is_err());
    assert_eq!(service.revision(), 0);
    assert_eq!(
        store
            .get(INPUT_MAP_STORAGE_BUCKET, "user-document")
            .unwrap(),
        None
    );
}

#[test]
fn add_input_map_binds_configuration_and_code_and_loads_on_start() {
    let mut builder = RayXAppBuilder::new().add_input_map(|options| {
        options.bind_key("editor", "cmd-s", "file::save");
    });
    builder.configuration.add_value(json!({
        "RayX": { "Services": { "InputMap": { "framework": [
            { "context": "", "binding": { "kind": "keyboard", "value": "cmd-q" }, "action": "app::quit" }
        ] } } }
    }));
    let services = builder.build().unwrap();
    services.validate_options_on_start().unwrap();
    block_on(
        services
            .startup_registry()
            .initialize(StartupInitializationContext::new(services.clone())),
    )
    .unwrap();
    let input_map = services.required_service::<InputMapService>().unwrap();
    assert_eq!(
        input_map
            .resolve(&["editor"], &key("cmd-s"))
            .unwrap()
            .action,
        "file::save"
    );
    assert_eq!(
        input_map.resolve(&[], &key("cmd-q")).unwrap().action,
        "app::quit"
    );
}

#[test]
fn duplicate_configured_bindings_fail_on_start() {
    let services = RayXAppBuilder::new()
        .add_input_map(|options| {
            options
                .bind_key("", "cmd-s", "a")
                .bind_key("", "cmd-s", "b");
        })
        .build()
        .unwrap();
    assert!(services.validate_options_on_start().is_err());
}
