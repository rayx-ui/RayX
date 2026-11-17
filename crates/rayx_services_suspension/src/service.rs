//! RayX's suspension service on NSH: the app's lifecycle, a desktop's window and the system's
//! wakes, followed from NSH's app state from its startup initialization on; quitting with NSH's
//! `quit`, and the user's requests to quit while the app asks first.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError, TryLockError};

use futures::FutureExt as _;
use futures::StreamExt as _;
use futures::channel::mpsc::{UnboundedSender, unbounded};
use futures::future::{BoxFuture, LocalBoxFuture};
use nsh_core::element::desktop::{on_app_quit, on_quit_request, quit, window_state};
use nsh_core::element::reactive::create_root;
use nsh_core::platform::Lifecycle;
use nsh_core::prelude::{Owner, effect, lifecycle, signal, spawn, wake_count};
use rayx_app::di::{
    AsyncInitializable, StartupInitializable, StartupInitializationContext,
    StartupInitializationResult, StartupServiceType,
};
use rayx_app::services::suspension::{
    SuspensionError, SuspensionEvent, SuspensionEventKind, SuspensionEventReason,
    SuspensionService, SuspensionSnapshot, WindowActivity,
};
use rayx_app::{EventStream, StateCell};

/// What the engine thread's task does for the service.
enum Order {
    /// Quit the app.
    Quit,
    /// Take the user's requests to quit (or stop).
    Ask(bool),
}

/// The app's lifecycle on the surface whose provider holds this service.
pub(crate) struct NshSuspension {
    suspension: StateCell<SuspensionSnapshot>,
    /// The service's state, changed one change at a time (an event, or the app asking before it
    /// quits, from any thread), and published from here.
    state: Mutex<SuspensionSnapshot>,
    /// Held by the thread publishing the state.
    publishing: Mutex<()>,
    /// Whether the state changed since it was last published.
    stale: AtomicBool,
    events: EventStream<SuspensionEvent>,
    /// The engine thread's task, once the service initialized.
    orders: Mutex<Option<UnboundedSender<Order>>>,
    /// Orders given before it initialized.
    waiting: Mutex<Vec<Order>>,
    following: AtomicBool,
}

impl NshSuspension {
    pub(crate) fn new() -> Arc<Self> {
        Arc::new(Self {
            suspension: StateCell::new(SuspensionSnapshot::default()),
            state: Mutex::new(SuspensionSnapshot::default()),
            publishing: Mutex::new(()),
            stale: AtomicBool::new(false),
            events: EventStream::new(),
            orders: Mutex::new(None),
            waiting: Mutex::new(Vec::new()),
            following: AtomicBool::new(false),
        })
    }

    /// Applies `event`: the state is published when it changed, and every subscriber hears the
    /// event.
    pub(crate) fn publish(&self, event: SuspensionEvent) {
        self.change(|state| state.apply(&event));
        self.events.publish(event);
    }

    /// Changes the state with `change` and publishes it. Changes are made one at a time, so one
    /// made on another thread at once (an event on the engine thread, the app asking before it
    /// quits on its own) is never lost.
    fn change(&self, change: impl FnOnce(SuspensionSnapshot) -> SuspensionSnapshot) {
        {
            let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
            *state = change(*state);
        }
        self.publish_state();
    }

    /// Publishes the state now, unless it is the one published: a change made while it is
    /// published (on another thread, or by a subscriber) is published right after, by whoever
    /// is publishing.
    fn publish_state(&self) {
        self.stale.store(true, Ordering::SeqCst);
        loop {
            let publishing = match self.publishing.try_lock() {
                Ok(guard) => guard,
                Err(TryLockError::Poisoned(poisoned)) => poisoned.into_inner(),
                Err(TryLockError::WouldBlock) => return,
            };
            self.stale.store(false, Ordering::SeqCst);
            let state = *self.state.lock().unwrap_or_else(PoisonError::into_inner);
            if *self.suspension.get() != state {
                self.suspension.publish(state);
            }
            drop(publishing);
            if !self.stale.load(Ordering::SeqCst) {
                return;
            }
        }
    }

    /// Gives `order` to the engine thread's task (kept until the service initialized).
    fn order(&self, order: Order) {
        let orders = self.orders.lock().unwrap_or_else(PoisonError::into_inner);
        match orders.as_ref() {
            Some(sender) => {
                let _ = sender.unbounded_send(order);
            }
            None => self
                .waiting
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(order),
        }
    }

