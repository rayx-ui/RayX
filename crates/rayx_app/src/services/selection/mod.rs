//! Selection: the app-wide summary of the active selection area (which area publishes, how much
//! it has selected), cleared when navigation changes the route, published by a
//! [`SelectionService`] implementation as [`SelectionState`]. It holds no selected content; the
//! components that own a selection publish its summary.
//!
//! RayX's implementation is `rayx_services_selection`'s (`add_selection()`), which follows the
//! app's `dyn NavigationService`; an app may register its own, in Rust or in TS. Reading the
//! state ([`SelectionState::active_area`], [`SelectionState::status`], the revisions) is
//! synchronous for any implementation, and so are the same helpers on `dyn SelectionService`.

mod selection_area_id;
mod selection_status;

use std::fmt;
use std::sync::{Arc, Mutex, PoisonError};

use serde::{Deserialize, Serialize};

pub use selection_area_id::SelectionAreaId;
pub use selection_status::SelectionStatus;

use super::ContractFailure;
use crate::ChangeSubscription;
use crate::published::StateCell;

/// The selection now: the active area and its status, and revisions that say what changed.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct SelectionState {
    /// The area that publishes now, `None` when none is active.
    pub active_area: Option<SelectionAreaId>,
    /// The active area's status (empty when none is active).
    pub status: SelectionStatus,
    /// Increases whenever navigation clears the selection.
    pub navigation_revision: u64,
    /// Increases whenever the active area changes.
    pub active_area_revision: u64,
    /// Increases whenever the status changes.
    pub status_revision: u64,
}

/// Why a selection command failed: it did not reach its implementation, or the implementation
/// failed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SelectionError {
    /// The call failed (a TS implementation threw, the engine is gone).
    Failed(ContractFailure),
}

impl fmt::Display for SelectionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Failed(failure) => failure.fmt(f),
        }
    }
}

impl std::error::Error for SelectionError {}

impl From<ContractFailure> for SelectionError {
    fn from(failure: ContractFailure) -> Self {
        Self::Failed(failure)
    }
}

/// The selection service: which selection area publishes and its content-free status, published
/// as [`SelectionState`]. Only the active area publishes, and every route change clears it.
#[rayx_app_macros::contract(ts = "SelectionServiceContract")]
pub trait SelectionService {
    /// The active area, its status and the revisions, now and as they change.
    #[state(ts = "SelectionState")]
    fn selection(&self) -> &StateCell<SelectionState>;

    /// Publishes `status` for `active_area` when it is the active area (a stale area's status is
    /// dropped).
    #[ts("(activeArea: SelectionAreaId, status: SelectionStatus)")]
    async fn publish_status(
        &self,
        active_area: SelectionAreaId,
        status: SelectionStatus,
    ) -> Result<(), SelectionError>;

    /// Makes `active_area` the area that publishes, with `status`, without moving any component's
    /// selection.
    #[ts("(activeArea: SelectionAreaId, status: SelectionStatus)")]
    async fn activate(
        &self,
        active_area: SelectionAreaId,
        status: SelectionStatus,
    ) -> Result<(), SelectionError>;

    /// Clears the selection when `active_area` is the active area, so a stale area never clears
    /// a newer one's.
    #[ts("(activeArea: SelectionAreaId)")]
    async fn deactivate(&self, active_area: SelectionAreaId) -> Result<(), SelectionError>;

    /// Clears the active area and its status.
    async fn clear(&self) -> Result<(), SelectionError>;
}

impl dyn SelectionService {
    /// The area that publishes now.
    pub fn active_area(&self) -> Option<SelectionAreaId> {
        self.selection().get().active_area
    }

    /// The active area's status.
    pub fn status(&self) -> SelectionStatus {
        self.selection().get().status
    }

    /// Increases whenever navigation clears the selection.
    pub fn navigation_revision(&self) -> u64 {
        self.selection().get().navigation_revision
    }

    /// Increases whenever the active area changes.
    pub fn active_area_revision(&self) -> u64 {
        self.selection().get().active_area_revision
    }

    /// Increases whenever the status changes.
    pub fn status_revision(&self) -> u64 {
        self.selection().get().status_revision
    }

    /// Calls `listener` with the active area and its status whenever either changes, until the
    /// subscription is dropped.
    pub fn subscribe(
        &self,
        listener: impl Fn(Option<SelectionAreaId>, SelectionStatus) + Send + Sync + 'static,
    ) -> ChangeSubscription {
        let state = self.selection().get();
        let last = Mutex::new((state.active_area, state.status));
        self.selection()
            .subscribe(move |state: &Arc<SelectionState>| {
                let now = (state.active_area, state.status);
                {
                    let mut last = last.lock().unwrap_or_else(PoisonError::into_inner);
                    if *last == now {
                        return;
                    }
                    *last = now;
                }
                listener(now.0, now.1);
            })
    }
}
