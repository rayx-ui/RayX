//! The core's services module ([`rayx_app::services`]), re-exported, with the NSH side of RayX's
//! services beside it, a folder each: what NSH implements for a service, its TS class and its TS
//! options. Here too: the one helper the TS classes share.
//!
//! The services' TS classes ([`I18nService`](crate::ts::I18nService) and the others in
//! [`ts`](crate::ts)): each class carries its Rust service's name and is its own token
//! (`services.getRequiredService(I18nService)`), and each `onChange` listener hears the
//! service's changes until the function it returned is called (or the reactive owner of the call
//! goes), or the object is disposed. Values cross as plain objects (the services' snapshots,
//! serialized). The services notify on the engine's thread, where TSX listeners run.

#[doc(no_inline)]
pub use rayx_app::services::*;

pub(crate) mod application_configuration;
pub(crate) mod i18n;
pub(crate) mod input_map;
pub(crate) mod listeners_ts;

use nsh_core::JsonValue;
use serde::Serialize;

/// `value` as a plain object for TSX.
pub fn json(value: &impl Serialize) -> JsonValue {
    serde_json::to_value(value).unwrap_or(JsonValue::Null)
}
