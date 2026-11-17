//! RayX's platform service on NSH: the device and the user's settings
//! ([`PlatformSnapshot`](rayx_app::services::platform::PlatformSnapshot)), published by RayX's
//! implementation of [`rayx_app::services::platform::PlatformInfoService`], which follows NSH's
//! environment from the app's start on, on every surface, in Rust and TSX apps alike.
//!
//! - Rust: `RayXAppBuilder::new().add_platform()` ([`PlatformAppBuilderExt`]).
//! - TS: `new RayXAppBuilder().addPlatform()` (a class extension of the host's
//!   `RayXAppBuilder`), with the class [`PlatformInfoService`] over whichever
//!   `dyn PlatformInfoService` is registered, Rust or TS (the contract's token).
//! - The app's set: [`register`] adds the class, its contract and the builder method
//!   (`#[rayx::native_module]` does when the app depends on `rayx` with this crate's feature).

mod builder;
mod platform_ts;
mod service;
mod snapshot;

use nsh_core::element::{Registry, TsAlias};
use rayx_app::StateCell;
use rayx_app::services::platform::*;
use rayx_app_nsh::ts::ContractRegistryExt as _;

pub use builder::{PlatformAppBuilderExt, PlatformBuilder};
pub use platform_ts::PlatformInfoService;
pub use snapshot::platform_snapshot;

rayx_app_nsh::contract_nsh!(
    rayx_app::services::platform::PlatformInfoService as PlatformInfoContract
);

/// The platform's snapshot in TS: the JSON of Rust's `PlatformSnapshot`.
pub const PLATFORM_SNAPSHOT: TsAlias = TsAlias {
    name: "PlatformSnapshot",
    ts: "Record<string, unknown>",
    doc: "The device and the user's settings: the JSON of Rust's `PlatformSnapshot`.",
};

/// Adds the platform service's TS side to an app's set: its class [`PlatformInfoService`], the
/// contract a TS implementation follows (the TS interface `PlatformInfoServiceContract`,
/// [`PlatformInfoContract`] in Rust, whose token is the class) and `builder.addPlatform()`.
pub fn register(registry: &mut Registry) {
    registry
        .ts_alias(PLATFORM_SNAPSHOT)
        .contract_class::<PlatformInfoContract, PlatformInfoService>()
        .class_extension::<PlatformBuilder>();
}
