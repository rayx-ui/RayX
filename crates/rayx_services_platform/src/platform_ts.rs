//! The platform service for TSX.

use std::sync::Arc;

use nsh_core::JsonValue;
use nsh_core::prelude::*;
use rayx_app::ChangeSubscription;
use rayx_app::services::platform::PlatformInfoService as Platform;
use rayx_app_nsh::ts::{Listeners, ServiceClass, json};

/// The device and the user's settings, for TSX:
/// `services.getRequiredService(PlatformInfoService).current()` (the platform service's snapshot:
/// platform, form factor, input, viewport, insets, keyboard, color scheme, accessibility, locale,
/// features, displays, window, …); `onChange` hears every change of them.
#[class]
pub struct PlatformInfoService {
    platform: Arc<dyn Platform>,
    listeners: Listeners,
}

impl ServiceClass for PlatformInfoService {
    type Service = dyn Platform;

    fn wrap(platform: Arc<dyn Platform>) -> Self {
        Self {
            platform,
            listeners: Listeners::default(),
        }
    }
}

#[class]
impl PlatformInfoService {
    /// The device and the user's settings now.
    #[ts("(): PlatformSnapshot")]
    pub fn current(&self) -> JsonValue {
        json(&*self.platform.platform().get())
    }

    /// The contract this class is the token of (TS types only: `null` at run time).
    #[getter]
    #[ts("(): PlatformInfoServiceContract | null")]
    pub fn contract(&self) -> Option<bool> {
        None
    }

    /// Calls `listener` with the snapshot after every change of the device or the user's
    /// settings, until the function it returns is called (or the reactive owner of the call
    /// goes), or this object is disposed.
    #[ts("(listener: (snapshot: PlatformSnapshot) => void): () => void")]
    pub fn on_change(
        &mut self,
        #[args(snapshot)] listener: Callback<JsonValue>,
    ) -> Result<Stop, String> {
        let platform = Arc::clone(&self.platform);
        self.listeners.listen(listener, move |post| {
            Ok::<ChangeSubscription, String>(
                platform
                    .platform()
                    .subscribe(move |snapshot| post(json(&**snapshot))),
            )
        })
    }
}
