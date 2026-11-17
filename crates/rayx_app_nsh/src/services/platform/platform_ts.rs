//! The platform service for TSX.

use std::sync::Arc;

use nsh_core::JsonValue;
use nsh_core::prelude::*;
use rayx_app::ChangeSubscription;
use rayx_runtime::services::platform::PlatformInfoService as Platform;

use crate::di::provider_ts::ServiceClass;
use crate::services::json;
use crate::services::listeners_ts::Listeners;

/// The device and the user's settings, for TSX:
/// `services.getRequiredService(PlatformInfoService).current()` (the platform service's snapshot:
/// platform, form factor, input, viewport, insets, keyboard, color scheme, accessibility, locale,
/// features, displays, window, …); `onChange` hears every change of them.
#[class]
pub struct PlatformInfoService {
    platform: Arc<Platform>,
    listeners: Listeners,
}

impl ServiceClass for PlatformInfoService {
    type Service = Platform;

    fn wrap(platform: Arc<Platform>) -> Self {
        Self {
            platform,
            listeners: Listeners::default(),
        }
    }
}

#[class]
impl PlatformInfoService {
    /// The device and the user's settings now.
    pub fn current(&self) -> JsonValue {
        json(&self.platform.current())
    }

    /// Calls `listener` with the snapshot after every change of the device or the user's
    /// settings, until the function it returns is called (or the reactive owner of the call
    /// goes), or this object is disposed.
    pub fn on_change(
        &mut self,
        #[args(snapshot)] listener: Callback<JsonValue>,
    ) -> Result<Stop, String> {
        let platform = Arc::clone(&self.platform);
        self.listeners.listen(listener, move |post| {
            Ok::<ChangeSubscription, String>(
                platform.subscribe(move |event| post(json(&event.current))),
            )
        })
    }
}
