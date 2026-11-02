//! The app's location on NSH: the navigation service's location shown as NSH's (`location()`,
//! `navigate()`), which the web keeps in the address bar and the browser's history and an
//! embedding app hears through its host channel, and the platform's moves of it (the browser's
//! back and forward, a typed address, the embedding app, a deep link) followed by the navigation.
//!
//! NSH's location is reachable only while the engine runs Rust code (a component, an effect, a
//! task), and a class's synchronous calls from TSX run outside it, as does any other thread. So
//! a navigation's location waits for a task on the engine thread to show it, and the navigations
//! made until the task runs share one entry of the platform's history, as what an app shows in
//! one turn does on NSH: the task shows the latest location, as a new entry when any of them was
//! a push. One `navigate` a turn also keeps NSH's location from going back through earlier ones
//! when it commits the turn's navigations.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use futures::StreamExt as _;
use futures::channel::mpsc::{UnboundedSender, unbounded};
use nsh_core::location::{HistoryBehavior, Location};
use nsh_core::prelude::{Owner, effect, location, navigate, spawn, try_global, untracked};
use rayx_core::di::AppService;
use rayx_runtime::services::navigation::{
    NavigationError, NavigationHistoryMode, NavigationLocationSynchronizer, NavigationService,
    Result,
};

use crate::{AppState, app_services, app_state};

/// Shows the navigation's location as NSH's and follows the platform's moves of it. The NSH
/// layer registers it as the app's `dyn NavigationLocationSynchronizer`, so `add_navigation`
/// starts at NSH's location (a deep link, the address bar) and shows every navigation there: a
/// push as a new entry of the platform's history, the other modes in place of the current one.
/// Keep the navigation's `base_path` empty: NSH's location is a path from the app's base already
/// (on the web, `nsh.json`'s `web.base`).
#[derive(Clone, Default, AppService)]
pub struct NshLocationSynchronizer {
    shared: Arc<Shared>,
}

#[derive(Default)]
struct Shared {
    /// The latest location to show, and how: a push when any navigation since the last one shown
    /// was.
    pending: Mutex<Option<(String, HistoryBehavior)>>,
    /// Wakes the task that shows it.
    flusher: Mutex<Option<UnboundedSender<()>>>,
    /// Where the latest location shown took NSH's: the move the effect that follows the platform
    /// skips, since the navigation made it.
    shown: Mutex<Option<Location>>,
    /// Whether the task and the effect run.
    started: AtomicBool,
}

/// A type for `try_global` to probe the reactive runtime with.
#[derive(Default)]
struct Probe;

fn reachable() -> bool {
    try_global::<Probe>().is_some()
}

impl NshLocationSynchronizer {
    /// The synchronizer of the surface the engine runs it on.
    pub fn new() -> Self {
        Self::default()
    }

    /// Where NSH's location is reachable: shows the location that waits. A location NSH refuses
    /// (not a path from the app's base) is the error.
    pub fn flush(&self) -> Result<()> {
        if !reachable() {
            return Ok(());
        }
        let Some((url, history)) = self
            .shared
            .pending
            .lock()
            .map_err(|_| NavigationError::Poisoned)?
            .take()
        else {
            return Ok(());
        };
        let refused = |error: nsh_core::location::InvalidLocation| {
            NavigationError::Location(error.message().to_owned())
        };
        // Noted before it moves, as NSH resolves it: the effect runs as the location changes.
        let current = untracked(location);
        let next = current.join(&url).map_err(refused)?;
        if next == current {
            return Ok(());
        }
        if let Ok(mut shown) = self.shared.shown.lock() {
            *shown = Some(next);
        }
        navigate(&url, history).map_err(refused)
    }

    /// Starts, once and where the engine runs Rust code, the task that shows the waiting location
    /// and the effect that follows NSH's; both live as long as the surface.
    fn start(&self) {
        if self.shared.started.swap(true, Ordering::SeqCst) {
            return;
        }
        let (wake, mut woken) = unbounded::<()>();
        if let Ok(mut flusher) = self.shared.flusher.lock() {
            *flusher = Some(wake);
        }
        let synchronizer = self.clone();
        Owner::detached().run(move || {
            let flusher = synchronizer.clone();
            drop(spawn(async move {
                while woken.next().await.is_some() {
                    let _ = flusher.flush();
                }
            }));
            effect(move || follow_platform(&synchronizer.shared));
        });
    }

    fn wake(&self) {
        if let Ok(flusher) = self.shared.flusher.lock()
            && let Some(flusher) = flusher.as_ref()
        {
            let _ = flusher.unbounded_send(());
        }
    }
}

/// Takes the navigation where NSH's location says once the app is ready, and again on every
/// move of the platform's. The navigation's own moves come back here too and are skipped.
fn follow_platform(shared: &Shared) {
    // Reading these subscribes: the effect runs again once the app is ready and on every move.
    if app_state() != AppState::Ready {
        return;
    }
    let location = location();
    let shown = shared.shown.lock().ok().and_then(|mut shown| shown.take());
    if shown.as_ref() == Some(&location) {
        return;
    }
    untracked(|| {
        let Some(navigation) = app_services()
            .ok()
            .and_then(|root| root.service::<NavigationService>())
        else {
            return;
        };
        if navigation.options().synchronize_location {
            let _ = navigation.apply_location(location.as_str());
        }
    });
}

impl NavigationLocationSynchronizer for NshLocationSynchronizer {
    /// NSH's location as the app starts; the navigation's startup asks for it on the engine
    /// thread, which also starts following it.
    fn location(&self) -> Option<String> {
        if !reachable() {
            return None;
        }
        self.start();
        let _ = self.flush();
        Some(untracked(location).as_str().to_owned())
    }

    /// Shows `location` once a task on the engine thread runs (see the module).
    fn synchronize(&self, location: &str, history_mode: NavigationHistoryMode) -> Result<()> {
        {
            let mut pending = self
                .shared
                .pending
                .lock()
                .map_err(|_| NavigationError::Poisoned)?;
            let push = history_mode == NavigationHistoryMode::Push
                || pending
                    .as_ref()
                    .is_some_and(|(_, history)| *history == HistoryBehavior::Push);
            let history = if push {
                HistoryBehavior::Push
            } else {
                HistoryBehavior::Replace
            };
            *pending = Some((location.to_owned(), history));
        }
        if reachable() {
            self.start();
        }
        self.wake();
        Ok(())
    }
}
