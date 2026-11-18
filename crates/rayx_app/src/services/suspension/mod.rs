//! Suspension: the app's lifecycle (started, shutting down), activation (focus) and visibility
//! (foreground, background), a desktop's window and the system's wakes from sleep, published by
//! a [`SuspensionService`] implementation, for work that pauses in the background or must be
//! saved as the app quits; and quitting the app, asked first while the app wants to be.
//!
//! RayX's implementation is `rayx_services_suspension`'s (`add_suspension()`), which follows
//! NSH's app state and quits through NSH; an app may register its own, in Rust or in TS.

mod model;

use std::fmt;
use std::sync::Arc;

use futures::FutureExt as _;

pub use model::{
    SuspensionActivationState, SuspensionEvent, SuspensionEventKind, SuspensionEventReason,
    SuspensionLifecycleState, SuspensionSnapshot, SuspensionVisibilityState, WindowActivity,
};

use super::ContractFailure;
use crate::ChangeSubscription;
use crate::published::{EventStream, StateCell};

/// Why a suspension command failed: it did not reach its implementation, or the implementation
/// failed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SuspensionError {
    /// The call failed (a TS implementation threw, the engine is gone).
    Failed(ContractFailure),
}

impl fmt::Display for SuspensionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Failed(failure) => failure.fmt(f),
        }
    }
}

impl std::error::Error for SuspensionError {}

impl From<ContractFailure> for SuspensionError {
    fn from(failure: ContractFailure) -> Self {
        Self::Failed(failure)
    }
}

/// The suspension service: the app's lifecycle, activation, visibility, a desktop's window and
/// wakes, as state and as events, and quitting the app. Work that pauses in the background
/// follows the state; work that must not be lost listens for
/// [`SuspensionEventKind::Destroying`]; an app that asks before it quits sets
/// [`ask_before_quit`](Self::ask_before_quit) and hears [`SuspensionEventKind::QuitRequested`].
#[rayx_app_macros::contract(ts = "SuspensionServiceContract")]
pub trait SuspensionService {
    /// The app's lifecycle, activation, visibility, window, wakes and whether it asks before it
    /// quits, now and as they change.
    #[state(ts = "SuspensionSnapshot")]
    fn suspension(&self) -> &StateCell<SuspensionSnapshot>;

    /// Each lifecycle event (started, activated, backgrounded, woke, destroying, …) and each
    /// request to quit while the app asks first.
    #[events(ts = "SuspensionEvent")]
    fn events(&self) -> &EventStream<SuspensionEvent>;

    /// Quits the app, whether or not it asks first: what an app does once it is ready to go
    /// after a request. A desktop's app quits (the app hears `Destroying` and `Destroyed`);
    /// elsewhere the system decides, and nothing happens.
    async fn quit(&self) -> Result<(), SuspensionError>;

    /// While `ask` holds, the user's requests to quit (the app's Quit item, its quit key) come to
    /// the app as [`SuspensionEventKind::QuitRequested`] events instead of quitting it. Desktops
    /// only; elsewhere nothing asks.
    async fn ask_before_quit(&self, ask: bool) -> Result<(), SuspensionError>;
}

impl dyn SuspensionService {
    /// Until the subscription is dropped, the user's requests to quit come to `listener` instead
    /// of quitting the app (the app asks, saves or waits, then calls [`quit`](SuspensionService::quit)); when
    /// it is dropped, a request quits again. One owner at a time: dropping it stops asking.
    pub fn on_quit_request(
        self: &Arc<Self>,
        listener: impl Fn() + Send + Sync + 'static,
    ) -> ChangeSubscription {
        let subscription = self.events().subscribe(move |event| {
            if event.kind == SuspensionEventKind::QuitRequested {
                listener();
            }
        });
        // Polled once: an implementation acts when called (RayX's), or starts the call (a TS
        // one's, through the engine).
        let _ = self.ask_before_quit(true).now_or_never();
        let service = Arc::clone(self);
        ChangeSubscription::new(move || {
            drop(subscription);
            let _ = service.ask_before_quit(false).now_or_never();
        })
    }
}
