//! RayX's app services, without NSH or GPUI (ported from RayX's `rayx_runtime::services`). Each
//! service is registered on a [`RayXAppBuilder`](rayx_core::RayXAppBuilder) through an
//! `AppBuilder` extension trait (`add_history()`, `add_navigation(..)`, …) and reads its options
//! from the configuration under `RayX:Services:<Name>` through [`rayx_core`]'s options model.
//! What a service needs from the platform (reading the app's files, registering fonts, the
//! device's state) is a trait here that the NSH layer, `rayx_app`, implements. It builds for
//! every target, wasm32 included.
//!
//! `docs/services.md` lists what each service brings over from RayX and what it leaves out.

// Unit tests panic on broken fixtures by design.
#![cfg_attr(test, allow(clippy::expect_used, clippy::unwrap_used))]

pub mod services;

/// The services' builder extensions: `use rayx_runtime::prelude::*;` to register them.
pub mod prelude {
    pub use crate::services::history::HistoryAppBuilderExt;
    pub use crate::services::navigation::NavigationAppBuilderExt;
    pub use crate::services::operations::OperationsAppBuilderExt;
    pub use crate::services::selection::SelectionAppBuilderExt;
    pub use crate::services::storage::StorageAppBuilderExt;
}
