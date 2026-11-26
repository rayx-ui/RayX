//! The input map contract in the core, NSH-free: resolving a binding and listing an action's
//! shortcuts are functions of the published `InputMapState`, so they answer the same over any
//! implementation (`rayx_app::testing::FakeInputMap` here, registered as `dyn InputMapService`
//! in a plain provider), and a failed call is in the contract's error type.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::Arc;

use futures::executor::block_on;
use rayx_app::services::ContractFailure;
use rayx_app::services::input_map::{
    InputBinding, InputMapApplyError, InputMapDocument, InputMapEntry, InputMapService,
    InputMapSource, InputMapState, InputWhen, ResolvedInputEntry, normalize_key_sequence,
    resolve_key_sequence_for_platform, to_nsh_keystroke,
};
use rayx_app::testing::FakeInputMap;
use rayx_app::{AppBuilder as _, RayXAppBuilder};

fn key(sequence: &str) -> InputBinding {
    InputBinding::Keyboard(sequence.into())
}

fn state() -> InputMapState {
    InputMapState {
        framework: InputMapDocument::new(
            InputMapSource::Framework,
            0,
            vec![InputMapEntry::keyboard("", "cmd-z", "edit::undo", None)],
        ),
        application: InputMapDocument::new(
            InputMapSource::Application,
            0,
            vec![InputMapEntry::keyboard(
                "editor",
                "cmd-d",
                "editor::duplicate",
                None,
            )],
        ),
        ..InputMapState::default()
    }
}

#[test]
fn lookups_are_functions_of_the_state() {
    let state = state();
    let resolved = state.resolve(&["editor"], &key("Cmd-D")).unwrap();
    assert_eq!(resolved.action, "editor::duplicate");
    assert_eq!(resolved.source, InputMapSource::Application);
    assert_eq!(
        state.resolve(&["editor"], &key("cmd-z")).unwrap().context,
        ""
    );
    assert_eq!(state.resolve(&["other"], &key("cmd-d")), None);
    assert_eq!(state.bindings_for("edit::undo").len(), 1);
}

#[test]
fn the_fake_is_an_input_map_resolved_by_trait() {
    let mut builder = RayXAppBuilder::new();
    builder
        .services_mut()
        .add_singleton_as::<dyn InputMapService>(
            FakeInputMap::new(state()) as Arc<dyn InputMapService>
        );
    let services = builder.build().unwrap();
    let input_map = services.required_service::<dyn InputMapService>().unwrap();
    assert_eq!(
        input_map
            .resolve(&["editor"], &key("cmd-d"))
            .unwrap()
            .action,
        "editor::duplicate"
    );

    let applied = block_on(input_map.apply_user_document(
        0,
        InputMapDocument::new(
            InputMapSource::User,
            0,
            vec![InputMapEntry {
                unbind: true,
                ..InputMapEntry::keyboard("", "cmd-z", "edit::undo", None)
            }],
        ),
    ))
    .unwrap();
    assert_eq!(applied.revision, 1);
    assert_eq!(input_map.revision(), 1);
    assert_eq!(input_map.resolve(&["editor"], &key("cmd-z")), None);
    assert!(input_map.bindings_for("edit::undo").is_empty());
    assert_eq!(block_on(input_map.load()).unwrap(), applied);
    assert!(matches!(
        block_on(input_map.apply_user_document(0, applied)),
        Err(InputMapApplyError::RevisionConflict {
            expected: 0,
            actual: 1
        })
    ));
}

#[test]
fn a_failed_call_is_the_contract_s_error() {
    let failure = ContractFailure {
        contract: "InputMapServiceContract",
        member: "load",
        message: "the engine is gone".into(),
    };
    let error = InputMapApplyError::from(failure.clone());
    assert!(matches!(&error, InputMapApplyError::Failed(f) if *f == failure));
    assert!(error.to_string().contains("the engine is gone"), "{error}");
}

