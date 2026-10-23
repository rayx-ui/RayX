use super::error::{ConfigurationError, OptionsFinalizeError};
use super::options::OptionsPlans;
use crate::options::OptionsRegistry;

/// Collects options plans and builds them into an [`OptionsRegistry`].
#[derive(Default)]
pub struct ConfigurationBuilder {
    options: OptionsPlans,
}

/// The result of [`ConfigurationBuilder::build_sync`]. RayX's result also holds
/// the configuration root built from sources, which the core does not have.
#[derive(Debug)]
pub struct ConfigurationBuildResult {
    pub options: OptionsRegistry,
}

impl ConfigurationBuilder {
    /// Registers `T::default()` options for `section`.
    pub fn options<T>(&mut self, section: impl Into<String>) -> &mut Self
    where
        T: Default + Send + Sync + 'static,
    {
        self.configure_options(section, T::default, |_| {}, |_| Ok(()));
        self
    }

    /// Registers layered options: the first plan for `T` supplies the base and the
    /// finalizer; later plans for the same `T` add configurators. Plans for one
    /// type under different sections fail the build.
    pub fn configure_options<T>(
        &mut self,
        section: impl Into<String>,
        base_factory: impl FnOnce() -> T + 'static,
        configure: impl FnOnce(&mut T) + 'static,
        finalize: impl FnOnce(&mut T) -> Result<(), OptionsFinalizeError> + 'static,
    ) -> &mut Self
    where
        T: Send + Sync + 'static,
    {
        self.options
            .configure(section.into(), base_factory, configure, finalize);
        self
    }

    /// Registers an exact options value; it replaces any earlier plan for `T`,
    /// and later layered plans for `T` are ignored.
    pub fn options_value<T>(&mut self, value: T) -> &mut Self
    where
        T: Send + Sync + 'static,
    {
        self.options.exact(value);
        self
    }

    pub fn build_options_sync(self) -> Result<OptionsRegistry, ConfigurationError> {
        self.build_sync().map(|result| result.options)
    }

    pub async fn build_options_async(self) -> Result<OptionsRegistry, ConfigurationError> {
        self.build_async().await.map(|result| result.options)
    }

    pub fn build_sync(self) -> Result<ConfigurationBuildResult, ConfigurationError> {
        Ok(ConfigurationBuildResult {
            options: self.options.materialize()?,
        })
    }

    /// The same as [`Self::build_sync`]; kept for RayX API parity (RayX needs it
    /// for async sources).
    pub async fn build_async(self) -> Result<ConfigurationBuildResult, ConfigurationError> {
        self.build_sync()
    }
}

impl std::fmt::Debug for ConfigurationBuilder {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ConfigurationBuilder")
            .field("options", &self.options.len())
            .finish()
    }
}
