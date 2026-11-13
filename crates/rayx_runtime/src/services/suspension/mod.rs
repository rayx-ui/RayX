//! Suspension: the app's lifecycle (started, shutting down), activation (focus) and visibility
//! (foreground, background), a desktop's window and the system's wakes from sleep, from the
//! events the UI layer publishes, for work that pauses in the background or must be saved as the
//! app quits; and quitting the app, asked first while the app wants to be.

mod app_ext;
mod model;
mod service;

pub use app_ext::SuspensionAppBuilderExt;
pub use model::{
    SuspensionActivationState, SuspensionEvent, SuspensionEventKind, SuspensionEventReason,
    SuspensionLifecycleState, SuspensionSnapshot, SuspensionVisibilityState, WindowActivity,
};
pub use service::{NoopSuspensionHost, SuspensionChangedEvent, SuspensionHost, SuspensionService};
