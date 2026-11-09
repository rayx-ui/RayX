//! A desktop's window in the suspension service: `WindowChanged` events set the snapshot's
//! window, a change of it is a change of state, and the same window again is not.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::{Arc, Mutex};

use rayx_runtime::services::suspension::*;

fn focused(focused: bool) -> WindowActivity {
    WindowActivity {
        focused,
        minimized: false,
        visible: true,
    }
}

#[test]
fn window_changes_are_changes_of_state() {
    let suspension = SuspensionService::new();
    let changes = Arc::new(Mutex::new(Vec::new()));
    let heard = changes.clone();
    let _subscription = suspension.subscribe_changed(move |change| {
        heard.lock().unwrap().push(change.current.window);
    });
    assert_eq!(suspension.current().window, None);

    let event =
        SuspensionEvent::window_changed(focused(true), SuspensionEventReason::WindowStateChanged);
    assert_eq!(event.kind, SuspensionEventKind::WindowChanged);
    assert!(suspension.publish(event));
    assert!(!suspension.publish(event), "the same window again");
    assert!(suspension.publish(SuspensionEvent::window_changed(
        focused(false),
        SuspensionEventReason::WindowStateChanged,
    )));

    let current = suspension.current();
    assert_eq!(current.window, Some(focused(false)));
    assert_eq!(
        current.last_reason,
        Some(SuspensionEventReason::WindowStateChanged)
    );
    // The app's own activation and visibility are the app state's, untouched by its window.
    assert_eq!(current.activation, SuspensionActivationState::Unknown);
    assert_eq!(
        *changes.lock().unwrap(),
        [Some(focused(true)), Some(focused(false))]
    );
}
