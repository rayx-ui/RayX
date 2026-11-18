use std::fmt;

use super::AppBuilder;
use crate::configuration::{Configuration, ConfigurationBuilder, ConfigurationError};
use crate::di::{ServiceKey, ServiceProvider, ServiceValidationError};

/// Why [`RayXAppBuilder::build`] made no provider.
#[derive(Debug)]
pub enum AppBuildError {
    /// The registrations are not complete: a required service is missing, or a singleton
    /// depends on a scoped one ([`ServiceProvider::validate`]). Nothing was made.
    Services(ServiceValidationError),
    /// The configuration did not load.
    Configuration(ConfigurationError),
}

impl fmt::Display for AppBuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Services(error) => error.fmt(f),
            Self::Configuration(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for AppBuildError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Services(error) => Some(error),
            Self::Configuration(error) => Some(error),
        }
    }
}

impl From<ServiceValidationError> for AppBuildError {
    fn from(error: ServiceValidationError) -> Self {
        Self::Services(error)
    }
}

impl From<ConfigurationError> for AppBuildError {
    fn from(error: ConfigurationError) -> Self {
        Self::Configuration(error)
    }
}

/// The app builder (.NET's `HostApplicationBuilder`): the configuration's sources and the
/// services, built into the app's root provider. Service crates extend it through
/// [`AppBuilder`] (`with_feed(..)`); the NSH layer builds one per surface.
#[derive(Debug, Default)]
pub struct RayXAppBuilder {
    pub configuration: ConfigurationBuilder,
    pub services: ServiceProvider,
}

impl AppBuilder for RayXAppBuilder {
    fn configuration_mut(&mut self) -> &mut ConfigurationBuilder {
        &mut self.configuration
    }

    fn services_mut(&mut self) -> &mut ServiceProvider {
        &mut self.services
    }
}

impl RayXAppBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// The builder with `extension` applied: an extension of the builder (`fn add_weather(builder:
    /// RayXAppBuilder) -> RayXAppBuilder`) in the chain, as TS's `builder.apply(f)` applies a TS
    /// function of the builder.
    pub fn apply(self, extension: impl FnOnce(Self) -> Self) -> Self {
        extension(self)
    }

    /// The root provider: the services, with the [`Configuration`] the sources build registered
    /// as a singleton (where options bind from). It first checks what the registrations
    /// require ([`ServiceProvider::validate`]): a missing service, or a singleton depending on a
    /// scoped one, fails it before any source loads or any factory runs. Its startup services
    /// are not initialized yet ([`StartupRegistry::initialize`](crate::di::StartupRegistry::initialize)
    /// does that, after it validates the options registered with `validate_on_start`). A source
    /// that loads asynchronously fails it; use [`Self::build_async`] then.
    pub fn build(self) -> Result<ServiceProvider, AppBuildError> {
        Self::validate(&self.services)?;
        let configuration = self.configuration.build()?;
        Ok(Self::root(self.services, configuration))
    }

    /// [`Self::build`], awaiting asynchronous configuration sources.
    pub async fn build_async(self) -> Result<ServiceProvider, AppBuildError> {
        Self::validate(&self.services)?;
        let configuration = self.configuration.build_async().await?;
        Ok(Self::root(self.services, configuration))
    }

    /// The checks of `services`, with the [`Configuration`] the build registers counted in.
    fn validate(services: &ServiceProvider) -> Result<(), ServiceValidationError> {
        services.validate_with(&[ServiceKey::of::<Configuration>()])
    }

    fn root(services: ServiceProvider, configuration: Configuration) -> ServiceProvider {
        services.with_singleton(configuration)
    }
}
