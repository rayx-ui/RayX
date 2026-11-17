//! The suspension service: lifecycle, activation and visibility from events, its state and
//! event subscribers, and `add_suspension()`'s one service per root.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::{Arc, Mutex};

use futures::FutureExt as _;
use rayx_app::RayXAppBuilder;
use rayx_app::services::suspension::*;
use rayx_app::testing::FakeSuspension;
use rayx_services_suspension::SuspensionAppBuilderExt as _;

fn event(kind: SuspensionEventKind) -> SuspensionEvent {
    SuspensionEvent::new(kind, SuspensionEventReason::AppStateChanged).at(42)
}

#[test]
fn events_move_lifecycle_activation_and_visibility() {
    let snapshot = SuspensionSnapshot::default().apply(&SuspensionEvent::new(
        SuspensionEventKind::Started,
        SuspensionEventReason::HostStarted,
    ));
    assert!(snapshot.is_running());
    assert!(snapshot.is_foreground());

    let snapshot = snapshot.apply(&event(SuspensionEventKind::Activated));
    assert!(snapshot.is_active());
    let snapshot = snapshot.apply(&event(SuspensionEventKind::Backgrounded));
    assert!(snapshot.is_backgrounded());
    assert!(!snapshot.is_active());
    assert_eq!(snapshot.updated_at_unix_millis, Some(42));
    assert_eq!(
        snapshot.last_reason,
        Some(SuspensionEventReason::AppStateChanged)
    );

    let snapshot = snapshot.apply(&event(SuspensionEventKind::Destroyed));
    assert_eq!(snapshot.lifecycle, SuspensionLifecycleState::Destroyed);
}

#[test]
fn event_subscribers_hear_every_event_and_state_subscribers_every_state() {
    let suspension = FakeSuspension::new();
    assert_eq!(
        *suspension.suspension().get(),
        SuspensionSnapshot::default()
    );
    let events = Arc::new(Mutex::new(0));
    let states = Arc::new(Mutex::new(Vec::new()));
    let event_sink = Arc::clone(&events);
    let state_sink = Arc::clone(&states);
    let _events = suspension
        .events()
        .subscribe(move |_| *event_sink.lock().unwrap() += 1);
    let _states = suspension
        .suspension()
        .subscribe(move |snapshot| state_sink.lock().unwrap().push(**snapshot));

    for kind in [
        SuspensionEventKind::Activated,
        SuspensionEventKind::Activated,
        SuspensionEventKind::Deactivated,
    ] {
        suspension.emit(event(kind));
    }
    assert_eq!(*events.lock().unwrap(), 3);
    let states = states.lock().unwrap();
    let activations: Vec<_> = states.iter().map(|state| state.activation).collect();
    assert_eq!(
        activations,
        [
            SuspensionActivationState::Active,
            SuspensionActivationState::Active,
            SuspensionActivationState::Inactive,
        ]
    );
    // The same event again is the same state; another one is a change.
    assert!(states[0].same_state(&states[1]));
    assert!(!states[1].same_state(&states[2]));
}

#[test]
fn add_suspension_registers_one_service_per_root() {
    let services = RayXAppBuilder::new().add_suspension().build().unwrap();
    let suspension = services
        .required_service::<dyn SuspensionService>()
        .unwrap();
    assert!(Arc::ptr_eq(
        &suspension,
        &services
            .create_scope()
            .required_service::<dyn SuspensionService>()
            .unwrap()
    ));
    assert!(Arc::ptr_eq(
        &suspension,
        &services
            .required_service::<dyn SuspensionService>()
            .unwrap()
    ));
    assert_eq!(
        *suspension.suspension().get(),
        SuspensionSnapshot::default()
    );

    // Without an engine, asking takes effect in its state at once (the platform hears it once
    // the service follows NSH).
    suspension
        .ask_before_quit(true)
        .now_or_never()
        .unwrap()
        .unwrap();
    assert!(suspension.suspension().get().asks_before_quit);

    // Another root has its own.
    let other = RayXAppBuilder::new()
        .add_suspension()
        .build()
        .unwrap()
        .required_service::<dyn SuspensionService>()
        .unwrap();
    assert!(!Arc::ptr_eq(&suspension, &other));
    assert!(!other.suspension().get().asks_before_quit);
}
