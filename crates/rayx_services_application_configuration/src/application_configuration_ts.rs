//! The application configuration service for TSX.

use std::sync::Arc;

use nsh_core::JsonValue;
use nsh_core::prelude::*;
use rayx_app::ChangeSubscription;
use rayx_app::services::application_configuration::{
    ApplicationConfiguration, ApplicationConfigurationService as Configuration, LayoutDirection,
};
use rayx_app_nsh::ts::{Listeners, ServiceClass, json};

/// The app's configuration (`RayX:Application`), for TSX:
/// `services.getRequiredService(ApplicationConfigurationService)`, its settings (`current()`),
/// the layout direction and whether motion is reduced; `onChange` hears every change of the
/// settings or of the platform's motion setting, and `replaceOptions` replaces the settings (a
/// promise).
///
/// It wraps whichever `dyn ApplicationConfigurationService` is registered (RayX's,
/// `addApplicationConfiguration`, or a TS implementation of the contract
/// `ApplicationConfigurationServiceContract` registered under this class).
#[class]
pub struct ApplicationConfigurationService {
    service: Arc<dyn Configuration>,
    listeners: Listeners,
}

impl ServiceClass for ApplicationConfigurationService {
    type Service = dyn Configuration;

    fn wrap(service: Arc<dyn Configuration>) -> Self {
        Self {
            service,
            listeners: Listeners::default(),
        }
    }
}

/// The service `this` wraps.
fn service(this: &Instance<ApplicationConfigurationService>) -> Arc<dyn Configuration> {
    this.with(|this| Arc::clone(&this.service))
}

#[class]
impl ApplicationConfigurationService {
    /// The contract this class is the token of (TS types only: `null` at run time).
    #[getter]
    #[ts("(): ApplicationConfigurationServiceContract | null")]
    pub fn contract(&self) -> Option<bool> {
        None
    }

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

    /// Replaces the settings with `settings` (all of them, as `current()` gives them; not the
    /// startup-only name and description); resolves with whether they changed, or rejects with
    /// the service's error.
    pub async fn replace_options(
        this: Instance<Self>,
        settings: JsonValue,
    ) -> Result<bool, String> {
        let options: ApplicationConfiguration =
            serde_json::from_value(settings).map_err(|error| format!("settings: {error}"))?;
        service(&this)
            .replace_options(options)
            .await
            .map_err(|error| error.to_string())
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
                service
                    .configuration()
                    .subscribe(move |snapshot| post(json(&snapshot.options))),
            )
        })
    }
}

impl ApplicationConfigurationService {
    /// The service, for Rust bridges that take an `Instance<ApplicationConfigurationService>`.
    pub fn service(&self) -> &Arc<dyn Configuration> {
        &self.service
    }
}
