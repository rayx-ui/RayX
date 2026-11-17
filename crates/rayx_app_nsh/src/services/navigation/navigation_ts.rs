//! The navigation service for TSX.

use std::sync::Arc;

use nsh_core::JsonValue;
use nsh_core::prelude::*;
use rayx_runtime::services::navigation::NavigationService as Navigation;

use crate::di::provider_ts::ServiceClass;
use crate::services::json;
use crate::services::listeners_ts::Listeners;
use crate::ts::NavigationRegion;

/// The app's navigation, for TSX: `services.getRequiredService(NavigationService)`, then
/// `navigate("/projects/12")`, `back()` and `current()`; `region(name)` is a region of it, and
/// `onChange` hears every route change, with `{ region, from, to }`.
#[class]
pub struct NavigationService {
    service: Arc<Navigation>,
    listeners: Listeners,
}

impl ServiceClass for NavigationService {
    type Service = Navigation;

    fn wrap(service: Arc<Navigation>) -> Self {
        Self {
            service,
            listeners: Listeners::default(),
        }
    }
}

#[class]
impl NavigationService {
    /// Navigates the root region to `uri`; the result has the region, the direction, the status
    /// (`Success`, `FailedNotFound`, …) and the route.
    pub fn navigate(&self, uri: String) -> Result<JsonValue, String> {
        self.service
            .navigate_to(uri)
            .map(|result| json(&result))
            .map_err(|error| error.to_string())
    }

    /// Goes back in the root region's journal.
    pub fn back(&self) -> Result<JsonValue, String> {
        self.service
            .back()
            .map(|result| json(&result))
            .map_err(|error| error.to_string())
    }

    /// Goes forward in the root region's journal.
    pub fn forward(&self) -> Result<JsonValue, String> {
        self.service
            .forward()
            .map(|result| json(&result))
            .map_err(|error| error.to_string())
    }

    /// The root region's current route and its status, `null` before the first navigation.
    pub fn current(&self) -> Result<JsonValue, String> {
        self.service
            .current()
            .map(|current| current.map_or(JsonValue::Null, |data| json(&data)))
            .map_err(|error| error.to_string())
    }

    /// The navigation region `name`, made when missing: its own journal, for a `NavStack`.
    pub fn region(&self, name: String) -> Result<Instance<NavigationRegion>, String> {
        NavigationRegion::of(Arc::clone(&self.service), name).map(Instance::new)
    }

    /// Calls `listener` with `{ region, from, to }` after every route change, until the
    /// function it returns is called (or the reactive owner of the call goes), or this object
    /// is disposed.
    pub fn on_change(
        &mut self,
        #[args(event)] listener: Callback<JsonValue>,
    ) -> Result<Stop, String> {
        let service = Arc::clone(&self.service);
        self.listeners.listen(listener, move |post| {
            service
                .subscribe_route_changed(move |event| {
                    post(serde_json::json!({
                        "region": event.region,
                        "from": event.from.as_ref().map(json),
                        "to": json(&event.to),
                    }));
                })
                .map_err(|error| error.to_string())
        })
    }
}
