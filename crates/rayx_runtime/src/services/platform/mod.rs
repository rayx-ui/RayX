//! The platform: the device and the user's settings ([`PlatformSnapshot`]: OS, form factor,
//! input, viewport, safe area, appearance, motion, language and regional settings), held by the
//! [`PlatformInfoService`] and kept current by the UI layer, with change listeners.

mod app_ext;
mod service;
mod snapshot;

pub use app_ext::PlatformAppBuilderExt;
pub use service::{PlatformChangedEvent, PlatformInfoService};
pub use snapshot::{
    ColorScheme, FormFactor, InputCapabilities, Insets, KeyboardCapability, LocaleSnapshot,
    Orientation, PlatformFamily, PlatformSnapshot, PointerCapability, TextDirection,
    ViewportSnapshot,
};
