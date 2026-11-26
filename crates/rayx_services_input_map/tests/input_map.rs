//! RayX's input map: documents, layered resolution, the user's stored document and
//! registration (the later of `add_input_map` and the app's own `dyn InputMapService` wins).

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::{Arc, Mutex};

use futures::executor::block_on;
use rayx_app::di::StartupInitializationContext;
use rayx_app::services::input_map::*;
use rayx_app::services::storage::KeyValueStore;
use rayx_app::testing::{FakeInputMap, FakeKeyValueStore};
use rayx_app::{AppBuilder as _, RayXAppBuilder, ServiceProvider};
use rayx_services_input_map::{
    INPUT_MAP_STORAGE_BUCKET, InputMapAppBuilderExt as _, InputMapOptions,
};
use serde_json::json;

/// `builder` with a key-value store, which the input map requires.
fn with_store(mut builder: RayXAppBuilder) -> RayXAppBuilder {
    builder
        .services_mut()
        .add_singleton_as::<dyn KeyValueStore>(FakeKeyValueStore::new() as Arc<dyn KeyValueStore>);
    builder
}

fn key(sequence: &str) -> InputBinding {
    InputBinding::Keyboard(sequence.into())
}

/// Starts an app over `store` with RayX's input map, its options set by `configure`.
fn start(
    store: Arc<dyn KeyValueStore>,
    configure: impl Fn(&mut InputMapOptions) + Send + Sync + 'static,
) -> ServiceProvider {
    let mut builder = RayXAppBuilder::new();
    builder
        .services_mut()
        .add_singleton_as::<dyn KeyValueStore>(store);
    let services = builder.add_input_map(configure).build().unwrap();
    services.validate_options_on_start().unwrap();
    block_on(
        services
            .startup_registry()
            .initialize(StartupInitializationContext::new(services.clone())),
    )
    .unwrap();
    services
}

/// RayX's input map over `store`, started, with the framework's and the application's bindings.
fn service(store: Arc<dyn KeyValueStore>) -> Arc<dyn InputMapService> {
    start(store, |options| {
        options.framework = vec![
            InputMapEntry::keyboard("", "cmd-z", "edit::undo", None),
            InputMapEntry::keyboard("editor", "cmd-d", "editor::duplicate", None),
        ];
        options.application = vec![InputMapEntry::keyboard(
            "editor",
            "cmd-d",
            "editor::select_next",
            None,
        )];
    })
    .required_service::<dyn InputMapService>()
    .unwrap()
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
    let service = service(FakeKeyValueStore::new());
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
    let store: Arc<dyn KeyValueStore> = FakeKeyValueStore::new();
    let service = service(Arc::clone(&store));
    let heard = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&heard);
    let _subscription = service
        .input_map()
        .subscribe(move |state| sink.lock().unwrap().push(state.user.revision));

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
    let store: Arc<dyn KeyValueStore> = FakeKeyValueStore::new();
    let service = service(Arc::clone(&store));
    let invalid = InputMapDocument::new(
        InputMapSource::User,
        0,
        vec![InputMapEntry::keyboard("", "", "a", None)],
    );
    assert!(block_on(service.apply_user_document(0, invalid)).is_err());
    assert_eq!(service.revision(), 0);
    assert_eq!(
        block_on(store.get(INPUT_MAP_STORAGE_BUCKET.into(), "user-document".into())).unwrap(),
        None
    );
}

#[test]
fn add_input_map_binds_configuration_and_code_and_loads_on_start() {
    let mut builder = with_store(RayXAppBuilder::new()).add_input_map(|options| {
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
    let input_map = services.required_service::<dyn InputMapService>().unwrap();
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
    let services = with_store(RayXAppBuilder::new())
        .add_input_map(|options| {
            options
                .bind_key("", "cmd-s", "a")
                .bind_key("", "cmd-s", "b");
        })
        .build()
        .unwrap();
    assert!(services.validate_options_on_start().is_err());
}

/// Initializes `services`' startup services.
fn initialize(services: &ServiceProvider) {
    block_on(
        services
            .startup_registry()
            .initialize(StartupInitializationContext::new(services.clone())),
    )
    .unwrap();
}

#[test]
fn the_later_registration_of_an_input_map_wins() {
    let own: Arc<dyn InputMapService> = FakeInputMap::new(InputMapState::default());

    // `add_input_map` replaces an input map the app registered before it.
    let mut builder = with_store(RayXAppBuilder::new());
    builder
        .services_mut()
        .replace_service::<dyn InputMapService>(Arc::clone(&own));
    let services = builder
        .add_input_map(|options| {
            options.bind_key("", "cmd-s", "file::save");
        })
        .build()
        .unwrap();
    initialize(&services);
    let input_map = services.required_service::<dyn InputMapService>().unwrap();
    assert!(!Arc::ptr_eq(&input_map, &own));
    assert_eq!(
        input_map.resolve(&[], &key("cmd-s")).unwrap().action,
        "file::save"
    );

    // One the app registers after it replaces RayX's, whose startup goes with it.
    let mut builder = with_store(RayXAppBuilder::new()).add_input_map(|_| {});
    builder
        .services_mut()
        .replace_service::<dyn InputMapService>(Arc::clone(&own));
    let services = builder.build().unwrap();
    initialize(&services);
    let input_map = services.required_service::<dyn InputMapService>().unwrap();
    assert!(Arc::ptr_eq(&input_map, &own));
}

#[test]
fn bind_key_with_and_effective_entries_carry_dispatch_policy() {
    let store: Arc<dyn KeyValueStore> = FakeKeyValueStore::new();
    let services = start(Arc::clone(&store), |options| {
        options.framework = vec![
            InputMapEntry::keyboard("", "mod-z", "edit::undo", None),
            InputMapEntry::keyboard("", "mod-s", "file::save", None),
        ];
        options.bind_key_with("editor", "mod-s", "editor::save", |entry| {
            entry
                .with_when(InputWhen::Always)
                .with_repeat(true)
                .with_pass_through(true)
        });
    });
    let input_map = services.required_service::<dyn InputMapService>().unwrap();

    // Platform resolution matches `mod-s` against `ctrl-s` on non-Apple and `cmd-s` on Apple.
    let win = input_map
        .resolve_for_platform(&["editor"], &key("ctrl-s"), false)
        .unwrap();
    assert_eq!(win.action, "editor::save");
    assert_eq!(win.when, InputWhen::Always);
    assert!(win.repeat);
    assert!(win.pass_through);
    assert_eq!(
        input_map
            .resolve_for_platform(&["editor"], &key("cmd-s"), true)
            .unwrap()
            .action,
        "editor::save"
    );
    assert_eq!(
        input_map.resolve_for_platform(&["editor"], &key("ctrl-s"), true),
        None
    );

    // User unbind removes `mod-z` from effective entries.
    block_on(input_map.apply_user_document(
        0,
        InputMapDocument::new(
            InputMapSource::User,
            0,
            vec![InputMapEntry {
                unbind: true,
                ..InputMapEntry::keyboard("", "mod-z", "edit::undo", None)
            }],
        ),
    ))
    .unwrap();
    let effective = input_map.effective_entries_for_contexts(&["editor"]);
    assert_eq!(effective.len(), 1);
    assert_eq!(effective[0].binding, key("mod-s"));
    assert_eq!(effective[0].action, "editor::save");
    assert_eq!(effective[0].when, InputWhen::Always);
    assert!(effective[0].repeat);
    assert!(effective[0].pass_through);
}
