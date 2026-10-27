//! Suspension: the app's lifecycle (started, shutting down), activation (focus) and visibility
//! (foreground, background), from the events the UI layer publishes, for work that pauses in
//! the background.

mod app_ext;
mod model;
mod service;

pub use app_ext::SuspensionAppBuilderExt;
pub use model::{
    SuspensionActivationState, SuspensionEvent, SuspensionEventKind, SuspensionEventReason,
    SuspensionLifecycleState, SuspensionSnapshot, SuspensionVisibilityState,
};
pub use service::{SuspensionChangedEvent, SuspensionService};
