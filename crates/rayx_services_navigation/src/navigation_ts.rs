//! The navigation service for TSX.

use std::sync::Arc;

use futures::FutureExt as _;
use futures::future::BoxFuture;
use nsh_core::JsonValue;
use nsh_core::prelude::*;
use rayx_app::services::navigation::{
    NavigationError, NavigationEvent, NavigationService as Navigation,
};
use rayx_app_nsh::ts::{Listeners, ServiceClass, json};
use serde::Serialize;

use crate::region_ts::NavigationRegion;

/// A command's answer for TS, which calls synchronously: RayX's implementation answers at
/// once; a TS implementation's call starts, and TS awaits its contract's member instead.
pub(crate) fn now<T: Serialize>(
    command: BoxFuture<'_, Result<T, NavigationError>>,
) -> Result<JsonValue, String> {
    match command.now_or_never() {
        Some(answer) => answer
            .map(|value| json(&value))
            .map_err(|error| error.to_string()),
        None => Err(
            "the navigation service answers later: await its contract's member instead".to_owned(),
        ),
    }
}

/// The app's navigation, for TSX: `services.getRequiredService(NavigationService)`, then
/// `navigate("/projects/12")`, `back()` and `current()`; `region(name)` is a region of it, and
/// `onChange` hears every route change, with `{ region, from, to }`.
#[class]
pub struct NavigationService {
    service: Arc<dyn Navigation>,
    listeners: Listeners,
}

impl ServiceClass for NavigationService {
    type Service = dyn Navigation;

    fn wrap(service: Arc<dyn Navigation>) -> Self {
        Self {
            service,
            listeners: Listeners::default(),
        }
    }
}

#[class]
impl NavigationService {
    /// The contract this class is the token of (TS types only: `null` at run time).
    #[getter]
    #[ts("(): NavigationServiceContract | null")]
    pub fn contract(&self) -> Option<bool> {
        None
    }

    /// Navigates the root region to `uri`; the result has the region, the direction, the status
    /// (`Success`, `FailedNotFound`, …) and the route.
    pub fn navigate(&self, uri: String) -> Result<JsonValue, String> {
        now(self.service.navigate_to(uri))
    }

    /// Goes back in the root region's journal.
    pub fn back(&self) -> Result<JsonValue, String> {
        now(self.service.back(self.service.root_region_name()))
    }

    /// Goes forward in the root region's journal.
    pub fn forward(&self) -> Result<JsonValue, String> {
        now(self.service.forward(self.service.root_region_name()))
    }

    /// The root region's current route and its status, `null` before the first navigation.
    pub fn current(&self) -> JsonValue {
        self.service
            .current()
            .map_or(JsonValue::Null, |data| json(&data))
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
            Ok::<_, String>(service.events().subscribe(move |event| {
                if let NavigationEvent::RouteChanged(event) = event {
                    post(serde_json::json!({
                        "region": event.region,
                        "from": event.from.as_ref().map(json),
                        "to": json(&event.to),
                    }));
                }
            }))
        })
    }
}
