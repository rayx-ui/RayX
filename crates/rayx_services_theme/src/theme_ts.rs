//! The theme service for TSX.

use std::sync::Arc;

use nsh_core::JsonValue;
use nsh_core::prelude::*;
use rayx_app::ChangeSubscription;
use rayx_app::services::theme::ThemeService as Theme;
use rayx_app_nsh::ts::{Listeners, ServiceClass, json};

use crate::options_ts::choice;

/// The app's theme, for TSX: `services.getRequiredService(ThemeService)`, its snapshot
/// (`current()`: the theme document's modifiers with the context each shows, the light/dark
/// choice and a revision); `onChange` hears every change, `setContext` picks a modifier's
/// context and `setColorMode` the light/dark choice (promises).
///
/// It wraps whichever `dyn ThemeService` is registered (RayX's, `addTheme`, or a TS
/// implementation of the contract `ThemeServiceContract` registered under this class).
#[class]
pub struct ThemeService {
    service: Arc<dyn Theme>,
    listeners: Listeners,
}

impl ServiceClass for ThemeService {
    type Service = dyn Theme;

    fn wrap(service: Arc<dyn Theme>) -> Self {
        Self {
            service,
            listeners: Listeners::default(),
        }
    }
}

/// The service `this` wraps.
fn service(this: &Instance<ThemeService>) -> Arc<dyn Theme> {
    this.with(|this| Arc::clone(&this.service))
}

#[class]
impl ThemeService {
    /// The contract this class is the token of (TS types only: `null` at run time).
    #[getter]
    #[ts("(): ThemeServiceContract | null")]
    pub fn contract(&self) -> Option<bool> {
        None
    }

    /// The modifiers, the light/dark choice and the revision now.
    #[ts("(): ThemeSnapshot")]
    pub fn current(&self) -> JsonValue {
        json(&*self.service.theme().get())
    }

    /// Shows `context` for the theme's modifier `modifier` (`setContext("accent", "cyan")`);
    /// resolves with whether that differs from what was shown, or rejects with the service's
    /// error (an unknown modifier or context).
    pub async fn set_context(
        this: Instance<Self>,
        modifier: String,
        context: String,
    ) -> Result<bool, String> {
        service(&this)
            .set_context(modifier, context)
            .await
            .map_err(|error| error.to_string())
    }

    /// Makes the light/dark modifier follow the color scheme shown (`"System"`) or show one of
    /// its contexts (`"Light"`, `"Dark"`); resolves with whether the choice changed.
    #[ts("(choice: ColorModeChoice): boolean")]
    pub async fn set_color_mode(this: Instance<Self>, choice: String) -> Result<bool, String> {
        let choice = self::choice(&choice)?;
        service(&this)
            .set_color_mode(choice)
            .await
            .map_err(|error| error.to_string())
    }

    /// Calls `listener` with the snapshot after every change, until the function it returns is
    /// called (or the reactive owner of the call goes), or this object is disposed.
    #[ts("(listener: (snapshot: ThemeSnapshot) => void): () => void")]
    pub fn on_change(
        &mut self,
        #[args(snapshot)] listener: Callback<JsonValue>,
    ) -> Result<Stop, String> {
        let service = Arc::clone(&self.service);
        self.listeners.listen(listener, move |post| {
            Ok::<ChangeSubscription, String>(
                service
                    .theme()
                    .subscribe(move |snapshot| post(json(&**snapshot))),
            )
        })
    }
}

impl ThemeService {
    /// The service, for Rust bridges that take an `Instance<ThemeService>`.
    pub fn service(&self) -> &Arc<dyn Theme> {
        &self.service
    }
}
