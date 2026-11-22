//! The core's dependency injection ([`rayx_app::di`]), re-exported, with NSH's screen scopes
//! and TS's provider, container, startup services and contracts beside it.

#[doc(no_inline)]
pub use rayx_app::di::*;

pub(crate) mod container_ts;
pub(crate) mod contract_ts;
pub(crate) mod provider_ts;
pub(crate) mod screen;
pub(crate) mod screen_ts;
pub(crate) mod startup_ts;
pub(crate) mod view_model_ts;
