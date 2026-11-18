//! `add_i18n` and `add_message_formatter`: in Rust ([`I18nAppBuilderExt`]) and in TS
//! ([`I18nBuilder`], a class extension of the host's `RayXAppBuilder`).

use std::sync::Arc;

use nsh_core::prelude::*;
use rayx_app::di::{StartupInitializationFailure, StartupService, StartupServiceType};
use rayx_app::services::assets::AssetService;
use rayx_app::services::i18n::{I18nService, MessageFormatter};
use rayx_app::services::platform::PlatformInfoService;
use rayx_app::{AppBuilder, ServiceAppBuilderExt as _};
use rayx_app_nsh::ts::{RayXAppBuilder, record};

use crate::formatter::{Following, RayxFormatter};
use crate::options_ts::I18nOptions as TsI18nOptions;
use crate::service::{Loading, RayxI18n};
use crate::{I18N_CONFIG_PATH, I18nOptions};

/// What an app registering RayX's i18n service without an asset service is told.
const ASSETS_HINT: &str = "add_assets(..) (addAssets in TS) from rayx_services_assets, or \
                           register your own dyn AssetService";

/// What an app registering RayX's i18n service without a platform service is told.
const PLATFORM_HINT: &str = "add_platform() (addPlatform in TS) from rayx_services_platform, or \
                             register your own dyn PlatformInfoService";

/// What an app registering RayX's message formatter without an i18n service is told.
const I18N_HINT: &str = "add_i18n(..) (addI18n in TS) from rayx_services_i18n, or register your \
                         own dyn I18nService";

/// Registers RayX's i18n service and message formatter on an app builder.
pub trait I18nAppBuilderExt: AppBuilder {
    /// Registers RayX's i18n service as the app's `dyn I18nService`, replacing any other, with
    /// RayX's `dyn MessageFormatter` over it ([`add_message_formatter`]): a startup service
    /// that loads the asset catalog's translations when the app starts (after the app's
    /// `dyn AssetService`, which it requires), then follows the user's languages and clock as
    /// the app's `dyn PlatformInfoService` (which it requires) reports them. Its
    /// [`I18nOptions`] bind from [`I18N_CONFIG_PATH`], then `configure` changes them; a
    /// language tag that does not read fails startup.
    ///
    /// [`add_message_formatter`]: Self::add_message_formatter
    fn add_i18n(self, configure: impl Fn(&mut I18nOptions) + Send + Sync + 'static) -> Self {
        let mut builder = self.add_options::<I18nOptions>(|options| {
            options
                .bind(I18N_CONFIG_PATH)
                .configure(configure)
                .validate(I18nOptions::is_valid, "an i18n language tag is invalid")
                .validate_on_start();
        });
        let services = builder.services_mut();
        services.require_for::<dyn I18nService, dyn AssetService>(ASSETS_HINT);
        services.require_for::<dyn I18nService, dyn PlatformInfoService>(PLATFORM_HINT);
        services.replace_service_factory_as::<RayxI18n>(|services| {
            let options = services
                .options::<I18nOptions>()
                .unwrap_or_else(|error| panic!("the i18n options are invalid: {error}"))
                .cloned();
            RayxI18n::new(
                options,
                services.service::<dyn AssetService>(),
                services.service::<dyn PlatformInfoService>(),
            )
            .unwrap_or_else(|error| panic!("the i18n options are invalid: {error}"))
        });
        // Replacing drops the startup service keyed `dyn I18nService` (an earlier one's), so
        // the service's own startup registration, under that key, comes after.
        services.replace_service_factory_as::<dyn I18nService>(|services| {
            services
                .required_service::<RayxI18n>()
                .unwrap_or_else(|error| panic!("RayX's i18n service is registered: {error}"))
        });
        services.add_startup_resolved_async(
            StartupServiceType::of::<dyn I18nService>(),
            |context| {
                let i18n = context.services().required_service::<RayxI18n>();
                Box::pin(async move {
                    let i18n = i18n.map_err(|error| StartupInitializationFailure {
                        service: StartupServiceType::of::<dyn I18nService>(),
                        message: error.to_string(),
                        timed_out: false,
                    })?;
                    Ok(Arc::new(Loading(i18n)) as StartupService)
                })
            },
        );
        builder.add_message_formatter()
    }

    /// Registers RayX's message formatter as the app's `dyn MessageFormatter`, replacing any
    /// other, over the app's `dyn I18nService` (which it requires): MessageFormat 2 messages,
    /// numbers, dates and plurals formatted synchronously for the service's locale, following
    /// every state the service publishes. A startup service after the i18n
    /// service: on NSH, from then on, `tr!` follows the service's changes and NSH's locale the
    /// language the app picked. Its missing-message policy and bidi isolation are
    /// [`I18nOptions`]' (`add_i18n` sets them; over an app's own service,
    /// `add_options::<I18nOptions>(..)` does).
    fn add_message_formatter(mut self) -> Self {
        let services = self.services_mut();
        services.require_for::<dyn MessageFormatter, dyn I18nService>(I18N_HINT);
        services.replace_service_factory_as::<RayxFormatter>(|services| {
            let options = services
                .options::<I18nOptions>()
                .map(|options| options.cloned())
                .unwrap_or_default();
            let i18n = services
                .required_service::<dyn I18nService>()
                .unwrap_or_else(|error| panic!("an i18n service is registered: {error}"));
            RayxFormatter::new(i18n, &options)
        });
        services.replace_service_factory_as::<dyn MessageFormatter>(|services| {
            services
                .required_service::<RayxFormatter>()
                .unwrap_or_else(|error| panic!("RayX's message formatter is registered: {error}"))
        });
        services.add_startup_resolved_async(
            StartupServiceType::of::<dyn MessageFormatter>(),
            |context| {
                let formatter = context.services().required_service::<RayxFormatter>();
                Box::pin(async move {
                    let formatter = formatter.map_err(|error| StartupInitializationFailure {
                        service: StartupServiceType::of::<dyn MessageFormatter>(),
                        message: error.to_string(),
                        timed_out: false,
                    })?;
                    Ok(Arc::new(Following(formatter)) as StartupService)
                })
            },
        );
        self
    }
}

impl<T> I18nAppBuilderExt for T where T: AppBuilder {}

/// The methods `rayx_services_i18n` adds to TS's `RayXAppBuilder`.
pub struct I18nBuilder;

#[class_extension(RayXAppBuilder)]
impl I18nBuilder {
    /// Registers the app's i18n service (it needs an asset service, `addAssets`, and a platform
    /// service, `addPlatform`) and the formatter `I18nService` formats with; `configure` sets
    /// its options (`options.defaultLanguage("en")`) after configuration's.
    pub fn add_i18n(
        this: Instance<RayXAppBuilder>,
        #[args(options)] configure: Option<Callback<Instance<TsI18nOptions>>>,
    ) -> Result<Instance<RayXAppBuilder>, String> {
        let steps = record(TsI18nOptions::new(), configure, TsI18nOptions::take)?;
        RayXAppBuilder::extend_with(this, move |builder| {
            builder.add_i18n(move |options| {
                for step in &steps {
                    step.apply(options);
                }
            })
        })
    }

    /// Registers the formatter `I18nService` formats with over an i18n service of the app's own
    /// (a TS implementation registered under `I18nService`); `addI18n` registers it already.
    pub fn add_message_formatter(
        this: Instance<RayXAppBuilder>,
    ) -> Result<Instance<RayXAppBuilder>, String> {
        RayXAppBuilder::extend_with(this, I18nAppBuilderExt::add_message_formatter)
    }
}
