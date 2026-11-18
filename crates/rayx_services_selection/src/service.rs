//! RayX's selection service: the active area and its status, published as [`SelectionState`],
//! cleared on every route change of the app's navigation.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError, TryLockError};

use futures::FutureExt as _;
use futures::future::BoxFuture;
use rayx_app::services::navigation::NavigationService;
use rayx_app::services::selection::{
    SelectionAreaId, SelectionError, SelectionService, SelectionState, SelectionStatus,
};
use rayx_app::{ChangeSubscription, StateCell};

/// RayX's selection service: only the active area publishes its status, and every route change
/// of the navigation it follows clears it.
pub(crate) struct RayxSelection {
    shared: Arc<Shared>,
    /// Its subscription to the navigation's route changes, for as long as it lives.
    _navigation: ChangeSubscription,
}

impl RayxSelection {
    /// A service cleared on every route change of `navigation`.
    pub(crate) fn new(navigation: &(dyn NavigationService + 'static)) -> Self {
        let shared = Arc::new(Shared::default());
        let on_navigation = Arc::clone(&shared);
        let subscription = navigation.on_route_changed(move |_| {
            on_navigation.change(|state| {
                state.navigation_revision += 1;
                set(state, None, SelectionStatus::default());
                true
            });
        });
        Self {
            shared,
            _navigation: subscription,
        }
    }

    /// Answers a command: changes the state with `change`, which says whether it changed it.
    fn answer(
        &self,
        change: impl FnOnce(&mut SelectionState) -> bool,
    ) -> BoxFuture<'_, Result<(), SelectionError>> {
        self.shared.change(change);
        futures::future::ready(Ok(())).boxed()
    }
}

/// What the service and its navigation listener share.
#[derive(Default)]
struct Shared {
    /// The latest state, changed under its lock.
    state: Mutex<SelectionState>,
    /// The published state.
    selection: StateCell<SelectionState>,
    /// Held by the thread publishing the state ([`Shared::publish_latest`]).
    publishing: Mutex<()>,
    /// A change was made since the publishing thread read the state.
    stale: AtomicBool,
}

impl Shared {
    /// Changes the state with `change`, which says whether it changed it, and publishes it if
    /// it did.
    fn change(&self, change: impl FnOnce(&mut SelectionState) -> bool) {
        {
            let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
            if !change(&mut state) {
                return;
            }
        }
        self.publish_latest();
    }

    /// Publishes the latest state, outside the state's lock (a subscriber may call the
    /// service). One thread publishes at a time and reads the state when it does, so the
    /// published state is never older than one published before it: a change made while
    /// another thread (or a subscriber of this one) publishes is published right after, by the
    /// publishing thread, once it finds the state stale.
    fn publish_latest(&self) {
        self.stale.store(true, Ordering::SeqCst);
        loop {
            let publishing = match self.publishing.try_lock() {
                Ok(guard) => guard,
                Err(TryLockError::Poisoned(poisoned)) => poisoned.into_inner(),
                Err(TryLockError::WouldBlock) => return,
            };
            self.stale.store(false, Ordering::SeqCst);
            let state = *self.state.lock().unwrap_or_else(PoisonError::into_inner);
            if *self.selection.get() != state {
                self.selection.publish(state);
            }
            drop(publishing);
            if !self.stale.load(Ordering::SeqCst) {
                return;
            }
        }
    }
}

/// Makes `area` the active area with `status`, counting what changed; whether anything did.
fn set(state: &mut SelectionState, area: Option<SelectionAreaId>, status: SelectionStatus) -> bool {
    let mut changed = false;
    if state.active_area != area {
        state.active_area = area;
        state.active_area_revision += 1;
        changed = true;
    }
    if state.status != status {
        state.status = status;
        state.status_revision += 1;
        changed = true;
    }
    changed
}

impl SelectionService for RayxSelection {
    fn selection(&self) -> &StateCell<SelectionState> {
        &self.shared.selection
    }

    fn publish_status(
        &self,
        active_area: SelectionAreaId,
        status: SelectionStatus,
    ) -> BoxFuture<'_, Result<(), SelectionError>> {
        self.answer(|state| {
            state.active_area == Some(active_area) && set(state, Some(active_area), status)
        })
    }

    fn activate(
        &self,
        active_area: SelectionAreaId,
        status: SelectionStatus,
    ) -> BoxFuture<'_, Result<(), SelectionError>> {
        self.answer(|state| set(state, Some(active_area), status))
    }

    fn deactivate(
        &self,
        active_area: SelectionAreaId,
    ) -> BoxFuture<'_, Result<(), SelectionError>> {
        self.answer(|state| {
            state.active_area == Some(active_area) && set(state, None, SelectionStatus::default())
        })
    }

    fn clear(&self) -> BoxFuture<'_, Result<(), SelectionError>> {
        self.answer(|state| set(state, None, SelectionStatus::default()))
    }
}
