//! The suspension service: lifecycle, activation and visibility from events, RayX's service
//! publishing its state only when it changed and every event to its subscribers, and
//! `add_suspension()`'s one service per root.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::{Arc, Mutex};

use futures::FutureExt as _;
use rayx_app::RayXAppBuilder;
use rayx_app::services::suspension::*;
use rayx_services_suspension::SuspensionAppBuilderExt as _;
use rayx_services_suspension::testing::FedSuspension;

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
fn event_subscribers_hear_every_event_and_state_subscribers_only_changes() {
    let fed = FedSuspension::new();
    let suspension = fed.service();
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
        .subscribe(move |snapshot| state_sink.lock().unwrap().push(snapshot.activation));

    for (kind, at) in [
        (SuspensionEventKind::Activated, 1),
        (SuspensionEventKind::Activated, 2),
        (SuspensionEventKind::Deactivated, 3),
    ] {
        fed.emit(event(kind).at(at));
    }
    assert_eq!(*events.lock().unwrap(), 3);
    // The same event again, later, is no change of state: nothing is published.
    assert_eq!(
        *states.lock().unwrap(),
        [
            SuspensionActivationState::Active,
            SuspensionActivationState::Inactive,
        ]
    );
    let current = suspension.suspension().get();
    assert_eq!(current.activation, SuspensionActivationState::Inactive);
    assert_eq!(current.updated_at_unix_millis, Some(3));
}

#[test]
fn asking_as_the_app_already_asks_publishes_nothing() {
    let suspension = FedSuspension::new().service();
    let states = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&states);
    let _states = suspension
        .suspension()
        .subscribe(move |snapshot| sink.lock().unwrap().push(snapshot.asks_before_quit));
    for ask in [true, true, false, false] {
        suspension
            .ask_before_quit(ask)
            .now_or_never()
            .unwrap()
            .unwrap();
    }
    assert_eq!(*states.lock().unwrap(), [true, false]);
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
