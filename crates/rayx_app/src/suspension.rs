//! The app's quitting on NSH, for the suspension service: [`NshSuspensionHost`] quits with NSH's
//! `quit` and, while the app asks before it quits, takes the user's requests with
//! `on_quit_request`; [`keep_quitting`] publishes the app's quit (`on_app_quit`) and its
//! requests, and writes what the app's storage still waits to write before the app goes.
//!
//! NSH's quitting is reachable only while the engine runs Rust code (a component, an effect, a
//! task), and the suspension service may be called on any thread, so the host keeps what it was
//! asked and a task on the engine thread does it.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use futures::StreamExt as _;
use futures::channel::mpsc::{UnboundedSender, unbounded};
use nsh_core::element::desktop::{on_app_quit, on_quit_request, quit};
use nsh_core::prelude::{effect, signal, spawn, try_global};
use rayx_core::ServiceProvider;
use rayx_core::di::AppService;
use rayx_runtime::services::suspension::{
    SuspensionEvent, SuspensionEventKind, SuspensionEventReason, SuspensionHost, SuspensionService,
};

use crate::{NshKeyValueStore, app_services};

/// The suspension service's host on NSH: [`SuspensionService::quit`] quits the app as its Quit
/// item does (a desktop's; elsewhere nothing happens), and while the app asks before it quits
/// ([`SuspensionService::ask_before_quit`]) its Quit item and quit key come to it as
/// [`SuspensionEventKind::QuitRequested`]. The NSH layer registers it as the app's
/// `dyn SuspensionHost`.
#[derive(Clone, Default, AppService)]
pub struct NshSuspensionHost {
    shared: Arc<Shared>,
}

#[derive(Default)]
struct Shared {
    /// Whether the app asks before it quits.
    ask: AtomicBool,
    /// A quit that waits for the engine thread.
    quit: AtomicBool,
    /// Wakes the task on the engine thread that does what waits.
    wake: Mutex<Option<UnboundedSender<()>>>,
}

/// A type for `try_global` to probe the reactive runtime with.
#[derive(Default)]
struct Probe;

fn reachable() -> bool {
    try_global::<Probe>().is_some()
}

impl NshSuspensionHost {
    pub fn new() -> Self {
        Self::default()
    }

    fn wake(&self) {
        if let Ok(wake) = self.shared.wake.lock()
            && let Some(wake) = wake.as_ref()
        {
            let _ = wake.unbounded_send(());
        }
    }

    /// Hands the host the engine thread's task (`wake`), which then does what waits.
    fn connect(&self, wake: UnboundedSender<()>) {
        let _ = wake.unbounded_send(());
        if let Ok(mut slot) = self.shared.wake.lock() {
            *slot = Some(wake);
        }
    }
}

impl SuspensionHost for NshSuspensionHost {
    fn quit(&self) {
        if reachable() {
            quit();
        } else {
            self.shared.quit.store(true, Ordering::SeqCst);
            self.wake();
        }
    }

    fn ask_before_quit(&self, ask: bool) {
        self.shared.ask.store(ask, Ordering::SeqCst);
        self.wake();
    }
}

/// The surface's quitting, under the current owner: once [`QuitWiring::connect`] gives it the
/// ready root, the root's host does its quits and asks before quitting on the engine thread, a
/// request to quit is published to the root's suspension service, and as the app quits the
/// service hears `Destroying`, the app's storage writes what waits, and the service hears
/// `Destroyed`.
pub(crate) struct QuitWiring {
    host: Rc<RefCell<Option<NshSuspensionHost>>>,
    wake: UnboundedSender<()>,
}

/// Starts the surface's quitting (see [`QuitWiring`]), where the engine runs Rust code.
pub(crate) fn keep_quitting() -> QuitWiring {
    let host: Rc<RefCell<Option<NshSuspensionHost>>> = Rc::default();
    let asks = signal(false);
    let (wake, mut woken) = unbounded::<()>();
    let connected = Rc::clone(&host);
    drop(spawn(async move {
        while woken.next().await.is_some() {
            let Some(host) = connected.borrow().clone() else {
                continue;
            };
            asks.set(host.shared.ask.load(Ordering::SeqCst));
            if host.shared.quit.swap(false, Ordering::SeqCst) {
                quit();
            }
        }
    }));
    // Registered while the app asks: NSH quits on its Quit item without a listener.
    effect(move || {
        if asks.get() {
            on_quit_request(|| {
                if let Some(suspension) = suspension() {
                    suspension.publish(SuspensionEvent::new(
                        SuspensionEventKind::QuitRequested,
                        SuspensionEventReason::UserAskedToQuit,
                    ));
                }
            });
        }
    });
    on_app_quit(|| {
        if let Ok(root) = app_services() {
            stop(&root);
        }
        std::future::ready(())
    });
    QuitWiring { host, wake }
}

impl QuitWiring {
    /// Gives the wiring the ready root's host, once.
    pub(crate) fn connect(&self, root: &ServiceProvider) {
        if self.host.borrow().is_some() {
            return;
        }
        if let Some(host) = root.service::<NshSuspensionHost>() {
            host.connect(self.wake.clone());
            *self.host.borrow_mut() = Some((*host).clone());
        }
    }
}

fn suspension() -> Option<Arc<SuspensionService>> {
    app_services()
        .ok()
        .and_then(|root| root.service::<SuspensionService>())
}

/// The app quits: the suspension service hears it, then the storage writes what waits (what the
/// app saved as it heard `Destroying` included).
fn stop(root: &ServiceProvider) {
    let suspension = root.service::<SuspensionService>();
    if let Some(suspension) = &suspension {
        suspension.publish(SuspensionEvent::new(
            SuspensionEventKind::Destroying,
            SuspensionEventReason::HostStopping,
        ));
    }
    if let Some(store) = root.service::<NshKeyValueStore>() {
        let _ = store.flush();
    }
    if let Some(suspension) = &suspension {
        suspension.publish(SuspensionEvent::new(
            SuspensionEventKind::Destroyed,
            SuspensionEventReason::HostStopped,
        ));
    }
}
