//! The platform: the device and the user's settings ([`PlatformSnapshot`]: OS, form factor,
//! input, viewport, safe area, appearance, motion, language and regional settings, and a
//! desktop's displays and window), held by the
//! [`PlatformInfoService`] and kept current by the UI layer, with change listeners.

mod app_ext;
mod service;
mod snapshot;

pub use app_ext::PlatformAppBuilderExt;
pub use service::{PlatformChangedEvent, PlatformInfoService};
pub use snapshot::{
    ColorScheme, DisplaySnapshot, FormFactor, InputCapabilities, Insets, KeyboardCapability,
    LocaleSnapshot, Orientation, PlatformFamily, PlatformRect, PlatformSnapshot, PointerCapability,
    TextDirection, ViewportSnapshot, WindowMode, WindowSnapshot,
};
