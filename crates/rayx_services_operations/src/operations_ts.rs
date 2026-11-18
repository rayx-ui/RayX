//! The operation service for TSX.

use std::sync::Arc;

use nsh_core::JsonValue;
use nsh_core::prelude::*;
use rayx_app::ChangeSubscription;
use rayx_app::services::operations::{
    OperationError, OperationOrigin, OperationRecord, OperationService as Operations,
    OperationTargetId,
};
use rayx_app_nsh::ts::{Listeners, ServiceClass, json};
use serde_json::from_value;

use crate::target_ts::OperationTarget;

/// The app's operation service, for TSX: `services.getRequiredService(OperationService)`, then
/// `registerTarget(id, target)` (a target's handler is a Rust object, an `OperationTarget`),
/// `submitRecord(record, origin)`, transactions by id (`begin()`, `commit(id)`, `abort(id)`)
/// and `onEvent` to hear every committed or failed operation.
///
/// It wraps whichever `dyn OperationService` is registered (RayX's, or a TS implementation of
/// the contract `OperationServiceContract` registered under this class); its commands return
/// promises that resolve once the service answered and reject with its error.
#[class]
pub struct OperationService {
    service: Arc<dyn Operations>,
    listeners: Listeners,
}

impl ServiceClass for OperationService {
    type Service = dyn Operations;

    fn wrap(service: Arc<dyn Operations>) -> Self {
        Self {
            service,
            listeners: Listeners::default(),
        }
    }
}

/// The service `this` wraps.
fn service(this: &Instance<OperationService>) -> Arc<dyn Operations> {
    this.with(|this| Arc::clone(&this.service))
}

/// A service's error, for TS.
fn message(error: OperationError) -> String {
    error.to_string()
}

/// `value` as `T`, or why it is not one.
fn read<T: serde::de::DeserializeOwned>(value: JsonValue, what: &str) -> Result<T, String> {
    from_value(value).map_err(|error| format!("{what} does not convert: {error}"))
}

/// `id` as a target's id.
fn target_id(id: String) -> Result<OperationTargetId, String> {
    OperationTargetId::new(id).map_err(message)
}

#[class]
impl OperationService {
    /// The contract this class is the token of (TS types only: `null` at run time).
    #[getter]
    #[ts("(): OperationServiceContract | null")]
    pub fn contract(&self) -> Option<bool> {
        None
    }

    /// The service's instance number.
    #[getter]
    pub fn instance_id(&self) -> f64 {
        self.service.instance_id() as f64
    }

    /// The registered targets' ids, sorted.
    #[getter]
    pub fn targets(&self) -> Vec<String> {
        self.service
            .targets()
            .iter()
            .map(|id| id.as_str().to_owned())
            .collect()
    }

    /// Registers `target` as the target `targetId`; rejects when one is registered.
    pub async fn register_target(
        this: Instance<Self>,
        target_id: String,
        target: Instance<OperationTarget>,
    ) -> Result<(), String> {
        let handle = target.with(OperationTarget::handle);
        service(&this)
            .register_target(self::target_id(target_id)?, handle)
            .await
            .map_err(message)
    }

    /// Releases the target `targetId`: operations submitted to it fail from now on. Whether it
    /// was registered.
    pub async fn release_target(this: Instance<Self>, target_id: String) -> Result<bool, String> {
        service(&this)
            .release_target(self::target_id(target_id)?)
            .await
            .map_err(message)
    }

    /// Opens a transaction; its id, for `submitRecord`, `commit` and `abort`.
    pub async fn begin(this: Instance<Self>) -> Result<f64, String> {
        service(&this)
            .begin()
            .await
            .map(|id| id as f64)
            .map_err(message)
    }

    /// Commits the transaction `transactionId`: its operations' events are published.
    pub async fn commit(this: Instance<Self>, transaction_id: f64) -> Result<(), String> {
        service(&this)
            .commit(transaction_id as u64)
            .await
            .map_err(message)
    }

    /// Aborts the transaction `transactionId`: its operations are undone, newest first.
    pub async fn abort(this: Instance<Self>, transaction_id: f64) -> Result<(), String> {
        service(&this)
            .abort(transaction_id as u64)
            .await
            .map_err(message)
    }

    /// Applies `record` from `origin` through its target, in the transaction `transactionId`
    /// when one is given; the receipt.
    #[ts(
        "(record: OperationRecord, origin: OperationOrigin, transactionId?: number | null): OperationReceipt"
    )]
    pub async fn submit_record(
        this: Instance<Self>,
        record: JsonValue,
        origin: JsonValue,
        transaction_id: Option<f64>,
    ) -> Result<JsonValue, String> {
        let record: OperationRecord = read(record, "the record")?;
        let origin: OperationOrigin = read(origin, "the origin")?;
        service(&this)
            .submit_record(record, origin, transaction_id.map(|id| id as u64))
            .await
            .map(|receipt| json(&receipt))
            .map_err(message)
    }

    /// Publishes `record`, which the peer `peerId` applied (its target moved from
    /// `beforeRevision` to `afterRevision`), as committed; the receipt.
    #[ts(
        "(record: OperationRecord, peerId: string, beforeRevision: number, afterRevision: number, affectedIds: string[]): OperationReceipt"
    )]
    pub async fn publish_remote_record(
        this: Instance<Self>,
        record: JsonValue,
        peer_id: String,
        before_revision: f64,
        after_revision: f64,
        affected_ids: Vec<String>,
    ) -> Result<JsonValue, String> {
        let record: OperationRecord = read(record, "the record")?;
        service(&this)
            .publish_remote_record(
                record,
                peer_id,
                before_revision as u64,
                after_revision as u64,
                affected_ids,
            )
            .await
            .map(|receipt| json(&receipt))
            .map_err(message)
    }

    /// Calls `listener` with every operation event (`{ Committed: receipt }` or
    /// `{ Failed: failure }`), until the function it returns is called (or the reactive owner
    /// of the call goes), or this object is disposed.
    #[ts("(listener: (event: OperationEvent) => void): () => void")]
    pub fn on_event(
        &mut self,
        #[args(event)] listener: Callback<JsonValue>,
    ) -> Result<Stop, String> {
        let service = Arc::clone(&self.service);
        self.listeners.listen(listener, move |post| {
            Ok::<ChangeSubscription, String>(
                service.events().subscribe(move |event| post(json(event))),
            )
        })
    }
}

impl OperationService {
    /// The service, for Rust bridges that take an `Instance<OperationService>`.
    pub fn service(&self) -> &Arc<dyn Operations> {
        &self.service
    }
}
