//! The navigation service for TSX.

use std::sync::Arc;

use futures::future::BoxFuture;
use nsh_core::JsonValue;
use nsh_core::prelude::*;
use rayx_app::services::navigation::{
    NavigationError, NavigationEvent, NavigationService as Navigation,
};
use rayx_app_nsh::ts::{Listeners, ServiceClass, json};
use serde::Serialize;

use crate::region_ts::NavigationRegion;

/// A command's answer for TS: its JSON, or the service's error as the promise's rejection.
pub(crate) async fn answer<T: Serialize>(
    command: BoxFuture<'_, Result<T, NavigationError>>,
) -> Result<JsonValue, String> {
    command
        .await
        .map(|value| json(&value))
        .map_err(|error| error.to_string())
}

/// The app's navigation, for TSX: `services.getRequiredService(NavigationService)`, then
/// `await navigate("/projects/12")`, `await back()` and `current()`; `region(name)` is a region
/// of it, and `onChange` hears every route change, with `{ region, from, to }`. Its commands are
/// promises, over RayX's implementation and a TS one alike; its reads are synchronous.
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

/// The service `this` wraps.
fn service(this: &Instance<NavigationService>) -> Arc<dyn Navigation> {
    this.with(|this| Arc::clone(&this.service))
}

#[class]
impl NavigationService {
    /// The contract this class is the token of (TS types only: `null` at run time).
    #[getter]
    #[ts("(): NavigationServiceContract | null")]
    pub fn contract(&self) -> Option<bool> {
        None
    }

    /// Navigates the root region to `uri`; resolves with the region, the direction, the status
    /// (`Success`, `FailedNotFound`, …) and the route, or rejects with the service's error.
    #[ts("(uri: string): NavigationResult")]
    pub async fn navigate(this: Instance<Self>, uri: String) -> Result<JsonValue, String> {
        let service = service(&this);
        answer(service.navigate_to(uri)).await
    }

    /// Goes back in the root region's journal.
    #[ts("(): NavigationResult")]
    pub async fn back(this: Instance<Self>) -> Result<JsonValue, String> {
        let service = service(&this);
        answer(service.back(service.root_region_name())).await
    }

    /// Goes forward in the root region's journal.
    #[ts("(): NavigationResult")]
    pub async fn forward(this: Instance<Self>) -> Result<JsonValue, String> {
        let service = service(&this);
        answer(service.forward(service.root_region_name())).await
    }

    /// Navigates the root region to its current route again.
    #[ts("(): NavigationResult")]
    pub async fn refresh(this: Instance<Self>) -> Result<JsonValue, String> {
        let service = service(&this);
        answer(service.refresh(service.root_region_name())).await
    }

    /// The root region's current route and its status, `null` before the first navigation.
    pub fn current(&self) -> JsonValue {
        self.service
            .current()
            .map_or(JsonValue::Null, |data| json(&data))
    }

    /// The navigation region `name`, made when missing: its own journal, for a `NavStack`. Over
    /// a TS implementation, which makes it later, the region's first command waits for it.
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
