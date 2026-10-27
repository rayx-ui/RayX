use super::{FONT_CONFIG_PATH, FontOptions, FontRegistrar, FontService};
use crate::services::assets::AssetAppBuilderExt;
use rayx_core::{AppBuilder, ServiceAppBuilderExt as _};

/// Registers the [`FontService`] on an app builder.
pub trait FontAppBuilderExt: AppBuilder {
    /// Registers the app's [`FontService`] (a startup singleton that registers the asset
    /// catalog's fonts, after the asset service, registered too when missing) unless one is
    /// registered. Its [`FontOptions`] bind from [`FONT_CONFIG_PATH`], then `configure`
    /// changes them; invalid aliases fail startup. It registers through the app's
    /// `dyn FontRegistrar` (`rayx_app` registers NSH's).
    fn add_fonts(self, configure: impl Fn(&mut FontOptions) + Send + Sync + 'static) -> Self {
        let mut builder = self
            .add_assets(|_| {})
            .add_options::<FontOptions>(|options| {
                options
                    .bind(FONT_CONFIG_PATH)
                    .configure(configure)
                    .validate(
                        |options| options.clone().normalized().is_ok(),
                        "the font aliases are invalid",
                    )
                    .validate_on_start();
            });
        builder
            .services_mut()
            .try_add_app_service_factory::<FontService>(|services| {
                let options = services
                    .options::<FontOptions>()
                    .unwrap_or_else(|error| panic!("the font options are invalid: {error}"))
                    .cloned();
                FontService::new(options, services.service::<dyn FontRegistrar>())
                    .unwrap_or_else(|error| panic!("the font options are invalid: {error}"))
            });
        builder
    }

    /// Registers `service` as the app's [`FontService`], replacing any other.
    fn with_font_service(mut self, service: FontService) -> Self {
        self.services_mut().replace_app_service(service);
        self
    }
}

impl<T> FontAppBuilderExt for T where T: AppBuilder {}
