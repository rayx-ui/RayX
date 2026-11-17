//! RayX's own suspension service without an engine, for tests: the service `add_suspension()`
//! registers, fed the events NSH's app state, window and wakes would give it.

use std::sync::Arc;

use rayx_app::services::suspension::{SuspensionEvent, SuspensionService};

use crate::service::NshSuspension;

/// RayX's suspension service, and what feeds it the events it hears from NSH.
#[derive(Clone)]
pub struct FedSuspension(Arc<NshSuspension>);

impl FedSuspension {
    /// A new service, at the default snapshot and following nothing.
    pub fn new() -> Self {
        Self(NshSuspension::new())
    }

    /// The service, as an app has it.
    pub fn service(&self) -> Arc<dyn SuspensionService> {
        Arc::clone(&self.0) as Arc<dyn SuspensionService>
    }

    /// Gives the service `event` as it gives itself what NSH tells it: its state is published
    /// when it changed, and every event subscriber hears it.
    pub fn emit(&self, event: SuspensionEvent) {
        self.0.publish(event);
    }
}

impl Default for FedSuspension {
    fn default() -> Self {
        Self::new()
    }
}
