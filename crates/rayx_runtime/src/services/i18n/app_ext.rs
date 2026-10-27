use super::{I18N_CONFIG_PATH, I18nOptions, I18nService};
use crate::services::assets::AssetAppBuilderExt;
use rayx_core::{AppBuilder, ServiceAppBuilderExt as _};

/// Registers the [`I18nService`] on an app builder.
pub trait I18nAppBuilderExt: AppBuilder {
    /// Registers the app's [`I18nService`] (a startup singleton that loads the asset catalog's
    /// translations, after the asset service, registered too when missing) unless one is
    /// registered. Its [`I18nOptions`] bind from [`I18N_CONFIG_PATH`], then `configure` changes
    /// them; a language tag that does not read fails startup. With
    /// [`I18nOptions::follow_platform`] it follows the platform service's languages and clock
    /// when one is registered.
    fn add_i18n(self, configure: impl Fn(&mut I18nOptions) + Send + Sync + 'static) -> Self {
        let mut builder = self
            .add_assets(|_| {})
            .add_options::<I18nOptions>(|options| {
                options
                    .bind(I18N_CONFIG_PATH)
                    .configure(configure)
                    .validate(I18nOptions::is_valid, "an i18n language tag is invalid")
                    .validate_on_start();
            });
        builder
            .services_mut()
            .try_add_app_service_factory::<I18nService>(|services| {
                let options = services
                    .options::<I18nOptions>()
                    .unwrap_or_else(|error| panic!("the i18n options are invalid: {error}"))
                    .cloned();
                I18nService::new(options)
                    .unwrap_or_else(|error| panic!("the i18n options are invalid: {error}"))
            });
        builder
    }

    /// Registers `service` as the app's [`I18nService`], replacing any other.
    fn with_i18n_service(mut self, service: I18nService) -> Self {
        self.services_mut().replace_app_service(service);
        self
    }
}

impl<T> I18nAppBuilderExt for T where T: AppBuilder {}
