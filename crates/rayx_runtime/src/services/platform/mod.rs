//! The platform: the device and the user's settings ([`PlatformSnapshot`]: OS and platform,
//! form factor, input (a TV's remote included), viewport, safe area, keyboard, appearance,
//! motion, accessibility, language and regional settings, a terminal's grid, what the platform
//! can do, and a desktop's displays and window), held by the
//! [`PlatformInfoService`] and kept current by the UI layer, with change listeners.

mod app_ext;
mod service;
mod snapshot;

pub use app_ext::PlatformAppBuilderExt;
pub use service::{PlatformChangedEvent, PlatformInfoService};
pub use snapshot::{
    ColorScheme, DisplaySnapshot, FormFactor, InputCapabilities, Insets, KeyboardCapability,
    LocaleSnapshot, Orientation, PlatformFamily, PlatformFeatures, PlatformRect, PlatformSnapshot,
    PointerCapability, TextDirection, ViewportSnapshot, WindowMode, WindowSnapshot,
};
