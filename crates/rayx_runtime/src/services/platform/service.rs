use std::sync::{Arc, Mutex, PoisonError};

use rayx_core::ChangeSubscription;
use rayx_core::change::Listeners;
use rayx_core::di::AppService;

use super::PlatformSnapshot;

/// A change of the platform's snapshot.
#[derive(Clone, Debug, PartialEq)]
pub struct PlatformChangedEvent {
    pub previous: PlatformSnapshot,
    pub current: PlatformSnapshot,
}

type PlatformListener = dyn Fn(&PlatformChangedEvent) + Send + Sync;

/// The device and the user's settings as the app's surface sees them ([`PlatformSnapshot`]),
/// kept current by the UI layer (`rayx_app` updates it from NSH's environment), with change
/// listeners. On NSH each surface has a provider, so the service describes its surface.
#[derive(Clone, AppService)]
pub struct PlatformInfoService {
    current: Arc<Mutex<PlatformSnapshot>>,
    listeners: Listeners<PlatformListener>,
}

impl PlatformInfoService {
    /// A service holding `snapshot` until the platform reports.
    pub fn new(snapshot: PlatformSnapshot) -> Self {
        Self {
            current: Arc::new(Mutex::new(snapshot)),
            listeners: Listeners::default(),
        }
    }

    /// The snapshot now.
    pub fn current(&self) -> PlatformSnapshot {
        self.current
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Replaces the snapshot; listeners hear it when it changed. Returns whether it changed.
    pub fn update(&self, snapshot: PlatformSnapshot) -> bool {
        let previous = {
            let mut current = self.current.lock().unwrap_or_else(PoisonError::into_inner);
            if *current == snapshot {
                return false;
            }
            std::mem::replace(&mut *current, snapshot.clone())
        };
        let event = PlatformChangedEvent {
            previous,
            current: snapshot,
        };
        self.listeners.notify(|listener| listener(&event));
        true
    }

    /// Changes the snapshot with `change`; listeners hear it when it changed.
    pub fn modify(&self, change: impl FnOnce(&mut PlatformSnapshot)) -> bool {
        let mut snapshot = self.current();
        change(&mut snapshot);
        self.update(snapshot)
    }

    /// Calls `listener` with every change until the subscription is dropped.
    pub fn subscribe(
        &self,
        listener: impl Fn(&PlatformChangedEvent) + Send + Sync + 'static,
    ) -> ChangeSubscription {
        self.listeners.add(Arc::new(listener))
    }
}

impl Default for PlatformInfoService {
    fn default() -> Self {
        Self::new(PlatformSnapshot::default())
    }
}
