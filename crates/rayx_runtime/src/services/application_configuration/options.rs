use serde::{Deserialize, Serialize};

/// The configuration section [`ApplicationConfiguration`] binds from.
pub const APPLICATION_CONFIGURATION_PATH: &str = "RayX:Application";

/// The app's own settings: its name, and the behaviour its UI components share (motion,
/// programmatic scrolling, tooltip and hover-card timing, notifications, layout direction).
/// Keys a section has that are not here are ignored, as in .NET.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ApplicationConfiguration {
    /// The app's name (startup only).
    pub name: String,
    /// What the app is (startup only).
    pub description: String,
    pub motion: MotionBehaviorOptions,
    pub scrolling: ScrollingBehaviorOptions,
    pub tooltips: TooltipBehaviorOptions,
    pub hover_cards: HoverCardBehaviorOptions,
    pub notifications: NotificationBehaviorOptions,
    pub layout: LayoutBehaviorOptions,
}

impl Default for ApplicationConfiguration {
    fn default() -> Self {
        Self {
            name: "RayX".to_owned(),
            description: String::new(),
            motion: MotionBehaviorOptions::default(),
            scrolling: ScrollingBehaviorOptions::default(),
            tooltips: TooltipBehaviorOptions::default(),
            hover_cards: HoverCardBehaviorOptions::default(),
            notifications: NotificationBehaviorOptions::default(),
            layout: LayoutBehaviorOptions::default(),
        }
    }
}

impl ApplicationConfiguration {
    /// Why the settings are invalid, if they are.
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.name.trim().is_empty() {
            return Err("name must not be empty");
        }
        if self.notifications.auto_hide_ms == 0 {
            return Err("notifications.autoHideMs must be greater than zero");
        }
        if self.notifications.max_items == 0 {
            return Err("notifications.maxItems must be greater than zero");
        }
        if self.scrolling.smooth_duration_ms == 0 {
            return Err("scrolling.smoothDurationMs must be greater than zero");
        }
        Ok(())
    }
}

/// Whether the app reduces motion beyond what the system asks.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub enum MotionPolicy {
    /// As the system's setting says.
    #[default]
    System,
    /// Always reduced.
    Reduced,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MotionBehaviorOptions {
    pub policy: MotionPolicy,
}

/// How the app scrolls when code asks it to (`scroll_to`).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub enum ProgrammaticScrollBehavior {
    #[default]
    Instant,
    Smooth,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ScrollingBehaviorOptions {
    pub programmatic: ProgrammaticScrollBehavior,
    /// How long a smooth programmatic scroll takes.
    pub smooth_duration_ms: u64,
}

impl Default for ScrollingBehaviorOptions {
    fn default() -> Self {
        Self {
            programmatic: ProgrammaticScrollBehavior::Instant,
            smooth_duration_ms: 200,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TooltipBehaviorOptions {
    /// How long the pointer rests before a tooltip shows.
    pub show_delay_ms: u64,
    /// How long after one tooltip closes the next shows at once.
    pub grace_period_ms: u64,
}

impl Default for TooltipBehaviorOptions {
    fn default() -> Self {
        Self {
            show_delay_ms: 500,
            grace_period_ms: 300,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HoverCardBehaviorOptions {
    pub open_delay_ms: u64,
    pub close_delay_ms: u64,
}

impl Default for HoverCardBehaviorOptions {
    fn default() -> Self {
        Self {
            open_delay_ms: 600,
            close_delay_ms: 300,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct NotificationBehaviorOptions {
    /// How long a notification stays.
    pub auto_hide_ms: u64,
    /// The corner or edge the notification stack grows from.
    pub placement: NotificationPlacement,
    /// The stack's distance from the surface's edges, in points.
    pub margins: NotificationMargins,
    /// How many notifications show at once.
    pub max_items: usize,
}

impl Default for NotificationBehaviorOptions {
    fn default() -> Self {
        Self {
            auto_hide_ms: 5_000,
            placement: NotificationPlacement::TopRight,
            margins: NotificationMargins::default(),
            max_items: 10,
        }
    }
}

/// Where the notification stack sits on the surface.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationPlacement {
    TopLeft,
    TopCenter,
    #[default]
    TopRight,
    BottomLeft,
    BottomCenter,
    BottomRight,
}

/// The notification stack's distances from the surface's edges, in points.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(default)]
pub struct NotificationMargins {
    pub top: u32,
    pub right: u32,
    pub bottom: u32,
    pub left: u32,
}

impl Default for NotificationMargins {
    fn default() -> Self {
        Self {
            top: 16,
            right: 16,
            bottom: 16,
            left: 16,
        }
    }
}

/// The reading direction of the app's layout.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LayoutDirection {
    #[default]
    Ltr,
    Rtl,
}

impl LayoutDirection {
    pub fn is_rtl(self) -> bool {
        self == Self::Rtl
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LayoutBehaviorOptions {
    pub direction: LayoutDirection,
}
