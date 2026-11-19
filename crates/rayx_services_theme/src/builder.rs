//! `add_theme`: in Rust ([`ThemeAppBuilderExt`]) and in TS ([`ThemeBuilder`], a class
//! extension of the host's `RayXAppBuilder`).

use std::sync::Arc;

use nsh_core::prelude::*;
use rayx_app::services::platform::PlatformInfoService;
use rayx_app::services::theme::ThemeService;
use rayx_app::{AppBuilder, ServiceAppBuilderExt as _};
use rayx_app_nsh::ts::{RayXAppBuilder, record};

use crate::nsh::provisional_pick;
use crate::options::{THEME_CONFIG_PATH, ThemeOptions};
use crate::options_ts::ThemeOptions as TsThemeOptions;
use crate::service::RayxTheme;

/// What an app registering RayX's theme service without a platform service is told.
const PLATFORM_HINT: &str = "add_platform() (addPlatform in TS) from rayx_services_platform, or \
                             register your own dyn PlatformInfoService";

/// Registers RayX's theme service on an app builder.
pub trait ThemeAppBuilderExt: AppBuilder {
    /// Registers RayX's theme service as the app's `dyn ThemeService`, replacing any other: a
    /// startup service that applies the start contexts and the color mode to the surface's theme
    /// before the app is ready, takes picks from any thread, and keeps the light/dark modifier in
    /// step with the color scheme the app's `dyn PlatformInfoService` (which it requires)
    /// reports, or with the app's choice. Its [`ThemeOptions`] bind from [`THEME_CONFIG_PATH`],
    /// then `configure` changes them; an empty name, or a start context for the light/dark
    /// modifier, fails startup.
    ///
    /// On an NSH surface it also picks at once, from `configure`'s options, so the app's first
    /// frame (a splash included) shows them; startup then reconciles that pick with the bound
    /// options and the platform.
    fn add_theme(self, configure: impl Fn(&mut ThemeOptions) + Send + Sync + 'static) -> Self {
        let configure = Arc::new(configure);
        let mut configured = ThemeOptions::default();
        configure(&mut configured);
        let provisional = provisional_pick(&configured);
        let mut builder = self.add_options::<ThemeOptions>(move |options| {
            options
                .bind(THEME_CONFIG_PATH)
                .configure(move |options| configure(options));
        });
        let services = builder.services_mut();
        services.require_for::<dyn ThemeService, dyn PlatformInfoService>(PLATFORM_HINT);
        // Registered as `dyn ThemeService` and a startup service registered as it: a later
        // registration of `dyn ThemeService` (the app's own) removes both.
        services.add_startup_singleton_for::<dyn ThemeService, _>(RayxTheme::new(
            configured,
            provisional,
        ));
        builder
    }
}

impl<T> ThemeAppBuilderExt for T where T: AppBuilder {}

/// The methods `rayx_services_theme` adds to TS's `RayXAppBuilder`.
pub struct ThemeBuilder;

#[class_extension(RayXAppBuilder)]
impl ThemeBuilder {
    /// Registers the app's theme service, which picks the theme's modifier contexts and keeps
    /// its light/dark modifier in step with the color scheme (it needs a platform service,
    /// `addPlatform`); `configure` sets its options (`options.colorMode("Dark")`,
    /// `options.context("accent", "cyan")`) after configuration's. It picks at once, so the
    /// first frame shows the picks.
    pub fn add_theme(
        this: Instance<RayXAppBuilder>,
        #[args(options)] configure: Option<Callback<Instance<TsThemeOptions>>>,
    ) -> Result<Instance<RayXAppBuilder>, String> {
        let steps = record(TsThemeOptions::new(), configure, TsThemeOptions::take)?;
        RayXAppBuilder::extend_with(this, move |builder| {
            builder.add_theme(move |options| {
                for step in &steps {
                    step.apply(options);
                }
            })
        })
    }
}
