//! Key sequences in one spelling: modifiers in any order and under any of their names, keys
//! in any case, bind and resolve the same keys; spellings of one binding in one document are
//! duplicates.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::Arc;

use futures::executor::block_on;
use rayx_app::di::StartupInitializationContext;
use rayx_app::services::input_map::*;
use rayx_app::services::storage::KeyValueStore;
use rayx_app::testing::FakeKeyValueStore;
use rayx_app::{AppBuilder as _, RayXAppBuilder};
use rayx_services_input_map::InputMapAppBuilderExt as _;

fn key(sequence: &str) -> InputBinding {
    InputBinding::Keyboard(sequence.into())
}

#[test]
fn key_sequences_normalize_to_one_spelling() {
    let cases = [
        ("cmd-shift-p", "shift-cmd-p"),
        ("shift-cmd-P", "shift-cmd-p"),
        ("mod-shift-p", "mod-shift-p"),
        ("shift-mod-P", "mod-shift-p"),
        ("Control-Alt-Delete", "ctrl-alt-delete"),
        ("option-opt-Shift-a", "alt-shift-a"),
        ("super-k", "cmd-k"),
        ("win-k", "cmd-k"),
        ("command-meta-k", "cmd-k"),
        ("platform-k", "mod-k"),
        ("CmdOrCtrl-S", "mod-s"),
        ("CommandOrControl-Shift-Z", "mod-shift-z"),
        ("fn-cmd-shift-alt-ctrl-mod-x", "mod-ctrl-alt-shift-cmd-fn-x"),
        ("control-k control-c", "ctrl-k ctrl-c"),
        ("fn-shift-f1", "shift-fn-f1"),
        ("ctrl--", "ctrl--"),
        ("--", "-"),
        ("-", "-"),
        ("Escape", "escape"),
    ];
    for (sequence, normalized) in cases {
        assert_eq!(
            normalize_key_sequence(sequence).as_deref(),
            Some(normalized),
            "{sequence}"
        );
        // Normalizing is idempotent.
        assert_eq!(
            normalize_key_sequence(normalized).as_deref(),
            Some(normalized)
        );
    }
    for invalid in ["", "hyper-k", "ctrl-", "ctrl- k", "ctrl-k  ctrl-c"] {
        assert_eq!(normalize_key_sequence(invalid), None, "{invalid:?}");
        assert!(!valid_key_sequence(invalid));
    }
    assert!(key("cmd-shift-p").same_as(&key("Shift-Super-P")));
    assert!(key("mod-s").same_as(&key("Platform-S")));
    assert!(key("mod-s").same_as(&key("CmdOrCtrl-s")));
    assert!(!key("mod-s").same_as(&key("cmd-s")));
    assert!(!key("cmd-p").same_as(&key("ctrl-p")));
}

#[test]
fn platform_resolution_and_nsh_keystrokes_handle_mod_and_punctuation() {
    assert_eq!(
        resolve_key_sequence_for_platform("mod-s", true).as_deref(),
        Some("cmd-s")
    );
    assert_eq!(
        resolve_key_sequence_for_platform("mod-s", false).as_deref(),
        Some("ctrl-s")
    );
    assert_eq!(
        resolve_key_sequence_for_platform("mod-shift-z", true).as_deref(),
        Some("shift-cmd-z")
    );
    assert_eq!(
        resolve_key_sequence_for_platform("mod-shift-z", false).as_deref(),
        Some("ctrl-shift-z")
    );
    assert_eq!(
        resolve_key_sequence_for_platform("mod-ctrl-a", false).as_deref(),
        Some("ctrl-a")
    );
    assert_eq!(
        resolve_key_sequence_for_platform("mod-cmd-a", true).as_deref(),
        Some("cmd-a")
    );

    assert_eq!(
        to_nsh_keystroke("mod-shift-z").as_deref(),
        Some("mod+shift+z")
    );
    assert_eq!(to_nsh_keystroke("mod-=").as_deref(), Some("mod+plus"));
    assert_eq!(to_nsh_keystroke("mod--").as_deref(), Some("mod+-"));
    assert_eq!(to_nsh_keystroke("mod-+").as_deref(), Some("mod+plus"));
    assert_eq!(to_nsh_keystroke("ctrl-k ctrl-c"), None);
    assert_eq!(to_nsh_keystroke("fn-f1"), None);
    assert_eq!(to_nsh_keystroke(""), None);
}

#[test]
fn bindings_resolve_whatever_their_spelling() {
    let mut builder = RayXAppBuilder::new();
    builder
        .services_mut()
        .add_singleton_as::<dyn KeyValueStore>(FakeKeyValueStore::new() as Arc<dyn KeyValueStore>);
    let services = builder
        .add_input_map(|options| {
            options.framework = vec![InputMapEntry::keyboard(
                "",
                "cmd-shift-p",
                "app::palette",
                None,
            )];
        })
        .build()
        .unwrap();
    block_on(
        services
            .startup_registry()
            .initialize(StartupInitializationContext::new(services.clone())),
    )
    .unwrap();
    let service = services.required_service::<dyn InputMapService>().unwrap();

    let resolved = service.resolve(&["editor"], &key("shift-cmd-P")).unwrap();
    assert_eq!(resolved.action, "app::palette");

    // The user's unbinding, spelled otherwise, removes the framework's binding.
    let mut user = service.user_document();
    user.entries.push(InputMapEntry {
        unbind: true,
        ..InputMapEntry::keyboard("", "Super-Shift-p", "app::palette", None)
    });
    block_on(service.apply_user_document(service.revision(), user)).unwrap();
    assert!(service.resolve(&[], &key("cmd-shift-p")).is_none());
    assert!(service.bindings_for("app::palette").is_empty());
}

#[test]
fn spellings_of_one_binding_are_duplicates() {
    let document = InputMapDocument::new(
        InputMapSource::Application,
        0,
        vec![
            InputMapEntry::keyboard("", "cmd-shift-p", "a", None),
            InputMapEntry::keyboard("", "shift-cmd-p", "b", None),
        ],
    );
    assert_eq!(document.validate(), Err(InputMapError::DuplicateBinding));
}
