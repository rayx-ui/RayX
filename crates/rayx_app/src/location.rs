//! The app's location on NSH: the navigation's location shown as NSH's app address (`app_url`,
//! `set_app_url_with`; TS's `UrlScheme.url()` / `setUrl`), which the web keeps in the address bar
//! and the browser's history and the native platforms keep for the app, and the platform's moves
//! of it the app did not make (the browser's back and forward, a typed address:
//! `next_app_url_change`) followed by the navigation.
//!
//! The address is a URL: on the web the page's (`https://example.com/app/projects/1`), on the
//! native platforms the link that opened the app (`myapp://projects/1`) or what the app set since.
//! The navigation's location is the part after the origin (`/app/projects/1`), or after the
//! scheme for a link (`/projects/1`); on the web set the navigation's `base_path` to `nsh.json`'s
//! `web.base`. A location the navigation shows is set as it is, which the web resolves against
//! the page's origin.
//!
//! NSH's address is reachable only while the engine runs Rust code (a component, an effect, a
//! task), and a class's synchronous calls from TSX run outside it, as does any other thread. So
//! a navigation's location waits for a task on the engine thread to show it, and the navigations
//! made until the task runs share one entry of the platform's history, as what an app shows in
//! one turn does on NSH: the task shows the latest location, as a new entry when any of them was
//! a push. The address at start is read once the engine runs (an answer, not a value), and the
//! navigation moves there once the app is ready; until then nothing is shown, so the navigation's
//! own start does not replace it.
//!
//! The synchronizer takes NSH's moves of the address (`next_app_url_change`, one listener at a
//! time): with `synchronize_location` on, the app hears them through the navigation, not through
//! TS's `UrlScheme.on("change")`.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use futures::StreamExt as _;
use futures::channel::mpsc::{UnboundedSender, unbounded};
use nsh_core::prelude::{
    Owner, app_url, effect, next_app_url_change, set_app_url_with, signal, spawn, try_global,
    untracked,
};
use nsh_core::url_scheme::HistoryBehavior;
use rayx_core::di::AppService;
use rayx_runtime::services::navigation::{
    NavigationError, NavigationHistoryMode, NavigationLocationSynchronizer, NavigationService,
    Result,
};

use crate::{AppState, app_services, app_state};

/// Shows the navigation's location as NSH's app address and follows the platform's moves of
/// it. The NSH layer registers it as the app's `dyn NavigationLocationSynchronizer`, so the
/// navigation moves to the address the app started at (a deep link, the address bar) once the
/// app is ready, and shows every navigation there: a push as a new entry of the platform's
/// history, the other modes in place of the current one. On the web, the navigation's
/// `base_path` is `nsh.json`'s `web.base` (see the module).
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
    /// The location the address has, as far as the synchronizer knows: the last one it showed
    /// or followed, which it does not show again.
    shown: Mutex<Option<String>>,
    /// Whether the address at start was read and the navigation went there: until then nothing
    /// is shown.
    settled: AtomicBool,
    /// Whether the tasks and the effect run.
    started: AtomicBool,
}

/// A type for `try_global` to probe the reactive runtime with.
#[derive(Default)]
struct Probe;

fn reachable() -> bool {
    try_global::<Probe>().is_some()
}

/// The navigation's location in the app's address `address` (see the module): the path, query
/// and fragment of a web address, what follows the scheme of a link, or the address itself when
/// it is no URL (one the app set on a native platform).
pub fn location_of_address(address: &str) -> String {
    let Ok(url) = url::Url::parse(address) else {
        return address.to_owned();
    };
    if matches!(url.scheme(), "http" | "https") {
        let mut location = url.path().to_owned();
        if let Some(query) = url.query() {
            location.push('?');
            location.push_str(query);
        }
        if let Some(fragment) = url.fragment() {
            location.push('#');
            location.push_str(fragment);
        }
        return location;
    }
    let after_scheme = &address[url.scheme().len() + 1..];
    format!("/{}", after_scheme.trim_start_matches('/'))
}

impl NshLocationSynchronizer {
    /// The synchronizer of the surface the engine runs it on.
    pub fn new() -> Self {
        Self::default()
    }

