//! Suspension: the app's lifecycle (started, shutting down), activation (focus) and visibility
//! (foreground, background), and a desktop's window, from the events the UI layer publishes, for
//! work that pauses in the background.

mod app_ext;
mod model;
mod service;

pub use app_ext::SuspensionAppBuilderExt;
pub use model::{
    SuspensionActivationState, SuspensionEvent, SuspensionEventKind, SuspensionEventReason,
    SuspensionLifecycleState, SuspensionSnapshot, SuspensionVisibilityState, WindowActivity,
};
pub use service::{SuspensionChangedEvent, SuspensionService};
