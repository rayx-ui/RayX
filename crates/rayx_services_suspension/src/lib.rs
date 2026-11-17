//! RayX's suspension service on NSH: the app's lifecycle, activation, visibility, a desktop's
//! window and the system's wakes, published by RayX's implementation of
//! [`rayx_app::services::suspension::SuspensionService`], which follows NSH's app state from the
//! app's start on, in Rust and TSX apps alike, and quits with NSH's `quit` (from any thread).
//!
//! - Rust: `RayXAppBuilder::new().add_suspension()` ([`SuspensionAppBuilderExt`]).
//! - TS: `new RayXAppBuilder().addSuspension()`, with the class [`SuspensionService`] over
//!   whichever `dyn SuspensionService` is registered (the contract's token).
//! - The app's set: [`register`].

mod builder;
mod service;
mod suspension_ts;

use nsh_core::element::{Registry, TsAlias};
use rayx_app::services::suspension::*;
use rayx_app::{EventStream, StateCell};
use rayx_app_nsh::ts::ContractRegistryExt as _;

pub use builder::{SuspensionAppBuilderExt, SuspensionBuilder};
pub use suspension_ts::SuspensionService;

rayx_app_nsh::contract_nsh!(
    rayx_app::services::suspension::SuspensionService as SuspensionContract
);

/// The suspension service's snapshot in TS.
pub const SUSPENSION_SNAPSHOT: TsAlias = TsAlias {
    name: "SuspensionSnapshot",
    ts: "Record<string, unknown>",
    doc: "The app's lifecycle, activation, visibility, window and wakes: the JSON of Rust's `SuspensionSnapshot`.",
};

/// A suspension event in TS.
pub const SUSPENSION_EVENT: TsAlias = TsAlias {
    name: "SuspensionEvent",
    ts: "{ kind: string; reason: string; [key: string]: unknown }",
    doc: "A lifecycle event: the JSON of Rust's `SuspensionEvent` (`kind` as Rust names it).",
};

/// Adds the suspension service's TS side to an app's set: its class [`SuspensionService`], the
/// contract a TS implementation follows (the TS interface `SuspensionServiceContract`,
/// [`SuspensionContract`] in Rust) and
/// `builder.addSuspension()`.
pub fn register(registry: &mut Registry) {
    registry
        .ts_alias(SUSPENSION_SNAPSHOT)
        .ts_alias(SUSPENSION_EVENT)
        .contract_class::<SuspensionContract, SuspensionService>()
        .class_extension::<SuspensionBuilder>();
}
