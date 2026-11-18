//! An operation service that publishes its targets in any order (a TS implementation's
//! insertion order): `has_target` finds each of them and `targets()` lists them sorted.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::Arc;

use rayx_app::services::operations::{OperationService, OperationState, OperationTargetId};
use rayx_app::testing::FakeOperations;

fn ids(names: &[&str]) -> Vec<OperationTargetId> {
    names
        .iter()
        .map(|name| OperationTargetId::new(*name).unwrap())
        .collect()
}

fn unsorted() -> OperationState {
    OperationState {
        targets: ids(&["zeta", "alpha", "mid", "beta"]),
        ..OperationState::default()
    }
}

#[test]
fn has_target_finds_every_target_published_unsorted() {
    let state = unsorted();
    for id in ids(&["zeta", "alpha", "mid", "beta"]) {
        assert!(state.has_target(&id), "{id:?}");
    }
    assert!(!state.has_target(&OperationTargetId::new("gamma").unwrap()));
}

#[test]
fn the_service_s_helpers_read_unsorted_targets() {
    let fake = FakeOperations::new();
    fake.operations().publish(unsorted());
    let service: Arc<dyn OperationService> = fake;
    assert_eq!(service.targets(), ids(&["alpha", "beta", "mid", "zeta"]));
    for id in ids(&["zeta", "alpha", "mid", "beta"]) {
        assert!(service.has_target(&id), "{id:?}");
    }
    assert!(!service.has_target(&OperationTargetId::new("gamma").unwrap()));
}
