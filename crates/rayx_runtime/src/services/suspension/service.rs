use std::sync::{Arc, Mutex, PoisonError};

use rayx_core::ChangeSubscription;
use rayx_core::change::Listeners;
use rayx_core::di::AppService;

use super::{SuspensionEvent, SuspensionSnapshot};

/// A change of the app's lifecycle, activation or visibility.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SuspensionChangedEvent {
    pub previous: SuspensionSnapshot,
    pub current: SuspensionSnapshot,
    /// The event that changed it.
    pub trigger: SuspensionEvent,
}

type ChangedListener = dyn Fn(&SuspensionChangedEvent) + Send + Sync;
type EventListener = dyn Fn(&SuspensionEvent) + Send + Sync;

/// The app's lifecycle, activation and visibility ([`SuspensionSnapshot`]), from the events the
/// UI layer publishes (`rayx_app` publishes NSH's app state), with listeners for every event
/// and for changes. Work that pauses in the background listens to it.
#[derive(Clone, AppService)]
pub struct SuspensionService {
    current: Arc<Mutex<SuspensionSnapshot>>,
    changed: Listeners<ChangedListener>,
    events: Listeners<EventListener>,
}

impl SuspensionService {
    pub fn new() -> Self {
        Self {
            current: Arc::default(),
            changed: Listeners::default(),
            events: Listeners::default(),
        }
    }

    /// The snapshot now.
    pub fn current(&self) -> SuspensionSnapshot {
        *self.current.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Applies `event`: every event listener hears it, and the change listeners hear it when
    /// it changed the lifecycle, activation or visibility. Returns whether it did.
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
}

impl Default for SuspensionService {
    fn default() -> Self {
        Self::new()
    }
}
