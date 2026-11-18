//! `add_fonts`: in Rust ([`FontAppBuilderExt`]) and in TS ([`FontBuilder`], a class extension
//! of the host's `RayXAppBuilder`).

use std::sync::Arc;

use nsh_core::prelude::*;
use rayx_app::di::{StartupInitializationFailure, StartupService, StartupServiceType};
use rayx_app::services::assets::AssetService;
use rayx_app::services::fonts::FontService;
use rayx_app::{AppBuilder, ServiceAppBuilderExt as _};
use rayx_app_nsh::EngineRunner;
use rayx_app_nsh::ts::{RayXAppBuilder, record};

use crate::nsh_registrar::NshFontRegistrar;
use crate::options_ts::FontOptions as TsFontOptions;
use crate::registrar::FontRegistrar;
use crate::service::{RayxFonts, Registering};
use crate::{FONT_CONFIG_PATH, FontOptions};

/// What an app registering RayX's font service without an asset service is told.
const ASSETS_HINT: &str = "add_assets(..) (addAssets in TS) from rayx_services_assets, or \
                           register your own dyn AssetService";

/// Registers RayX's font service on an app builder.
pub trait FontAppBuilderExt: AppBuilder {
    /// Registers RayX's font service as the app's `dyn FontService`, replacing any other: a
    /// startup service that registers the asset catalog's fonts with NSH when the app starts
    /// (after the app's `dyn AssetService`, which it requires), and others from any thread
    /// while it runs. Its [`FontOptions`] bind from [`FONT_CONFIG_PATH`], then `configure`
    /// changes them; invalid aliases fail startup.
    fn add_fonts(self, configure: impl Fn(&mut FontOptions) + Send + Sync + 'static) -> Self {
        add_fonts_with(self, configure, |services| {
            Arc::new(NshFontRegistrar {
                runner: services
                    .service::<EngineRunner>()
                    .map(|runner| (*runner).clone()),
            })
        })
    }
}

impl<T> FontAppBuilderExt for T where T: AppBuilder {}

/// `add_fonts`, registering through the registrar `registrar` makes.
pub(crate) fn add_fonts_with<B: AppBuilder>(
    mut builder: B,
    configure: impl Fn(&mut FontOptions) + Send + Sync + 'static,
    registrar: impl Fn(&rayx_app::ServiceProvider) -> Arc<dyn FontRegistrar> + Send + Sync + 'static,
) -> B {
    builder = builder.add_options::<FontOptions>(|options| {
        options
            .bind(FONT_CONFIG_PATH)
            .configure(configure)
            .validate(
                |options| options.clone().normalized().is_ok(),
                "the font aliases are invalid",
            )
            .validate_on_start();
    });
    let services = builder.services_mut();
    services.require_for::<dyn FontService, dyn AssetService>(ASSETS_HINT);
    services.replace_service_factory_as::<RayxFonts>(move |services| {
        let options = services
            .options::<FontOptions>()
            .unwrap_or_else(|error| panic!("the font options are invalid: {error}"))
            .cloned();
        RayxFonts::new(
            options,
            Some(registrar(services)),
            services.service::<dyn AssetService>(),
        )
        .unwrap_or_else(|error| panic!("the font options are invalid: {error}"))
    });
    services.replace_service_factory_as::<dyn FontService>(|services| {
        services
            .required_service::<RayxFonts>()
            .unwrap_or_else(|error| panic!("RayX's font service is registered: {error}"))
    });
    services.add_startup_resolved_async(StartupServiceType::of::<dyn FontService>(), |context| {
        let fonts = context.services().required_service::<RayxFonts>();
        Box::pin(async move {
            let fonts = fonts.map_err(|error| StartupInitializationFailure {
                service: StartupServiceType::of::<dyn FontService>(),
                message: error.to_string(),
                timed_out: false,
            })?;
            Ok(Arc::new(Registering(fonts)) as StartupService)
        })
    });
    builder
}

/// The methods `rayx_services_fonts` adds to TS's `RayXAppBuilder`.
pub struct FontBuilder;

#[class_extension(RayXAppBuilder)]
impl FontBuilder {
    /// Registers RayX's font service (it needs an asset service: `addAssets`); `configure` sets
    /// its aliases (`options.alias("script", (alias) => alias.family("Pacifico"))`) after
    /// configuration's options.
    pub fn add_fonts(
        this: Instance<RayXAppBuilder>,
        #[args(options)] configure: Option<Callback<Instance<TsFontOptions>>>,
    ) -> Result<Instance<RayXAppBuilder>, String> {
        let steps = record(TsFontOptions::new(), configure, TsFontOptions::take)?;
        RayXAppBuilder::extend_with(this, move |builder| {
            builder.add_fonts(move |options| {
                for step in &steps {
                    step.apply(options);
                }
            })
        })
    }
}
