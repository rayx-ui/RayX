use super::AppBuilder;
use crate::configuration::ConfigurationBuilder;

pub trait ConfigurationAppBuilderExt: AppBuilder {
    fn configure(mut self, configure: impl FnOnce(&mut ConfigurationBuilder)) -> Self {
        configure(self.configuration_mut());
        self
    }
}

impl<T> ConfigurationAppBuilderExt for T where T: AppBuilder {}
