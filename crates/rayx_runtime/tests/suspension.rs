//! The suspension service: lifecycle, activation and visibility from events, and listeners.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::{Arc, Mutex};

use rayx_core::RayXAppBuilder;
use rayx_runtime::services::suspension::*;

fn event(kind: SuspensionEventKind) -> SuspensionEvent {
    SuspensionEvent::new(kind, SuspensionEventReason::AppStateChanged).at(42)
}

#[test]
fn events_move_lifecycle_activation_and_visibility() {
    let suspension = SuspensionService::new();
    assert_eq!(suspension.current(), SuspensionSnapshot::default());
    suspension.publish(SuspensionEvent::new(
        SuspensionEventKind::Started,
        SuspensionEventReason::HostStarted,
    ));
    assert!(suspension.current().is_running());
    assert!(suspension.current().is_foreground());

    suspension.publish(event(SuspensionEventKind::Activated));
    assert!(suspension.current().is_active());
    suspension.publish(event(SuspensionEventKind::Backgrounded));
    let current = suspension.current();
    assert!(current.is_backgrounded());
    assert!(!current.is_active());
    assert_eq!(current.updated_at_unix_millis, Some(42));
    assert_eq!(
        current.last_reason,
        Some(SuspensionEventReason::AppStateChanged)
    );

    suspension.publish(event(SuspensionEventKind::Destroyed));
    assert_eq!(
        suspension.current().lifecycle,
        SuspensionLifecycleState::Destroyed
    );
}

#[test]
fn event_listeners_hear_everything_and_change_listeners_only_changes() {
    let suspension = SuspensionService::new();
    let events = Arc::new(Mutex::new(0));
    let changes = Arc::new(Mutex::new(Vec::new()));
    let event_sink = Arc::clone(&events);
    let change_sink = Arc::clone(&changes);
    let _events = suspension.subscribe_events(move |_| *event_sink.lock().unwrap() += 1);
    let _changes = suspension.subscribe_changed(move |change| {
        change_sink
            .lock()
            .unwrap()
            .push((change.previous.activation, change.current.activation));
    });
    assert!(suspension.publish(event(SuspensionEventKind::Activated)));
    assert!(!suspension.publish(event(SuspensionEventKind::Activated)));
    assert!(suspension.publish(event(SuspensionEventKind::Deactivated)));
    assert_eq!(*events.lock().unwrap(), 3);
    assert_eq!(
        *changes.lock().unwrap(),
        vec![
            (
                SuspensionActivationState::Unknown,
                SuspensionActivationState::Active
            ),
            (
                SuspensionActivationState::Active,
                SuspensionActivationState::Inactive
            ),
        ]
    );
}

#[test]
fn add_suspension_registers_one_service_per_root() {
    let services = RayXAppBuilder::new().add_suspension().build().unwrap();
    let suspension = services.required_service::<SuspensionService>().unwrap();
    suspension.publish(event(SuspensionEventKind::Started));
    assert!(
        services
            .create_scope()
            .required_service::<SuspensionService>()
            .unwrap()
            .current()
            .is_running()
    );
}
