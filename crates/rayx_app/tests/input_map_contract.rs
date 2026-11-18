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
    InputMapSource, InputMapState,
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