#[test]
fn mod_modifier_normalization_and_aliases_stay_distinct_from_cmd() {
    let cases = [
        ("mod-shift-p", "mod-shift-p"),
        ("shift-mod-p", "mod-shift-p"),
        ("cmd-shift-p", "shift-cmd-p"),
        ("platform-s", "mod-s"),
        ("CmdOrCtrl-S", "mod-s"),
        ("CommandOrControl-Option-K", "mod-alt-k"),
        ("opt-control-a", "ctrl-alt-a"),
        ("command-meta-super-win-k", "cmd-k"),
        ("fn-cmd-shift-alt-ctrl-mod-x", "mod-ctrl-alt-shift-cmd-fn-x"),
        ("mod--", "mod--"),
        ("mod-=", "mod-="),
        ("mod-+", "mod-+"),
    ];
    for (input, expected) in cases {
        assert_eq!(
            normalize_key_sequence(input).as_deref(),
            Some(expected),
            "{input}"
        );
    }
    assert!(!key("mod-shift-p").same_as(&key("cmd-shift-p")));
    assert!(key("mod-shift-p").same_as(&key("shift-platform-P")));
}

#[test]
fn resolve_key_sequence_for_platform_expands_and_deduplicates_mod() {
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
        resolve_key_sequence_for_platform("mod-ctrl-a", true).as_deref(),
        Some("ctrl-cmd-a")
    );
    assert_eq!(
        resolve_key_sequence_for_platform("mod-cmd-a", true).as_deref(),
        Some("cmd-a")
    );
    assert_eq!(
        resolve_key_sequence_for_platform("platform-k mod-c", false).as_deref(),
        Some("ctrl-k ctrl-c")
    );
    assert_eq!(resolve_key_sequence_for_platform("hyper-a", true), None);
    assert_eq!(resolve_key_sequence_for_platform("", false), None);
}

#[test]
fn to_nsh_keystroke_translates_single_strokes_and_rejects_chords_or_invalid() {
    let cases = [
        ("mod-shift-z", Some("mod+shift+z")),
        ("Shift-Platform-Z", Some("mod+shift+z")),
        ("mod-=", Some("mod+plus")),
        ("mod--", Some("mod+-")),
        ("mod-+", Some("mod+plus")),
        ("=", Some("plus")),
        ("+", Some("plus")),
        ("-", Some("-")),
        ("--", Some("-")),
        ("cmd-shift-p", Some("shift+cmd+p")),
        ("Escape", Some("escape")),
        ("ctrl-k ctrl-c", None),
        ("mod-k mod-s", None),
        ("fn-f1", None),
        ("", None),
        ("ctrl-", None),
        ("hyper-k", None),
    ];
    for (input, expected) in cases {
        assert_eq!(to_nsh_keystroke(input).as_deref(), expected, "{input:?}");
    }
}

