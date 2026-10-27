use super::{SelectionAreaId, SelectionStatus};
use crate::services::navigation::{NavigationService, NavigationSubscription};
use futures::FutureExt as _;
use futures::future::LocalBoxFuture;
use rayx_core::ChangeSubscription;
use rayx_core::change::Listeners;
use rayx_core::di::{
    AppService, AsyncInitializable, StartupInitializable, StartupInitializationContext,
    StartupInitializationResult, StartupInitializationState, StartupServiceType,
};
use std::sync::{
    Arc, RwLock,
    atomic::{AtomicU64, Ordering},
};

type SelectionListener = dyn Fn(Option<SelectionAreaId>, SelectionStatus) + Send + Sync;

/// Publishes application-wide selection invalidation and active-area summary state.
#[derive(Clone, AppService)]
#[app_service(startup)]
pub struct SelectionService {
    shared: Arc<SelectionServiceState>,
    _navigation_subscription: Arc<NavigationSubscription>,
    initialization: StartupInitializationState,
}

impl SelectionService {
    pub fn new(navigation: NavigationService) -> Self {
        let shared = Arc::new(SelectionServiceState::default());
        let shared_on_navigation = shared.clone();
        let subscription = navigation
            .subscribe_route_changed(move |_| {
                shared_on_navigation
                    .navigation_revision
                    .fetch_add(1, Ordering::AcqRel);
                shared_on_navigation.publish(None, SelectionStatus::default());
            })
            .unwrap_or_else(|error| {
                panic!("SelectionService could not subscribe to navigation: {error}")
            });

        Self {
            shared,
            _navigation_subscription: Arc::new(subscription),
            initialization: StartupInitializationState::default(),
        }
    }

    /// Monotonically increases whenever navigation invalidates active selection state.
    pub fn navigation_revision(&self) -> u64 {
        self.shared.navigation_revision.load(Ordering::Acquire)
    }

    /// Monotonically increases whenever the active area identity changes.
    pub fn active_area_revision(&self) -> u64 {
        self.shared.active_area_revision.load(Ordering::Acquire)
    }

    /// Monotonically increases whenever the published selection status changes.
    pub fn status_revision(&self) -> u64 {
        self.shared.status_revision.load(Ordering::Acquire)
    }

    /// Returns the application summary identity of the currently active area.
    pub fn active_area(&self) -> Option<SelectionAreaId> {
        *read_lock(&self.shared.active_area)
    }

    /// Returns the application summary status of the active area's selection.
    pub fn status(&self) -> SelectionStatus {
        *read_lock(&self.shared.status)
    }

    /// Publishes area-aware status without storing concrete participant state.
    pub fn publish_status(&self, active_area: SelectionAreaId, status: SelectionStatus) {
        self.shared.publish_if_active(active_area, status);
    }

    /// Makes an area the current publisher without moving component selection state.
    pub fn activate(&self, active_area: SelectionAreaId, status: SelectionStatus) {
        self.shared.publish(Some(active_area), status);
    }

    /// Clears one area without allowing a stale area to overwrite a newer owner.
    pub fn deactivate(&self, active_area: SelectionAreaId) {
        if *read_lock(&self.shared.active_area) == Some(active_area) {
            self.shared.publish(None, SelectionStatus::default());
        }
    }

    /// Clears the active-area summary and publishes an empty status.
    pub fn clear(&self) {
        self.shared.publish(None, SelectionStatus::default());
    }

    /// Calls `listener` with the active area and its status whenever either changes, until the
    /// subscription is dropped.
    pub fn subscribe(
        &self,
        listener: impl Fn(Option<SelectionAreaId>, SelectionStatus) + Send + Sync + 'static,
    ) -> ChangeSubscription {
        self.shared.listeners.add(Arc::new(listener))
    }
}

impl AsyncInitializable for SelectionService {
    fn is_initialized(&self) -> bool {
        self.initialization.is_initialized()
    }

    fn initialize(
        &self,
        _context: StartupInitializationContext,
    ) -> LocalBoxFuture<'static, StartupInitializationResult> {
        let service = self.clone();
        async move {
            service.initialization.mark_initialized();
            Ok(())
        }
        .boxed_local()
    }
}

impl StartupInitializable for SelectionService {
    fn startup_service_type(&self) -> StartupServiceType {
        StartupServiceType::of::<Self>()
    }
}

#[derive(Default)]
pub(super) struct SelectionServiceState {
    pub(super) navigation_revision: AtomicU64,
    pub(super) active_area_revision: AtomicU64,
    pub(super) status_revision: AtomicU64,
    pub(super) active_area: RwLock<Option<SelectionAreaId>>,
    pub(super) status: RwLock<SelectionStatus>,
    listeners: Listeners<SelectionListener>,
}

impl SelectionServiceState {
    pub(super) fn publish_if_active(&self, active_area: SelectionAreaId, status: SelectionStatus) {
        if *read_lock(&self.active_area) == Some(active_area) {
            self.publish(Some(active_area), status);
        }
    }

    pub(super) fn publish(&self, active_area: Option<SelectionAreaId>, status: SelectionStatus) {
        let mut changed = false;
        let mut current_area = write_lock(&self.active_area);
        if *current_area != active_area {
            *current_area = active_area;
            self.active_area_revision.fetch_add(1, Ordering::AcqRel);
            changed = true;
        }
        drop(current_area);

        let mut current_status = write_lock(&self.status);
        if *current_status != status {
            *current_status = status;
            self.status_revision.fetch_add(1, Ordering::AcqRel);
            changed = true;
        }
        drop(current_status);

        if changed {
            // Listeners run outside the locks, so they may read or publish again.
            self.listeners
                .notify(|listener| listener(active_area, status));
        }
    }
}

pub(super) fn read_lock<T>(lock: &RwLock<T>) -> std::sync::RwLockReadGuard<'_, T> {
    lock.read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn write_lock<T>(lock: &RwLock<T>) -> std::sync::RwLockWriteGuard<'_, T> {
    lock.write()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}
