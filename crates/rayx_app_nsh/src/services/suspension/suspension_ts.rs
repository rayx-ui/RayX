//! The suspension service for TSX.

use std::sync::Arc;

use nsh_core::JsonValue;
use nsh_core::prelude::*;
use rayx_runtime::services::suspension::SuspensionService as Suspension;

use crate::di::provider_ts::ServiceClass;
use crate::services::json;
use crate::services::listeners_ts::Listeners;

/// The app's lifecycle, for TSX: `services.getRequiredService(SuspensionService).lifecycle()`
/// (the suspension service's snapshot: lifecycle, activation and visibility); `onLifecycle`
/// hears every lifecycle event (`{ kind, reason, … }`, `kind` as Rust names it: `"Started"`,
/// `"Activated"`, `"Backgrounded"`, `"Woke"`, `"QuitRequested"`, `"Destroying"`, …).
/// `askBeforeQuit(listener)` makes the user's requests to quit come to `listener` until this
/// object is disposed, and `quit()` quits (a desktop's app).
#[class]
pub struct SuspensionService {
    suspension: Arc<Suspension>,
    listeners: Listeners,
}

impl ServiceClass for SuspensionService {
    type Service = Suspension;

    fn wrap(suspension: Arc<Suspension>) -> Self {
        Self {
            suspension,
            listeners: Listeners::default(),
        }
    }
}

#[class]
impl SuspensionService {
    /// The app's lifecycle, activation and visibility now.
    pub fn lifecycle(&self) -> JsonValue {
        json(&self.suspension.current())
    }

    /// Calls `listener` with every lifecycle event (`{ kind, reason, timestamp_unix_millis,
    /// window }`: the app's start, focus, background and foreground, its window's changes, the
    /// system's wakes from sleep, its quit), until the function it returns is called (or the
    /// reactive owner of the call goes), or this object is disposed.
    pub fn on_lifecycle(
        &mut self,
        #[args(event)] listener: Callback<JsonValue>,
    ) -> Result<Stop, String> {
        let suspension = Arc::clone(&self.suspension);
        self.listeners.listen(listener, move |post| {
            Ok::<_, String>(suspension.subscribe_events(move |event| post(json(event))))
        })
    }

    /// Until the function it returns is called (or the reactive owner of the call goes), or
    /// this object is disposed, the user's requests to quit (the app's Quit item, its quit key)
    /// call `listener` instead of quitting: the app asks, saves or waits, then calls `quit()`.
    /// Once no listener is left, a request quits again. Desktops only.
    pub fn ask_before_quit(&mut self, listener: Callback<JsonValue>) -> Result<Stop, String> {
        let suspension = Arc::clone(&self.suspension);
        self.listeners.listen(listener, move |post| {
            Ok::<_, String>(suspension.ask_before_quit(move || post(JsonValue::Null)))
        })
    }

    /// Quits the app, whether or not it asks first (a desktop's; elsewhere nothing happens).
    pub fn quit(&self) {
        self.suspension.quit();
    }
}
