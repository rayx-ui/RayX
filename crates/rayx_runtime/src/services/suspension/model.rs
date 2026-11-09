use serde::{Deserialize, Serialize};

/// Whether the app is in focus.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub enum SuspensionActivationState {
    #[default]
    Unknown,
    Active,
    Inactive,
}

/// Where the app is in its life.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub enum SuspensionLifecycleState {
    #[default]
    NotStarted,
    Running,
    Destroying,
    Destroyed,
}

/// Whether the app is seen.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub enum SuspensionVisibilityState {
    #[default]
    Unknown,
    Foreground,
    Background,
}

/// The app's window on a desktop: whether it has the focus, is minimized and is on the screen.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct WindowActivity {
    /// It has the keyboard focus.
    pub focused: bool,
    pub minimized: bool,
    /// It is on the screen: shown and not minimized.
    pub visible: bool,
}

/// What happened.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub enum SuspensionEventKind {
    /// The app started: running, in the foreground.
    Started,
    /// The app got the focus: active, in the foreground.
    Activated,
    /// The app lost the focus but is still seen (a system dialog over it, another window
    /// focused).
    Deactivated,
    /// The app went to the background: inactive, unseen.
    Backgrounded,
    /// The app came back to the foreground (the focus follows with `Activated`).
    Foregrounded,
    /// The app is shutting down.
    Destroying,
    /// The app shut down.
    Destroyed,
    /// The app's window on a desktop got or lost the focus, was minimized or restored, or was
    /// shown or hidden: the event's `window` is its state now.
    WindowChanged,
}

/// Why it happened.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub enum SuspensionEventReason {
    /// The host started the app.
    HostStarted,
    /// The host is stopping the app.
    HostStopping,
    /// The host stopped the app.
    HostStopped,
    /// The platform reported a new app state (NSH's `lifecycle()`).
    AppStateChanged,
    /// The platform reported a new state of the app's window (NSH's `window_state()`).
    WindowStateChanged,
    /// The app published it itself.
    Application,
}

/// The app's lifecycle, activation and visibility, and what changed them last.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct SuspensionSnapshot {
    pub lifecycle: SuspensionLifecycleState,
    pub activation: SuspensionActivationState,
    pub visibility: SuspensionVisibilityState,
    /// When the last event happened, in milliseconds since the Unix epoch, when its publisher
    /// knew.
    pub updated_at_unix_millis: Option<u64>,
    pub last_reason: Option<SuspensionEventReason>,
    /// The app's window on a desktop (`None` elsewhere, and until the platform reports it).
    pub window: Option<WindowActivity>,
}

impl SuspensionSnapshot {
    pub fn is_running(&self) -> bool {
        self.lifecycle == SuspensionLifecycleState::Running
    }

    pub fn is_active(&self) -> bool {
        self.activation == SuspensionActivationState::Active
    }

    pub fn is_foreground(&self) -> bool {
        self.visibility == SuspensionVisibilityState::Foreground
    }

    pub fn is_backgrounded(&self) -> bool {
        self.visibility == SuspensionVisibilityState::Background
    }

    /// Whether `other` has the same lifecycle, activation, visibility and window.
    pub fn same_state(&self, other: &Self) -> bool {
        (
            self.lifecycle,
            self.activation,
            self.visibility,
            self.window,
        ) == (
            other.lifecycle,
            other.activation,
            other.visibility,
            other.window,
        )
    }

    pub(super) fn apply(mut self, event: &SuspensionEvent) -> Self {
        use SuspensionActivationState as Activation;
        use SuspensionLifecycleState as Lifecycle;
        use SuspensionVisibilityState as Visibility;
        match event.kind {
            SuspensionEventKind::Started => {
                self.lifecycle = Lifecycle::Running;
                self.visibility = Visibility::Foreground;
            }
            SuspensionEventKind::Activated => {
                self.activation = Activation::Active;
                self.visibility = Visibility::Foreground;
            }
            SuspensionEventKind::Deactivated => self.activation = Activation::Inactive,
            SuspensionEventKind::Backgrounded => {
                self.activation = Activation::Inactive;
                self.visibility = Visibility::Background;
            }
            SuspensionEventKind::Foregrounded => self.visibility = Visibility::Foreground,
            SuspensionEventKind::Destroying => {
                self.lifecycle = Lifecycle::Destroying;
                self.activation = Activation::Inactive;
            }
            SuspensionEventKind::Destroyed => {
                self.lifecycle = Lifecycle::Destroyed;
                self.activation = Activation::Inactive;
                self.visibility = Visibility::Background;
            }
            SuspensionEventKind::WindowChanged => self.window = event.window,
        }
        self.updated_at_unix_millis = event.timestamp_unix_millis.or(self.updated_at_unix_millis);
        self.last_reason = Some(event.reason);
        self
    }
}

/// An app lifecycle event.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SuspensionEvent {
    pub kind: SuspensionEventKind,
    pub reason: SuspensionEventReason,
    /// When it happened, in milliseconds since the Unix epoch.
    pub timestamp_unix_millis: Option<u64>,
    /// The app's window, for [`SuspensionEventKind::WindowChanged`].
    pub window: Option<WindowActivity>,
}

impl SuspensionEvent {
    /// An event of `kind` for `reason`, stamped with the system clock where the target has one
    /// (not in WebAssembly, where the core reads no clock: stamp it with [`Self::at`]).
    pub fn new(kind: SuspensionEventKind, reason: SuspensionEventReason) -> Self {
        Self {
            kind,
            reason,
            timestamp_unix_millis: now_unix_millis(),
            window: None,
        }
    }

    /// The app's window is now `window` ([`SuspensionEventKind::WindowChanged`]).
    pub fn window_changed(window: WindowActivity, reason: SuspensionEventReason) -> Self {
        Self {
            window: Some(window),
            ..Self::new(SuspensionEventKind::WindowChanged, reason)
        }
    }

    /// The same event at `timestamp_unix_millis`.
    pub fn at(mut self, timestamp_unix_millis: u64) -> Self {
        self.timestamp_unix_millis = Some(timestamp_unix_millis);
        self
    }
}

#[cfg(not(target_family = "wasm"))]
fn now_unix_millis() -> Option<u64> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .and_then(|duration| u64::try_from(duration.as_millis()).ok())
}

#[cfg(target_family = "wasm")]
fn now_unix_millis() -> Option<u64> {
    None
}