    /// Where NSH's address is reachable and the address at start was taken: shows the location
    /// that waits. A location NSH refuses (too long, with whitespace, of another origin than the
    /// page's) is the error.
    pub fn flush(&self) -> Result<()> {
        if !reachable() || !self.shared.settled.load(Ordering::SeqCst) {
            return Ok(());
        }
        let Some((location, history)) = self
            .shared
            .pending
            .lock()
            .map_err(|_| NavigationError::Poisoned)?
            .take()
        else {
            return Ok(());
        };
        {
            let mut shown = self
                .shared
                .shown
                .lock()
                .map_err(|_| NavigationError::Poisoned)?;
            if shown.as_deref() == Some(location.as_str()) {
                return Ok(());
            }
            *shown = Some(location.clone());
        }
        set_app_url_with(location, history).map_err(NavigationError::Location)
    }

    /// Starts, once and where the engine runs Rust code, the task that shows the waiting
    /// location, the task that follows NSH's moves of the address, and the effect that takes the
    /// navigation to the address at start once the app is ready; they live as long as the
    /// surface.
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
            // The address at start: `None` until NSH answers, then the address it had, if any.
            let initial = signal(None::<Option<String>>);
            drop(spawn(async move {
                let address = app_url().await;
                initial.set(Some(address));
            }));
            let flusher = synchronizer.clone();
            drop(spawn(async move {
                while woken.next().await.is_some() {
                    let _ = flusher.flush();
                }
            }));
            let follower = synchronizer.clone();
            drop(spawn(async move {
                loop {
                    let change = next_app_url_change().await;
                    if follower.shared.settled.load(Ordering::SeqCst) {
                        untracked(|| follower.follow(&change.url));
                    } else {
                        // Not settled yet: the move is where the navigation starts.
                        initial.set(Some(Some(change.url)));
                    }
                }
            }));
            effect(move || {
                // Reading these subscribes: the effect runs again once the app is ready and once
                // NSH answers.
                if app_state() != AppState::Ready {
                    return;
                }
                let Some(address) = initial.get() else {
                    return;
                };
                if !synchronizer.shared.settled.load(Ordering::SeqCst) {
                    untracked(|| synchronizer.settle(address));
                }
            });
        });
    }

    /// Takes the navigation to the address at start, then shows what waits (the navigation's
    /// own start, when there was no address to go to).
    fn settle(&self, address: Option<String>) {
        if let Some(address) = address
            && let Some(navigation) = navigation()
            && navigation.options().synchronize_location
        {
            let location = location_of_address(&address);
            let there = navigation.location().is_ok_and(|now| now == location)
                || navigation.apply_location(&location).is_ok();
            if there {
                // The platform is there already: what waited from the navigation's start goes.
                if let Ok(mut shown) = self.shared.shown.lock() {
                    *shown = Some(location);
                }
                if let Ok(mut pending) = self.shared.pending.lock() {
                    *pending = None;
                }
            }
        }
        self.shared.settled.store(true, Ordering::SeqCst);
        let _ = self.flush();
    }

    /// Takes the navigation where NSH moved the address (the browser's back or forward, a
    /// typed address). The platform is there already: it is not shown again.
    fn follow(&self, address: &str) {
        let Some(navigation) = navigation() else {
            return;
        };
        if !navigation.options().synchronize_location {
            return;
        }
        let location = location_of_address(address);
        if let Ok(mut shown) = self.shared.shown.lock() {
            *shown = Some(location.clone());
        }
        let _ = navigation.apply_location(&location);
    }

    fn wake(&self) {
        if let Ok(flusher) = self.shared.flusher.lock()
            && let Some(flusher) = flusher.as_ref()
        {
            let _ = flusher.unbounded_send(());
        }
    }
}

/// The ready root's navigation, if it has one.
fn navigation() -> Option<Arc<NavigationService>> {
    app_services()
        .ok()
        .and_then(|root| root.service::<NavigationService>())
}

impl NavigationLocationSynchronizer for NshLocationSynchronizer {
    /// None: NSH answers the address at start only once the engine runs, so the navigation
    /// starts at its default route and moves there once the app is ready. Asked on the engine
    /// thread, it starts the synchronizer.
    fn location(&self) -> Option<String> {
        if reachable() {
            self.start();
        }
        None
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
