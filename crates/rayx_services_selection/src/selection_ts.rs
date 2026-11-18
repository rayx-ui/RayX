//! The selection service for TSX.

use std::sync::Arc;

use futures::future::BoxFuture;
use nsh_core::JsonValue;
use nsh_core::prelude::*;
use rayx_app::services::selection::{
    SelectionAreaId, SelectionError, SelectionService as Selection, SelectionStatus,
};
use rayx_app_nsh::ts::{Listeners, ServiceClass, json};

/// The app's selection summary, for TSX: `services.getRequiredService(SelectionService)`, the
/// active selection area (a number) and its content-free status (`{ participant_count,
/// selected_character_count, is_collapsed, is_selecting }`), over whichever
/// `dyn SelectionService` is registered. Only the active area publishes, and every route change
/// clears it. Its commands (`activate`, `publishStatus`, `deactivate`, `clear`) return promises
/// that resolve once the service took them and reject with its error.
#[class]
pub struct SelectionService {
    service: Arc<dyn Selection>,
    listeners: Listeners,
}

impl ServiceClass for SelectionService {
    type Service = dyn Selection;

    fn wrap(service: Arc<dyn Selection>) -> Self {
        Self {
            service,
            listeners: Listeners::default(),
        }
    }
}

/// The service `this` wraps.
fn service(this: &Instance<SelectionService>) -> Arc<dyn Selection> {
    this.with(|this| Arc::clone(&this.service))
}

/// `status` as Rust's `SelectionStatus`.
fn status_of(status: JsonValue) -> Result<SelectionStatus, String> {
    serde_json::from_value(status).map_err(|error| format!("not a SelectionStatus: {error}"))
}

/// What a command answered, for TS: its error as the promise's rejection.
async fn answered(call: BoxFuture<'_, Result<(), SelectionError>>) -> Result<(), String> {
    call.await.map_err(|error| error.to_string())
}

#[class]
impl SelectionService {
    /// The contract this class is the token of (TS types only: `null` at run time).
    #[getter]
    #[ts("(): SelectionServiceContract | null")]
    pub fn contract(&self) -> Option<bool> {
        None
    }

    /// The selection now: `{ active_area, status, navigation_revision, active_area_revision,
    /// status_revision }`.
    #[ts("(): SelectionState")]
    pub fn state(&self) -> JsonValue {
        json(&*self.service.selection().get())
    }

    /// The area that publishes now, `null` when none is active.
    pub fn active_area(&self) -> Option<u64> {
        self.service.active_area().map(SelectionAreaId::get)
    }

    /// The active area's status.
    #[ts("(): SelectionStatus")]
    pub fn status(&self) -> JsonValue {
        json(&self.service.status())
    }

    /// Makes `activeArea` the area that publishes, with `status`; resolves once the service
    /// did.
    #[ts("(activeArea: number, status: SelectionStatus)")]
    pub async fn activate(
        this: Instance<Self>,
        active_area: u64,
        status: JsonValue,
    ) -> Result<(), String> {
        let status = status_of(status)?;
        answered(service(&this).activate(SelectionAreaId::from_raw(active_area), status)).await
    }

    /// Publishes `status` for `activeArea` when it is the active area (a stale area's status is
    /// dropped); resolves once the service took it.
    #[ts("(activeArea: number, status: SelectionStatus)")]
    pub async fn publish_status(
        this: Instance<Self>,
        active_area: u64,
        status: JsonValue,
    ) -> Result<(), String> {
        let status = status_of(status)?;
        answered(service(&this).publish_status(SelectionAreaId::from_raw(active_area), status))
            .await
    }

    /// Clears the selection when `activeArea` is the active area; resolves once the service
    /// took it.
    pub async fn deactivate(this: Instance<Self>, active_area: u64) -> Result<(), String> {
        answered(service(&this).deactivate(SelectionAreaId::from_raw(active_area))).await
    }

    /// Clears the active area and its status; resolves once the service did.
    pub async fn clear(this: Instance<Self>) -> Result<(), String> {
        answered(service(&this).clear()).await
    }

    /// Calls `listener` with the selection (as `state()` gives it) after every change, until
    /// the function it returns is called (or the reactive owner of the call goes), or this
    /// object is disposed.
    #[ts("(listener: (state: SelectionState) => void): () => void")]
    pub fn on_change(
        &mut self,
        #[args(state)] listener: Callback<JsonValue>,
    ) -> Result<Stop, String> {
        let service = Arc::clone(&self.service);
        self.listeners.listen(listener, move |post| {
            Ok::<_, String>(
                service
                    .selection()
                    .subscribe(move |state| post(json(&**state))),
            )
        })
    }
}

impl SelectionService {
    /// The service, for Rust bridges that take an `Instance<SelectionService>`.
    pub fn service(&self) -> &Arc<dyn Selection> {
        &self.service
    }
}
