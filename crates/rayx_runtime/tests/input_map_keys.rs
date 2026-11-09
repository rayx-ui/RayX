//! Key sequences in one spelling: modifiers in any order and under any of their names, keys
//! in any case, bind and resolve the same keys; spellings of one binding in one document are
//! duplicates.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::Arc;

use futures::executor::block_on;
use rayx_runtime::services::input_map::*;
use rayx_runtime::services::storage::{InMemoryKeyValueStore, KeyValueStore};

fn key(sequence: &str) -> InputBinding {
    InputBinding::Keyboard(sequence.into())
}

#[test]
fn key_sequences_normalize_to_one_spelling() {
    let cases = [
        ("cmd-shift-p", "shift-cmd-p"),
        ("shift-cmd-P", "shift-cmd-p"),
        ("Control-Alt-Delete", "ctrl-alt-delete"),
        ("super-k", "cmd-k"),
        ("win-k", "cmd-k"),
        ("platform-k", "cmd-k"),
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
    assert!(!key("cmd-p").same_as(&key("ctrl-p")));
}

#[test]
fn bindings_resolve_whatever_their_spelling() {
    let framework = InputMapDocument::new(
        InputMapSource::Framework,
        0,
        vec![InputMapEntry::keyboard(
            "",
            "cmd-shift-p",
            "app::palette",
            None,
        )],
    );
    let application = InputMapDocument::new(InputMapSource::Application, 0, Vec::new());
    let store: Arc<dyn KeyValueStore> = Arc::new(InMemoryKeyValueStore::new());
    let service = InputMapService::new(store, framework, application).unwrap();
    block_on(service.load()).unwrap();

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
