use super::AppBuilder;
use crate::configuration::{Configuration, ConfigurationBuilder, ConfigurationError};
use crate::di::ServiceProvider;

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

    /// The root provider: the services, with the [`Configuration`] the sources build registered
    /// as a singleton (where options bind from). Its startup services are not initialized yet
    /// ([`StartupRegistry::initialize`](crate::di::StartupRegistry::initialize) does that, after
    /// it validates the options registered with `validate_on_start`). A source that loads
    /// asynchronously fails it; use [`Self::build_async`] then.
    pub fn build(self) -> Result<ServiceProvider, ConfigurationError> {
        let configuration = self.configuration.build()?;
        Ok(Self::root(self.services, configuration))
    }

    /// [`Self::build`], awaiting asynchronous configuration sources.
    pub async fn build_async(self) -> Result<ServiceProvider, ConfigurationError> {
        let configuration = self.configuration.build_async().await?;
        Ok(Self::root(self.services, configuration))
    }

    fn root(services: ServiceProvider, configuration: Configuration) -> ServiceProvider {
        services.with_singleton(configuration)
    }
}
