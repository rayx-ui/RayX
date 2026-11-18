//! RayX's input map on NSH: RayX's implementation of
//! [`rayx_app::services::input_map::InputMapService`], which action a key sequence or a pointer
//! gesture runs in a context, in three layers: the framework's and the application's bindings
//! (from its [`InputMapOptions`], bound from configuration and set in code) and the user's,
//! which it keeps in the app's `dyn KeyValueStore` (required, never registered: `add_storage()`
//! from `rayx_services_storage`, or the app's own) and reads when the app starts.
//!
//! - Rust: `RayXAppBuilder::new().add_input_map(|options| { .. })` ([`InputMapAppBuilderExt`]),
//!   then `services.required_service::<dyn InputMapService>()`.
//! - TS: `new RayXAppBuilder().addInputMap((options) => { .. })`, with the class
//!   [`InputMapService`] over whichever `dyn InputMapService` is registered (the contract's
//!   token).
//! - The app's set: [`register`].

#![cfg_attr(test, allow(clippy::expect_used, clippy::unwrap_used))]

mod builder;
mod input_map_ts;
mod options;
mod options_ts;
mod service;

use nsh_core::element::{Registry, TsAlias};
use rayx_app::StateCell;
use rayx_app::services::input_map::*;
use rayx_app_nsh::ts::ContractRegistryExt as _;

pub use builder::{InputMapAppBuilderExt, InputMapBuilder};
pub use input_map_ts::InputMapService;
pub use options::{INPUT_MAP_OPTIONS_SECTION, InputMapOptions};
pub use options_ts::InputMapOptions as TsInputMapOptions;
pub use service::{INPUT_MAP_STORAGE_BUCKET, INPUT_MAP_USER_DOCUMENT_KEY};

rayx_app_nsh::contract_nsh!(rayx_app::services::input_map::InputMapService as InputMapContract);

/// The input map's state in TS.
pub const INPUT_MAP_STATE: TsAlias = TsAlias {
    name: "InputMapState",
    ts: "{ framework?: InputMapDocument; application?: InputMapDocument; user?: InputMapDocument; \
         revision?: number }",
    doc: "The framework's, the application's and the user's documents and the user's revision: the JSON of Rust's `InputMapState` (a field left out is its default).",
};

/// An input-map document in TS.
pub const INPUT_MAP_DOCUMENT: TsAlias = TsAlias {
    name: "InputMapDocument",
    ts: "{ schema_version: number; source: \"framework\" | \"application\" | \"user\"; \
         revision: number; entries?: InputMapEntry[] }",
    doc: "One layer's bindings: the JSON of Rust's `InputMapDocument` (`schema_version` is 1).",
};

/// An input-map entry in TS.
pub const INPUT_MAP_ENTRY: TsAlias = TsAlias {
    name: "InputMapEntry",
    ts: "{ context: string; binding: InputBinding; action: string; arguments?: unknown; \
         unbind?: boolean }",
    doc: "A binding of an action in a context (`\"\"` everywhere), or with `unbind` its removal: the JSON of Rust's `InputMapEntry`.",
};

/// An input binding in TS.
pub const INPUT_BINDING: TsAlias = TsAlias {
    name: "InputBinding",
    ts: "{ kind: \"keyboard\"; value: string } | { kind: \"pointer\"; value: { kind: \"click\" | \
         \"drag\" | \"wheel\"; [key: string]: unknown } }",
    doc: "A key sequence (`\"ctrl-k ctrl-c\"`) or a pointer gesture: the JSON of Rust's `InputBinding`.",
};

/// A resolved action in TS.
pub const RESOLVED_INPUT_ACTION: TsAlias = TsAlias {
    name: "ResolvedInputAction",
    ts: "{ action: string; arguments?: unknown; context: string; \
         source: \"framework\" | \"application\" | \"user\" }",
    doc: "What a binding runs, the context of the entry that matched and its layer: the JSON of Rust's `ResolvedInputAction`.",
};

/// Adds the input map's TS side to an app's set: its class [`InputMapService`], its options class
/// (`InputMapOptions` in TS), the contract a TS implementation follows (the TS interface
/// `InputMapServiceContract`, [`InputMapContract`] in Rust, whose token is the class) and
/// `builder.addInputMap(..)`.
pub fn register(registry: &mut Registry) {
    registry
        .ts_alias(INPUT_MAP_STATE)
        .ts_alias(INPUT_MAP_DOCUMENT)
        .ts_alias(INPUT_MAP_ENTRY)
        .ts_alias(INPUT_BINDING)
        .ts_alias(RESOLVED_INPUT_ACTION)
        .class::<TsInputMapOptions>()
        .contract_class::<InputMapContract, InputMapService>()
        .class_extension::<InputMapBuilder>();
}
