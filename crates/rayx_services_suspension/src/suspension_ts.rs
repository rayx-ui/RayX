//! The suspension service for TSX.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use futures::FutureExt as _;
use nsh_core::JsonValue;
use nsh_core::prelude::*;
use rayx_app::ChangeSubscription;
use rayx_app::services::suspension::{SuspensionEventKind, SuspensionService as Suspension};
use rayx_app_nsh::ts::{Listeners, ServiceClass, json};

/// The app's lifecycle, for TSX: `services.getRequiredService(SuspensionService).lifecycle()`
/// (the suspension service's snapshot: lifecycle, activation and visibility); `onLifecycle`
/// hears every lifecycle event (`{ kind, reason, … }`, `kind` as Rust names it: `"Started"`,
/// `"Activated"`, `"Backgrounded"`, `"Woke"`, `"QuitRequested"`, `"Destroying"`, …).
/// `askBeforeQuit(listener)` makes the user's requests to quit come to `listener` until this
/// object is disposed, and `quit()` quits (a desktop's app).
#[class]
pub struct SuspensionService {
    suspension: Arc<dyn Suspension>,
    listeners: Listeners,
    /// How many of its `askBeforeQuit` listeners listen: the app asks while one does.
    asking: Arc<AtomicUsize>,
}

impl ServiceClass for SuspensionService {
    type Service = dyn Suspension;

    fn wrap(suspension: Arc<dyn Suspension>) -> Self {
        Self {
            suspension,
            listeners: Listeners::default(),
            asking: Arc::default(),
        }
    }
}

#[class]
impl SuspensionService {
    /// The app's lifecycle, activation and visibility now.
    #[ts("(): SuspensionSnapshot")]
    pub fn lifecycle(&self) -> JsonValue {
        json(&*self.suspension.suspension().get())
    }

    /// The contract this class is the token of (TS types only: `null` at run time).
    #[getter]
    #[ts("(): SuspensionServiceContract | null")]
    pub fn contract(&self) -> Option<bool> {
        None
    }

    /// Calls `listener` with every lifecycle event (`{ kind, reason, timestamp_unix_millis,
    /// window }`: the app's start, focus, background and foreground, its window's changes, the
    /// system's wakes from sleep, its quit), until the function it returns is called (or the
    /// reactive owner of the call goes), or this object is disposed.
    #[ts("(listener: (event: SuspensionEvent) => void): () => void")]
    pub fn on_lifecycle(
        &mut self,
        #[args(event)] listener: Callback<JsonValue>,
    ) -> Result<Stop, String> {
        let suspension = Arc::clone(&self.suspension);
        self.listeners.listen(listener, move |post| {
            Ok::<_, String>(
                suspension
                    .events()
                    .subscribe(move |event| post(json(event))),
            )
        })
    }

    /// Until the function it returns is called (or the reactive owner of the call goes), or
    /// this object is disposed, the user's requests to quit (the app's Quit item, its quit key)
    /// call `listener` instead of quitting: the app asks, saves or waits, then calls `quit()`.
    /// Once no listener is left, a request quits again. Desktops only.
    #[ts("(listener: () => void): () => void")]
    pub fn ask_before_quit(&mut self, listener: Callback<JsonValue>) -> Result<Stop, String> {
        let suspension = Arc::clone(&self.suspension);
        let asking = Arc::clone(&self.asking);
        self.listeners.listen(listener, move |post| {
            let requests = suspension.events().subscribe(move |event| {
                if event.kind == SuspensionEventKind::QuitRequested {
                    post(JsonValue::Null);
                }
            });
            // The app asks from the first listener until the last one stops (polled once, as
            // `on_quit_request` does: one owner of asking, this object, for all of them).
            if asking.fetch_add(1, Ordering::SeqCst) == 0 {
                let _ = suspension.ask_before_quit(true).now_or_never();
            }
            Ok::<_, String>(ChangeSubscription::new(move || {
                drop(requests);
                if asking.fetch_sub(1, Ordering::SeqCst) == 1 {
                    let _ = suspension.ask_before_quit(false).now_or_never();
                }
            }))
        })
    }

    /// Quits the app, whether or not it asks first (a desktop's; elsewhere nothing happens).
    pub fn quit(&self) {
        // Polled once, as `on_quit_request` does: an implementation acts when called (RayX's),
        // or starts the call (a TS one's). Not a task: TS calls it where no reactive runtime
        // runs (a timer).
        let _ = self.suspension.quit().now_or_never();
    }
}
