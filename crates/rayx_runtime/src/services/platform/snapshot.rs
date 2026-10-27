use serde::{Deserialize, Serialize};

/// The operating system.
#[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub enum PlatformFamily {
    Windows,
    MacOS,
    Linux,
    Ios,
    Android,
    Web,
    Unknown(String),
}

impl PlatformFamily {
    /// The family this binary was built for (`Web` for wasm32).
    pub fn current() -> Self {
        if cfg!(target_family = "wasm") {
            Self::Web
        } else if cfg!(target_os = "windows") {
            Self::Windows
        } else if cfg!(target_os = "macos") {
            Self::MacOS
        } else if cfg!(target_os = "ios") {
            Self::Ios
        } else if cfg!(target_os = "android") {
            Self::Android
        } else if cfg!(target_os = "linux") {
            Self::Linux
        } else {
            Self::Unknown(std::env::consts::OS.to_owned())
        }
    }
}

/// The kind of device.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub enum FormFactor {
    Desktop,
    Tablet,
    Phone,
    Tv,
    Unknown,
}

impl FormFactor {
    /// A phone or a tablet.
    pub fn is_handheld(self) -> bool {
        matches!(self, Self::Phone | Self::Tablet)
    }
}

/// The light or dark appearance the app is shown in.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub enum ColorScheme {
    #[default]
    Light,
    Dark,
}

/// Portrait or landscape, from the viewport's sides.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub enum Orientation {
    Portrait,
    Landscape,
}

/// The direction text is written in.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub enum TextDirection {
    #[default]
    Ltr,
    Rtl,
}

/// The precision of the pointer the user has.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub enum PointerCapability {
    /// A mouse or a trackpad.
    Fine,
    /// A finger.
    Coarse,
}

/// The keyboard the user types on.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub enum KeyboardCapability {
    Hardware,
    /// On screen.
    Virtual,
}

/// How the user gives input, as the form factor suggests.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct InputCapabilities {
    pub pointer: PointerCapability,
    pub keyboard: KeyboardCapability,
    pub touch: bool,
}

impl InputCapabilities {
    /// What a device of `form_factor` has: desktops a fine pointer and a hardware keyboard,
    /// handhelds touch and an on-screen keyboard.
    pub fn for_form_factor(form_factor: FormFactor) -> Self {
        if form_factor.is_handheld() {
            Self {
                pointer: PointerCapability::Coarse,
                keyboard: KeyboardCapability::Virtual,
                touch: true,
            }
        } else {
            Self {
                pointer: PointerCapability::Fine,
                keyboard: KeyboardCapability::Hardware,
                touch: false,
            }
        }
    }
}

/// A size in points.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ViewportSnapshot {
    pub width: f32,
    pub height: f32,
}

/// Edges in points: the system UI's (notch, home indicator) over the viewport.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Insets {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

impl Insets {
    pub const ZERO: Self = Self {
        top: 0.0,
        right: 0.0,
        bottom: 0.0,
        left: 0.0,
    };
}

/// The user's language and regional settings.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocaleSnapshot {
    /// The locale formats use (`en-US`).
    pub locale: String,
    /// The preferred languages, most preferred first.
    pub languages: Vec<String>,
    /// The time zone's IANA name (`Europe/Berlin`).
    pub time_zone: String,
    /// `h12` or `h23`.
    pub hour_cycle: String,
    /// The first day of the week, as its English name in lowercase (`monday`).
    pub first_day_of_week: String,
    /// `metric`, `us` or `uk`.
    pub measurement_system: String,
    /// `celsius` or `fahrenheit`.
    pub temperature_unit: String,
    /// The direction of the first language's script.
    pub text_direction: TextDirection,
}

impl Default for LocaleSnapshot {
    fn default() -> Self {
        Self {
            locale: "en-US".to_owned(),
            languages: vec!["en-US".to_owned()],
            time_zone: "UTC".to_owned(),
            hour_cycle: "h12".to_owned(),
            first_day_of_week: "sunday".to_owned(),
            measurement_system: "us".to_owned(),
            temperature_unit: "fahrenheit".to_owned(),
            text_direction: TextDirection::Ltr,
        }
    }
}

/// What the app knows of the device and the user's settings at one moment.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlatformSnapshot {
    pub family: PlatformFamily,
    /// The OS version: major and minor (the API level and 0 on Android).
    pub os_version: (u16, u16),
    pub form_factor: FormFactor,
    /// Desktop behavior applies: a desktop OS, or a browser in the desktop idiom.
    pub desktop: bool,
    pub input: InputCapabilities,
    /// The app's surface (its window, or the screen).
    pub viewport: ViewportSnapshot,
    /// Device pixels per point.
    pub scale_factor: f32,
    pub safe_area: Insets,
    /// The on-screen keyboard's height over the surface, 0 when hidden.
    pub keyboard_height: f32,
    pub color_scheme: ColorScheme,
    /// The user's text size over the default.
    pub font_scale: f32,
    pub reduced_motion: bool,
    pub screen_reader: bool,
    pub locale: LocaleSnapshot,
}

impl PlatformSnapshot {
    /// A desktop of `viewport`'s size on the family this binary was built for, with default
    /// settings: what the service holds until the platform reports.
    pub fn desktop(viewport: ViewportSnapshot) -> Self {
        Self {
            family: PlatformFamily::current(),
            os_version: (0, 0),
            form_factor: FormFactor::Desktop,
            desktop: true,
            input: InputCapabilities::for_form_factor(FormFactor::Desktop),
            viewport,
            scale_factor: 1.0,
            safe_area: Insets::ZERO,
            keyboard_height: 0.0,
            color_scheme: ColorScheme::Light,
            font_scale: 1.0,
            reduced_motion: false,
            screen_reader: false,
            locale: LocaleSnapshot::default(),
        }
    }

    /// A handheld of `form_factor` with `viewport`'s size: touch input.
    pub fn handheld(viewport: ViewportSnapshot, form_factor: FormFactor) -> Self {
        Self {
            form_factor,
            desktop: false,
            input: InputCapabilities::for_form_factor(form_factor),
            ..Self::desktop(viewport)
        }
    }

    /// Portrait when the viewport is taller than wide.
    pub fn orientation(&self) -> Orientation {
        if self.viewport.height > self.viewport.width {
            Orientation::Portrait
        } else {
            Orientation::Landscape
        }
    }
}

impl Default for PlatformSnapshot {
    fn default() -> Self {
        Self::desktop(ViewportSnapshot {
            width: 1440.0,
            height: 960.0,
        })
    }
}
