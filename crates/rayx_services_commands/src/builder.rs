//! `add_commands`: in Rust ([`CommandsAppBuilderExt`]) and in TS ([`CommandsBuilder`], a class
//! extension of the host's `RayXAppBuilder`).

use std::sync::Arc;

use nsh_core::prelude::*;
use rayx_app::AppBuilder;
use rayx_app::services::commands::{CommandService, ContextService};
use rayx_app::services::input_map::InputMapService;
use rayx_app_nsh::ts::RayXAppBuilder;

use crate::context::RayxContext;
use crate::service::RayxCommands;

/// Registers RayX's context and command services on an app builder.
pub trait CommandsAppBuilderExt: AppBuilder {
    /// Registers RayX's context service (`dyn ContextService`) and command service
    /// (`dyn CommandService`), replacing any earlier registrations of them.
    ///
    /// If the app also registers a `dyn InputMapService` (before or after `add_commands()`),
    /// the command service resolves shortcuts and active keybindings from it automatically; no
    /// `InputMapService` is required to use commands standalone.
    fn add_commands(mut self) -> Self {
        let services = self.services_mut();
        services
            .replace_service_factory_as::<RayxContext>(|_services| Arc::new(RayxContext::new()));
        services.replace_service_factory_as::<dyn ContextService>(|services| {
            services
                .required_service::<RayxContext>()
                .unwrap_or_else(|error| panic!("RayX's context service is registered: {error}"))
                as Arc<dyn ContextService>
        });
        services.replace_service_factory_as::<RayxCommands>(|services| {
            let context = services
                .required_service::<dyn ContextService>()
                .unwrap_or_else(|error| panic!("RayX's context service is registered: {error}"));
            let input_map = services.service::<dyn InputMapService>();
            Arc::new(RayxCommands::new(context, input_map))
        });
        services.replace_service_factory_as::<dyn CommandService>(|services| {
            services
                .required_service::<RayxCommands>()
                .unwrap_or_else(|error| panic!("RayX's command service is registered: {error}"))
                as Arc<dyn CommandService>
        });
        self
    }
}

impl<T> CommandsAppBuilderExt for T where T: AppBuilder {}

/// The methods `rayx_services_commands` adds to TS's `RayXAppBuilder`.
pub struct CommandsBuilder;

#[class_extension(RayXAppBuilder)]
impl CommandsBuilder {
    /// Registers the app's context service (`ContextService`) and command service
    /// (`CommandService`), optionally observing `InputMapService` if registered.
    pub fn add_commands(
        this: Instance<RayXAppBuilder>,
    ) -> Result<Instance<RayXAppBuilder>, String> {
        RayXAppBuilder::extend_with(this, CommandsAppBuilderExt::add_commands)
    }
}