#[test]
fn dispatch_policy_fields_default_and_round_trip_json() {
    // Existing JSON without when/repeat/pass_through deserializes cleanly.
    let legacy = r#"{
        "context": "editor",
        "binding": { "kind": "keyboard", "value": "mod-s" },
        "action": "file::save"
    }"#;
    let parsed: InputMapEntry = serde_json::from_str(legacy).unwrap();
    assert_eq!(parsed.when, InputWhen::Auto);
    assert!(!parsed.repeat);
    assert!(!parsed.pass_through);

    // Default policy fields are omitted on serialization.
    let default_json = serde_json::to_value(&parsed).unwrap();
    assert!(default_json.get("when").is_none());
    assert!(default_json.get("repeat").is_none());
    assert!(default_json.get("pass_through").is_none());

    // Builder methods set non-default fields and round-trip.
    let entry = InputMapEntry::keyboard("editor", "mod-z", "edit::undo", None)
        .with_when(InputWhen::NotEditing)
        .with_repeat(true)
        .with_pass_through(true);
    assert_eq!(entry.when, InputWhen::NotEditing);
    assert!(entry.repeat);
    assert!(entry.pass_through);
    let json_text = serde_json::to_string(&entry).unwrap();
    assert!(json_text.contains(r#""when":"not_editing""#), "{json_text}");
    assert!(json_text.contains(r#""repeat":true"#), "{json_text}");
    assert!(json_text.contains(r#""pass_through":true"#), "{json_text}");
    let restored: InputMapEntry = serde_json::from_str(&json_text).unwrap();
    assert_eq!(restored, entry);
}

#[test]
fn resolve_for_platform_and_effective_entries_honor_precedence_and_unbind() {
    let state = InputMapState {
        framework: InputMapDocument::new(
            InputMapSource::Framework,
            0,
            vec![
                InputMapEntry::keyboard("", "mod-s", "file::save", None),
                InputMapEntry::keyboard("", "mod-z", "edit::undo", None)
                    .with_when(InputWhen::NotEditing),
                InputMapEntry::keyboard("", "mod-c", "edit::copy", None),
                InputMapEntry::keyboard("editor", "mod-d", "editor::duplicate", None),
            ],
        ),
        application: InputMapDocument::new(
            InputMapSource::Application,
            0,
            vec![
                InputMapEntry::keyboard("editor", "mod-d", "editor::select_next", None)
                    .with_repeat(true),
                InputMapEntry::keyboard("canvas", "mod-s", "canvas::save", None)
                    .with_when(InputWhen::Always)
                    .with_pass_through(true),
            ],
        ),
        user: InputMapDocument::new(
            InputMapSource::User,
            1,
            vec![
                // Unbind `mod-z` globally in user layer.
                InputMapEntry {
                    unbind: true,
                    ..InputMapEntry::keyboard("", "mod-z", "edit::undo", None)
                },
                // Unbind `mod-c` only inside `canvas`.
                InputMapEntry {
                    unbind: true,
                    ..InputMapEntry::keyboard("canvas", "mod-c", "edit::copy", None)
                },
                // Rebind `mod-d` in `editor` with alternate spelling.
                InputMapEntry::keyboard("editor", "Platform-D", "user::duplicate", None),
            ],
        ),
        revision: 1,
    };

    // Windows (`apple = false`): pressing `ctrl-s` matches `mod-s` in `canvas`.
    let win_save = state
        .resolve_for_platform(&["canvas", "editor"], &key("ctrl-s"), false)
        .unwrap();
    assert_eq!(win_save.action, "canvas::save");
    assert_eq!(win_save.source, InputMapSource::Application);
    assert_eq!(win_save.when, InputWhen::Always);
    assert!(win_save.pass_through);
    assert!(!win_save.repeat);

    // macOS (`apple = true`): pressing `cmd-s` matches `mod-s`, `ctrl-s` does not.
    assert_eq!(
        state
            .resolve_for_platform(&["canvas"], &key("cmd-s"), true)
            .unwrap()
            .action,
        "canvas::save"
    );
    assert_eq!(
        state.resolve_for_platform(&["canvas"], &key("ctrl-s"), true),
        None
    );

    // User unbind of `mod-z` blocks `ctrl-z` on Windows and `cmd-z` on macOS.
    assert_eq!(
        state.resolve_for_platform(&["editor"], &key("ctrl-z"), false),
        None
    );
    assert_eq!(
        state.resolve_for_platform(&["editor"], &key("cmd-z"), true),
        None
    );

    // Effective entries for `["canvas", "editor"]`:
    // - `mod-c`: unbound in `canvas` (innermost) -> omitted even though `""` binds it.
    // - `mod-s`: `canvas` (Application) wins over `""` (Framework).
    // - `mod-d`: `editor` (User) wins over `editor` (Application & Framework).
    // - `mod-z`: unbound in `""` (User) -> omitted.
    let effective = state.effective_entries_for_contexts(&["canvas", "editor"]);
    assert_eq!(
        effective,
        vec![
            ResolvedInputEntry {
                binding: key("mod-s"),
                action: "canvas::save".into(),
                arguments: None,
                context: "canvas".into(),
                source: InputMapSource::Application,
                when: InputWhen::Always,
                repeat: false,
                pass_through: true,
            },
            ResolvedInputEntry {
                binding: key("mod-d"),
                action: "user::duplicate".into(),
                arguments: None,
                context: "editor".into(),
                source: InputMapSource::User,
                when: InputWhen::Auto,
                repeat: false,
                pass_through: false,
            },
        ]
    );

    // Outside `canvas`, `mod-c` from `""` is effective again.
    let editor_effective = state.effective_entries_for_contexts(&["editor"]);
    assert_eq!(editor_effective.len(), 3);
    assert!(
        editor_effective
            .iter()
            .any(|e| e.binding == key("mod-c") && e.action == "edit::copy")
    );
    assert!(
        editor_effective
            .iter()
            .any(|e| e.binding == key("mod-s") && e.action == "file::save")
    );
}
