//! RayX's operation service: RayX's implementation of
//! [`rayx_app::services::operations::OperationService`]. Targets register by id with their
//! handlers; each operation submitted to a target is applied atomically by its handler (one at
//! a time per target), answered with its inverse and published as committed (at once, or when
//! its transaction commits; an aborted transaction is undone, newest first) or failed: what the
//! app's history undoes and redoes, and what a collaboration layer sends to peers.
//!
//! - Rust: `RayXAppBuilder::new().add_operations()` ([`OperationsAppBuilderExt`]), then
//!   `services.required_service::<dyn OperationService>()`, `register_handler::<T, H>(..)`,
//!   `submit::<T>(..)` and `begin_transaction(mode)` (helpers of `dyn OperationService`).
//! - TS: `new RayXAppBuilder().addOperations()`, with the classes [`OperationService`] (the
//!   contract's token) and [`OperationTarget`] (a target's handler, the class handle a TS
//!   implementation of the service calls).
//! - The app's set: [`register`].
//!
//! The `diagnostics` feature keeps the latest committed operations and failure codes in the
//! published state (`diagnostics()` on `dyn OperationService`).

#![cfg_attr(test, allow(clippy::expect_used, clippy::unwrap_used))]

mod builder;
mod operations_ts;
mod service;
mod target_ts;

use nsh_core::element::{Registry, TsAlias};
use rayx_app::services::operations::*;
use rayx_app::{EventStream, StateCell};
use rayx_app_nsh::ts::ContractRegistryExt as _;

pub use builder::{OperationsAppBuilderExt, OperationsBuilder};
pub use operations_ts::OperationService;
pub use target_ts::OperationTarget;

rayx_app_nsh::contract_nsh!(
    rayx_app::services::operations::OperationService as OperationServiceContract
);

/// The operation service's state in TS.
pub const OPERATION_STATE: TsAlias = TsAlias {
    name: "OperationState",
    ts: "{ targets: string[]; recent_transactions: unknown[]; recent_failure_codes: string[] }",
    doc: "The registered targets (and diagnostics): the JSON of Rust's `OperationState`.",
};

/// An operation event in TS.
pub const OPERATION_EVENT: TsAlias = TsAlias {
    name: "OperationEvent",
    ts: "{ Committed: OperationReceipt } | { Failed: { transaction_id: number; target_id: string; kind: string; version: number; origin: OperationOrigin; error_code: string } }",
    doc: "An operation committed or failed: the JSON of Rust's `OperationEvent`.",
};

/// Optional metadata attached to an operation record, in TS.
pub const OPERATION_METADATA: TsAlias = TsAlias {
    name: "OperationMetadata",
    ts: "{ command_id?: string | null; label_key?: string | null; tags?: string[]; extra?: unknown }",
    doc: "Optional neutral metadata attached to an `OperationRecord`: the JSON of Rust's `OperationMetadata`.",
};

/// An operation record in TS.
pub const OPERATION_RECORD: TsAlias = TsAlias {
    name: "OperationRecord",
    ts: "{ target_id: string; kind: string; version: number; payload: number[]; payloadJson?: unknown; metadata?: OperationMetadata | null }",
    doc: "An operation as it crosses: its target, kind, version, the operation's JSON as bytes (with optional `payloadJson` convenience field), and optional metadata (the JSON of Rust's `OperationRecord`).",
};

/// An operation's origin in TS.
pub const OPERATION_ORIGIN: TsAlias = TsAlias {
    name: "OperationOrigin",
    ts: "\"LocalUser\" | \"System\" | \"Import\" | \"HistoryUndo\" | \"HistoryRedo\" | { Remote: string } | { Agent: string }",
    doc: "Where an operation comes from: history records the local user's (and, when enabled, an agent's).",
};

/// A receipt in TS.
pub const OPERATION_RECEIPT: TsAlias = TsAlias {
    name: "OperationReceipt",
    ts: "{ transaction_id: number; target_id: string; forward: OperationRecord; inverse: OperationRecord | null; origin: OperationOrigin; status: \"Applied\" | \"NoOp\" | \"Conflict\"; affected_ids: string[]; before_revision: number; after_revision: number }",
    doc: "An operation a target applied: the JSON of Rust's `OperationReceipt`.",
};

/// A target's handle in TS.
pub const OPERATION_TARGET_HANDLE: TsAlias = TsAlias {
    name: "OperationTargetHandle",
    ts: "{ handle: number }",
    doc: "A target's handler as it reaches a TS implementation of the operation service: `OperationTarget.of(handle)` takes it, once.",
};

/// What a target's handler did, in TS.
pub const OPERATION_TARGET_RESULT: TsAlias = TsAlias {
    name: "OperationTargetResult",
    ts: "{ status: \"Applied\" | \"NoOp\" | \"Conflict\"; inverse: OperationRecord | null; affected_ids: string[]; before_revision: number; after_revision: number }",
    doc: "What a target's handler did with a record: the JSON of Rust's `OperationTargetResult`.",
};

/// Adds the operation service's TS side to an app's set: its classes [`OperationService`] and
/// [`OperationTarget`], the contract a TS implementation follows (the TS interface
/// `OperationServiceContract`, [`OperationServiceContract`] in Rust) and
/// `builder.addOperations()`.
pub fn register(registry: &mut Registry) {
    registry
        .ts_alias(OPERATION_METADATA)
        .ts_alias(OPERATION_RECORD)
        .ts_alias(OPERATION_ORIGIN)
        .ts_alias(OPERATION_RECEIPT)
        .ts_alias(OPERATION_STATE)
        .ts_alias(OPERATION_EVENT)
        .ts_alias(OPERATION_TARGET_HANDLE)
        .ts_alias(OPERATION_TARGET_RESULT)
        .class::<OperationTarget>()
        .contract_class::<OperationServiceContract, OperationService>()
        .class_extension::<OperationsBuilder>();
}

/// RayX's operation service, made directly, for the crate's tests and an app's.
#[doc(hidden)]
pub mod testing {
    use std::sync::Arc;

    use rayx_app::services::operations::OperationService;

    /// A new RayX operation service, with no targets.
    pub fn operations() -> Arc<dyn OperationService> {
        Arc::new(crate::service::RayxOperations::new())
    }
}
