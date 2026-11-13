use std::sync::{Arc, Mutex, PoisonError};

use rayx_core::ChangeSubscription;
use rayx_core::change::Listeners;
use rayx_core::di::AppService;

use super::{SuspensionEvent, SuspensionEventKind, SuspensionSnapshot};

/// A change of the app's lifecycle, activation, visibility, window or wakes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SuspensionChangedEvent {
    pub previous: SuspensionSnapshot,
    pub current: SuspensionSnapshot,
    /// The event that changed it.
    pub trigger: SuspensionEvent,
}

/// What the suspension service asks of the platform: to quit the app, and whether the user's
/// requests to quit (the app's Quit item, its quit key) come to the app first rather than
/// quitting. The UI layer implements it (`rayx_app` with NSH's `quit` and `on_quit_request`).
pub trait SuspensionHost: Send + Sync + 'static {
    /// Quits the app (a desktop's; elsewhere the system decides, and nothing happens). Callable
    /// on any thread.
    fn quit(&self);

    /// While `ask` holds, the user's requests to quit come to the app
    /// ([`SuspensionEventKind::QuitRequested`]) instead of quitting it. Callable on any thread.
    fn ask_before_quit(&self, ask: bool);
}

/// A host that quits nothing and never asks: the service's own until the UI layer registers one.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoopSuspensionHost;

impl SuspensionHost for NoopSuspensionHost {
    fn quit(&self) {}

    fn ask_before_quit(&self, _ask: bool) {}
}

type ChangedListener = dyn Fn(&SuspensionChangedEvent) + Send + Sync;
type EventListener = dyn Fn(&SuspensionEvent) + Send + Sync;
type QuitRequestListener = dyn Fn() + Send + Sync;

/// The app's lifecycle, activation, visibility and a desktop's window and wakes
/// ([`SuspensionSnapshot`]), from the events the UI layer publishes (`rayx_app` publishes NSH's
/// app state, the window's, the system's wakes and the app's quitting), with listeners for every
/// event and for changes. Work that pauses in the background listens to it; work that must not
/// be lost listens for [`SuspensionEventKind::Destroying`], and an app that asks before it quits
/// registers with [`ask_before_quit`](Self::ask_before_quit).
#[derive(Clone, AppService)]
pub struct SuspensionService {
    current: Arc<Mutex<SuspensionSnapshot>>,
    changed: Listeners<ChangedListener>,
    events: Listeners<EventListener>,
    quit_requests: Listeners<QuitRequestListener>,
    host: Arc<dyn SuspensionHost>,
}

impl SuspensionService {
    pub fn new() -> Self {
        Self::with_host(Arc::new(NoopSuspensionHost))
    }

    /// A service that quits and asks before quitting through `host`.
    pub fn with_host(host: Arc<dyn SuspensionHost>) -> Self {
        Self {
            current: Arc::default(),
            changed: Listeners::default(),
            events: Listeners::default(),
            quit_requests: Listeners::default(),
            host,
        }
    }

    /// The snapshot now.
    pub fn current(&self) -> SuspensionSnapshot {
        *self.current.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Applies `event`: every event listener hears it, and the change listeners hear it when
    /// it changed the lifecycle, activation, visibility, window or wakes. A
    /// [`SuspensionEventKind::QuitRequested`] goes to the quit request listeners too
    /// ([`ask_before_quit`](Self::ask_before_quit)). Returns whether the state changed.
    pub fn publish(&self, event: SuspensionEvent) -> bool {
        let changed = {
            let mut current = self.current.lock().unwrap_or_else(PoisonError::into_inner);
            let previous = *current;
            *current = previous.apply(&event);
            (!previous.same_state(&current)).then_some(SuspensionChangedEvent {
                previous,
                current: *current,
                trigger: event,
            })
        };
        self.events.notify(|listener| listener(&event));
        if let Some(changed) = &changed {
            self.changed.notify(|listener| listener(changed));
        }
        if event.kind == SuspensionEventKind::QuitRequested {
            self.quit_requests.notify(|listener| listener());
        }
        changed.is_some()
    }

    /// Calls `listener` with every change of state until the subscription is dropped.
    pub fn subscribe_changed(
        &self,
        listener: impl Fn(&SuspensionChangedEvent) + Send + Sync + 'static,
    ) -> ChangeSubscription {
        self.changed.add(Arc::new(listener))
    }

    /// Calls `listener` with every event until the subscription is dropped.
    pub fn subscribe_events(
        &self,
        listener: impl Fn(&SuspensionEvent) + Send + Sync + 'static,
    ) -> ChangeSubscription {
        self.events.add(Arc::new(listener))
    }

    /// Until the subscription is dropped, the user's requests to quit (the app's Quit item, its
    /// quit key) no longer quit the app: `listener` hears each one, and the app asks the user,
    /// saves or waits for its work, then calls [`quit`](Self::quit). Without a listener they
    /// quit as before. The system ending the app (logging out) never asks: the app hears
    /// [`SuspensionEventKind::Destroying`] then. Desktops only; elsewhere nothing asks.
    pub fn ask_before_quit(
        &self,
        listener: impl Fn() + Send + Sync + 'static,
    ) -> ChangeSubscription {
        let subscription = self.quit_requests.add(Arc::new(listener));
        self.host.ask_before_quit(true);
        let (listeners, host) = (self.quit_requests.clone(), Arc::clone(&self.host));
        ChangeSubscription::new(move || {
            drop(subscription);
            if listeners.is_empty() {
                host.ask_before_quit(false);
            }
        })
    }

    /// Whether the user's requests to quit come to the app ([`ask_before_quit`](Self::ask_before_quit)).
    pub fn asks_before_quit(&self) -> bool {
        !self.quit_requests.is_empty()
    }

    /// Quits the app, whether or not it asks before quitting: what a quit request's listener
    /// calls once the app is ready to go. A desktop's app quits (the app hears
    /// [`SuspensionEventKind::Destroying`] and [`SuspensionEventKind::Destroyed`]); elsewhere
    /// the system decides, and nothing happens.
    pub fn quit(&self) {
        self.host.quit();
    }
}

impl Default for SuspensionService {
    fn default() -> Self {
        Self::new()
    }
}
