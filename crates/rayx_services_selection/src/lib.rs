//! RayX's selection service: the app-wide summary of the active selection area (which area
//! publishes, how much it has selected, never the selected content), published by RayX's
//! implementation of [`rayx_app::services::selection::SelectionService`] and cleared on every
//! route change of the app's `dyn NavigationService`, which it requires.
//!
//! - Rust: `RayXAppBuilder::new().add_navigation(|_| {}).add_selection()`
//!   ([`SelectionAppBuilderExt`]).
//! - TS: `new RayXAppBuilder().addNavigation().addSelection()`, with the class
//!   [`SelectionService`] over whichever `dyn SelectionService` is registered (the contract's
//!   token).
//! - The app's set: [`register`].

mod builder;
mod selection_ts;
mod service;

use nsh_core::element::{Registry, TsAlias};
use rayx_app::StateCell;
use rayx_app::services::selection::*;
use rayx_app_nsh::ts::ContractRegistryExt as _;

pub use builder::{SelectionAppBuilderExt, SelectionBuilder};
pub use selection_ts::SelectionService;

rayx_app_nsh::contract_nsh!(rayx_app::services::selection::SelectionService as SelectionContract);

/// A selection area's identity in TS.
pub const SELECTION_AREA_ID: TsAlias = TsAlias {
    name: "SelectionAreaId",
    ts: "number",
    doc: "A selection area's identity in the app's summary: the number of Rust's `SelectionAreaId`.",
};

/// A selection's status in TS.
pub const SELECTION_STATUS: TsAlias = TsAlias {
    name: "SelectionStatus",
    ts: "{ participant_count: number; selected_character_count: number; is_collapsed: boolean; is_selecting: boolean }",
    doc: "The content-free summary of the active area's selection: the JSON of Rust's `SelectionStatus`.",
};

/// The selection's state in TS.
pub const SELECTION_STATE: TsAlias = TsAlias {
    name: "SelectionState",
    ts: "{ active_area: SelectionAreaId | null; status: SelectionStatus; navigation_revision: number; active_area_revision: number; status_revision: number }",
    doc: "The active area, its status and the revisions that say what changed: the JSON of Rust's `SelectionState`.",
};

/// Adds the selection service's TS side to an app's set: its class [`SelectionService`], the
/// contract a TS implementation follows (the TS interface `SelectionServiceContract`,
/// [`SelectionContract`] in Rust) and `builder.addSelection()`.
pub fn register(registry: &mut Registry) {
    registry
        .ts_alias(SELECTION_AREA_ID)
        .ts_alias(SELECTION_STATUS)
        .ts_alias(SELECTION_STATE)
        .contract_class::<SelectionContract, SelectionService>()
        .class_extension::<SelectionBuilder>();
}
