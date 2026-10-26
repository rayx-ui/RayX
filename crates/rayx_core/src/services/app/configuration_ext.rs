use super::AppBuilder;
use crate::configuration::ConfigurationBuilder;

pub trait ConfigurationAppBuilderExt: AppBuilder {
    /// Adds configuration sources (.NET's `ConfigureAppConfiguration`):
    /// `.configure_app_configuration(|configuration| { configuration.add_json(SETTINGS); })`.
    fn configure_app_configuration(
        mut self,
        configure: impl FnOnce(&mut ConfigurationBuilder),
    ) -> Self {
        configure(self.configuration_mut());
        self
    }
}

impl<T> ConfigurationAppBuilderExt for T where T: AppBuilder {}