    /// Follows NSH's app state, window and the system's wakes, takes the user's requests to quit
    /// while the app asks, and publishes the app's quit, for the surface's life. On the engine
    /// thread, where NSH runs Rust code (the startup run).
    fn follow(self: &Arc<Self>) {
        if self.following.swap(true, Ordering::SeqCst) {
            return;
        }
        let service = Arc::clone(self);
        // A root of its own, never disposed: under no owner NSH drops what `on_app_quit` keeps
        // (its cleanup) at once, and the app's quit would go unheard.
        Owner::detached().run(|| {
            create_root(move || {
                let asks = signal(false);
                let (sender, mut orders) = unbounded::<Order>();
                for order in std::mem::take(
                    &mut *service
                        .waiting
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner),
                ) {
                    let _ = sender.unbounded_send(order);
                }
                *service
                    .orders
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner) = Some(sender);
                drop(spawn(async move {
                    while let Some(order) = orders.next().await {
                        match order {
                            Order::Quit => quit(),
                            Order::Ask(ask) => asks.set(ask),
                        }
                    }
                }));
                // Registered while the app asks: NSH quits on its Quit item without a listener.
                let requests = Arc::clone(&service);
                effect(move || {
                    if asks.get() {
                        let requests = Arc::clone(&requests);
                        on_quit_request(move || {
                            requests.publish(SuspensionEvent::new(
                                SuspensionEventKind::QuitRequested,
                                SuspensionEventReason::UserAskedToQuit,
                            ));
                        });
                    }
                });
                let quitting = Arc::clone(&service);
                on_app_quit(move || {
                    // What must not be lost is saved as subscribers hear `Destroying` (the storage
                    // writes what waits then).
                    quitting.publish(SuspensionEvent::new(
                        SuspensionEventKind::Destroying,
                        SuspensionEventReason::HostStopping,
                    ));
                    quitting.publish(SuspensionEvent::new(
                        SuspensionEventKind::Destroyed,
                        SuspensionEventReason::HostStopped,
                    ));
                    std::future::ready(())
                });
                service.publish(SuspensionEvent::new(
                    SuspensionEventKind::Started,
                    SuspensionEventReason::HostStarted,
                ));
                let seen = Rc::new(Seen::default());
                effect(move || follow_app_state(&service, &seen));
            })
        });
    }
}

/// What the app state's effect saw last.
#[derive(Default)]
struct Seen {
    state: Cell<Option<Lifecycle>>,
    window: Cell<Option<WindowActivity>>,
    wakes: Cell<Option<u32>>,
}

/// Publishes what changed of NSH's app state, a desktop's window and the system's wakes.
fn follow_app_state(service: &NshSuspension, seen: &Seen) {
    // Read before anything returns, so the effect runs again after each wake.
    let wakes = wake_count();
    let window = window_state().map(|state| WindowActivity {
        focused: state.focused,
        minimized: state.minimized,
        visible: state.visible,
    });
    if let Some(window) = window
        && seen.window.replace(Some(window)) != Some(window)
    {
        service.publish(SuspensionEvent::window_changed(
            window,
            SuspensionEventReason::WindowStateChanged,
        ));
    }
    // The wakes since the service initialized (those before are where the app started).
    if let Some(before) = seen.wakes.replace(Some(wakes)) {
        for _ in before..wakes {
            service.publish(SuspensionEvent::new(
                SuspensionEventKind::Woke,
                SuspensionEventReason::SystemWoke,
            ));
        }
    }
    let state = lifecycle();
    let previous = seen.state.replace(Some(state));
    if previous == Some(state) {
        return;
    }
    let publish = |kind| {
        service.publish(SuspensionEvent::new(
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

impl SuspensionService for NshSuspension {
    fn suspension(&self) -> &StateCell<SuspensionSnapshot> {
        &self.suspension
    }

    fn events(&self) -> &EventStream<SuspensionEvent> {
        &self.events
    }

    fn quit(&self) -> BoxFuture<'_, Result<(), SuspensionError>> {
        self.order(Order::Quit);
        Box::pin(futures::future::ready(Ok(())))
    }

    fn ask_before_quit(&self, ask: bool) -> BoxFuture<'_, Result<(), SuspensionError>> {
        self.change(|state| SuspensionSnapshot {
            asks_before_quit: ask,
            ..state
        });
        self.order(Order::Ask(ask));
        Box::pin(futures::future::ready(Ok(())))
    }
}

/// The service's startup role: it follows NSH from the app's start on.
pub(crate) struct Following(pub(crate) Arc<NshSuspension>);

impl AsyncInitializable for Following {
    fn is_initialized(&self) -> bool {
        self.0.following.load(Ordering::SeqCst)
    }

    fn initialize(
        &self,
        _context: StartupInitializationContext,
    ) -> LocalBoxFuture<'static, StartupInitializationResult> {
        self.0.follow();
        futures::future::ready(Ok(())).boxed_local()
    }
}

impl StartupInitializable for Following {
    fn startup_service_type(&self) -> Option<StartupServiceType> {
        Some(StartupServiceType::of::<dyn SuspensionService>())
    }
}
