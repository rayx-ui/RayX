//! Keeping the platform and suspension services current from NSH's environment and app state.

use std::cell::Cell;
use std::rc::Rc;

use nsh_core::element::desktop::window_state;
use nsh_core::platform::Lifecycle;
use nsh_core::prelude::{Owner, cleanup, effect, global, lifecycle, untracked, wake_count};
use rayx_app::ServiceProvider;
use rayx_runtime::services::platform::PlatformInfoService;
use rayx_runtime::services::suspension::{
    SuspensionEvent, SuspensionEventKind, SuspensionEventReason, SuspensionService, WindowActivity,
};

use crate::services::platform::snapshot::{ask_features, platform_snapshot};
use crate::services::suspension::host::{QuitWiring, keep_quitting};
use crate::{AppState, app_services, app_state};

/// Gives the root's platform service NSH's environment as it is now, without subscribing:
/// the app's start does it before startup, so startup services see the device
/// and the user's settings rather than the snapshot's defaults. [`use_platform_sync`] keeps it
/// current from then on.
pub(crate) fn seed_platform(root: &ServiceProvider) {
    if let Some(platform) = root.service::<PlatformInfoService>() {
        platform.update(untracked(platform_snapshot));
    }
}

/// Keeps the surface's platform and suspension services current from NSH's environment:
/// once the app is ready, the [`PlatformInfoService`] gets every change of the device and the
/// user's settings (and what the platform answers of web views and speech), and the
/// [`SuspensionService`] the app's start, every change of NSH's app state (active, inactive,
/// background) and of a desktop's window, every wake of the system from sleep, the user's
/// requests to quit while the app asks first, and the app's quit (`Destroying`, then the
/// storage writes what waits, then `Destroyed`). Call it in the app's root component; it stops
/// with the component.
pub fn use_platform_sync() {
    if global::<PlatformSync>().with_untracked(|sync| sync.running) {
        return;
    }
    global::<PlatformSync>().set(PlatformSync { running: true });
    cleanup(|| global::<PlatformSync>().set(PlatformSync { running: false }));
    sync_platform();
}

/// Whether this surface's platform and suspension services are kept current already (by a Rust
/// root's [`use_platform_sync`] or TSX's `builder.start()`), so they are not kept twice.
#[derive(Default)]
struct PlatformSync {
    running: bool,
}

/// [`use_platform_sync`] for the surface's life: what TSX's `builder.start()` starts, since a TSX
/// app has no root component. Call it where the engine runs Rust code (a task).
pub(crate) fn keep_platform_current() {
    if global::<PlatformSync>().with_untracked(|sync| sync.running) {
        return;
    }
    global::<PlatformSync>().set(PlatformSync { running: true });
    Owner::detached().run(sync_platform);
}

/// The effect that copies NSH's environment and app state into the services, and the app's
/// quitting.
fn sync_platform() {
    ask_features();
    let quitting = Rc::new(keep_quitting());
    let seen = Rc::new(Seen::default());
    effect(move || {
        // Reading the app's state subscribes: the effect runs again once it is ready.
        if app_state() != AppState::Ready {
            return;
        }
        let Ok(root) = app_services() else {
            return;
        };
        sync(&root, &seen, &quitting);
    });
}

/// What the effect saw last.
#[derive(Default)]
struct Seen {
    started: Cell<bool>,
    state: Cell<Option<Lifecycle>>,
    window: Cell<Option<WindowActivity>>,
    wakes: Cell<Option<u32>>,
}

fn sync(root: &ServiceProvider, seen: &Seen, quitting: &QuitWiring) {
    quitting.connect(root);
    if let Some(platform) = root.service::<PlatformInfoService>() {
        platform.update(platform_snapshot());
    }
    // Read before anything returns, so the effect runs again after each wake.
    let wakes = wake_count();
    let Some(suspension) = root.service::<SuspensionService>() else {
        return;
    };
    let (started, last_state, last_window) = (&seen.started, &seen.state, &seen.window);
    if !started.replace(true) {
        suspension.publish(SuspensionEvent::new(
            SuspensionEventKind::Started,
            SuspensionEventReason::HostStarted,
        ));
    }
    // A desktop's window: its focus, being minimized and being shown.
    let window = window_state().map(|state| WindowActivity {
        focused: state.focused,
        minimized: state.minimized,
        visible: state.visible,
    });
    if let Some(window) = window
        && last_window.replace(Some(window)) != Some(window)
    {
        suspension.publish(SuspensionEvent::window_changed(
            window,
            SuspensionEventReason::WindowStateChanged,
        ));
    }
    // The wakes since the app was ready (those before it are where it started).
    if let Some(before) = seen.wakes.replace(Some(wakes)) {
        for _ in before..wakes {
            suspension.publish(SuspensionEvent::new(
                SuspensionEventKind::Woke,
                SuspensionEventReason::SystemWoke,
            ));
        }
    }
    let state = lifecycle();
    let previous = last_state.replace(Some(state));
    if previous == Some(state) {
        return;
    }
    let publish = |kind| {
        suspension.publish(SuspensionEvent::new(
            kind,
            SuspensionEventReason::AppStateChanged,
        ));
    };
    match state {
        Lifecycle::Active => {
            if previous == Some(Lifecycle::Background) {
                publish(SuspensionEventKind::Foregrounded);
            }
            publish(SuspensionEventKind::Activated);
        }
        Lifecycle::Inactive => {
            if previous == Some(Lifecycle::Background) {
                publish(SuspensionEventKind::Foregrounded);
            }
            publish(SuspensionEventKind::Deactivated);
        }
        Lifecycle::Background => publish(SuspensionEventKind::Backgrounded),
    }
}
