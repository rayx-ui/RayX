//! A target's handler for TSX: the class handle a TS implementation of the operation service
//! calls.

use nsh_core::JsonValue;
use nsh_core::prelude::*;
use rayx_app::services::operations::{
    OperationOrigin, OperationRecord, OperationTargetHandle, OperationTargetResult,
};
use rayx_app_nsh::ts::json;
use serde_json::from_value;

/// A target's handler, a Rust object: what an operation service's `registerTarget` gets. A TS
/// implementation of the service (`OperationServiceContract`) receives it as a handle
/// (`{ handle }`), takes it with `OperationTarget.of(handle)` (once) and applies each record
/// submitted to the target with `apply(record, origin)`.
#[class]
pub struct OperationTarget {
    handle: OperationTargetHandle,
}

#[class]
impl OperationTarget {
    /// The handler a handle stands for, taken: a handle is taken once.
    #[ts("(handle: OperationTargetHandle): OperationTarget")]
    pub fn of(handle: JsonValue) -> Result<Self, String> {
        from_value::<OperationTargetHandle>(handle)
            .map(OperationTarget::new)
            .map_err(|error| error.to_string())
    }

    /// Applies `record` from `origin` (at once, synchronously); what it did: its status, the
    /// inverse record, the ids it changed and the target's revisions. Throws when the target
    /// refused or failed it, unchanged.
    #[ts("(record: OperationRecord, origin: OperationOrigin): OperationTargetResult")]
    pub fn apply(&self, record: JsonValue, origin: JsonValue) -> Result<JsonValue, String> {
        let record: OperationRecord =
            from_value(record).map_err(|error| format!("the record does not convert: {error}"))?;
        let origin: OperationOrigin =
            from_value(origin).map_err(|error| format!("the origin does not convert: {error}"))?;
        self.handle
            .apply(&record, &origin)
            .map(|result: OperationTargetResult| json(&result))
            .map_err(|error| error.to_string())
    }
}

impl OperationTarget {
    /// The class over `handle`, for Rust code handing a target's handler to TS.
    pub fn new(handle: OperationTargetHandle) -> Self {
        Self { handle }
    }

    /// The handler's handle.
    pub fn handle(&self) -> OperationTargetHandle {
        self.handle.clone()
    }
}
