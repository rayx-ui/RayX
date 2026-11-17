//! A desktop's window in the suspension service: `WindowChanged` events set the snapshot's
//! window, a change of it is a change of state, and the same window again is not.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::{Arc, Mutex};

use rayx_app::services::suspension::*;
use rayx_app::testing::FakeSuspension;

fn focused(focused: bool) -> WindowActivity {
    WindowActivity {
        focused,
        minimized: false,
        visible: true,
    }
}

#[test]
fn window_changes_are_changes_of_state() {
    let event =
        SuspensionEvent::window_changed(focused(true), SuspensionEventReason::WindowStateChanged);
    assert_eq!(event.kind, SuspensionEventKind::WindowChanged);
    let start = SuspensionSnapshot::default();
    assert_eq!(start.window, None);

    let first = start.apply(&event);
    assert!(!start.same_state(&first));
    assert!(
        first.same_state(&first.apply(&event)),
        "the same window again"
    );
    let second = first.apply(&SuspensionEvent::window_changed(
        focused(false),
        SuspensionEventReason::WindowStateChanged,
    ));
    assert!(!first.same_state(&second));

    assert_eq!(second.window, Some(focused(false)));
    assert_eq!(
        second.last_reason,
        Some(SuspensionEventReason::WindowStateChanged)
    );
    // The app's own activation and visibility are the app state's, untouched by its window.
    assert_eq!(second.activation, SuspensionActivationState::Unknown);
    assert_eq!(second.visibility, SuspensionVisibilityState::Unknown);
}

#[test]
fn state_subscribers_hear_the_window() {
    let suspension = FakeSuspension::new();
    let windows = Arc::new(Mutex::new(Vec::new()));
    let heard = Arc::clone(&windows);
    let _subscription = suspension
        .suspension()
        .subscribe(move |snapshot| heard.lock().unwrap().push(snapshot.window));
    for focus in [true, false] {
        suspension.emit(SuspensionEvent::window_changed(
            focused(focus),
            SuspensionEventReason::WindowStateChanged,
        ));
    }
    assert_eq!(
        *windows.lock().unwrap(),
        [Some(focused(true)), Some(focused(false))]
    );
    assert_eq!(suspension.suspension().get().window, Some(focused(false)));
}
