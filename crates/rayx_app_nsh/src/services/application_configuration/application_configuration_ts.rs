//! The application configuration service for TSX.

use std::sync::Arc;

use nsh_core::JsonValue;
use nsh_core::prelude::*;
use rayx_app::ChangeSubscription;
use rayx_runtime::services::application_configuration::{
    ApplicationConfigurationService as Configuration, LayoutDirection,
};

use crate::di::provider_ts::ServiceClass;
use crate::services::json;
use crate::services::listeners_ts::Listeners;

/// The app's configuration (`RayX:Application`), for TSX:
/// `services.getRequiredService(ApplicationConfigurationService)`, its settings (`current()`),
/// the layout direction and whether motion is reduced; `onChange` hears every change of the
/// settings.
#[class]
pub struct ApplicationConfigurationService {
    service: Arc<Configuration>,
    listeners: Listeners,
}

impl ServiceClass for ApplicationConfigurationService {
    type Service = Configuration;

    fn wrap(service: Arc<Configuration>) -> Self {
        Self {
            service,
            listeners: Listeners::default(),
        }
    }
}

#[class]
impl ApplicationConfigurationService {
    /// The settings now.
    pub fn current(&self) -> JsonValue {
        json(&self.service.options())
    }

    /// `"ltr"` or `"rtl"`.
    pub fn layout_direction(&self) -> String {
        match self.service.layout_direction() {
            LayoutDirection::Ltr => "ltr",
            LayoutDirection::Rtl => "rtl",
        }
        .to_owned()
    }

    /// Whether motion is reduced, by the user's platform setting or the app's.
    pub fn reduced_motion(&self) -> bool {
        self.service.effective_motion(None).is_reduced()
    }

    /// Calls `listener` with the settings after every change, until the function it returns is
    /// called (or the reactive owner of the call goes), or this object is disposed.
    pub fn on_change(
        &mut self,
        #[args(settings)] listener: Callback<JsonValue>,
    ) -> Result<Stop, String> {
        let service = Arc::clone(&self.service);
        self.listeners.listen(listener, move |post| {
            Ok::<ChangeSubscription, String>(
                service.subscribe(move |snapshot| post(json(&snapshot.options))),
            )
        })
    }
}
