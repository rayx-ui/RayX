use super::access::{AppOptions, OptionsMonitor, OptionsSnapshot};
use super::pipeline::{OptionsFactory, Stage, Step};
use super::{DEFAULT_NAME, OptionsError};
use crate::di::ServiceProvider;
use serde::{Serialize, de::DeserializeOwned};
use std::marker::PhantomData;
use std::sync::Arc;

/// Registers how the options `T` named [`Self::name`] are made (.NET's `OptionsBuilder<T>`),
/// from [`ServiceProvider::add_options`] or [`ServiceProvider::add_named_options`].
///
/// Options are made from `T::default()`: the configure steps ([`Self::configure`],
/// [`Self::bind`], ...) run in registration order, then the post-configure steps, then every
/// validation, whose failures together fail the options ([`OptionsError::Validation`]). The
/// `*_with` steps take a service, resolved when the options' reader (the app's options, a
/// snapshot, the monitor) is made.
pub struct OptionsBuilder<'a, T> {
    services: &'a mut ServiceProvider,
    name: String,
    options: PhantomData<fn() -> T>,
}

impl<'a, T> OptionsBuilder<'a, T>
where
    T: Default + Send + Sync + 'static,
{
    fn new(services: &'a mut ServiceProvider, name: String) -> Self {
        services.add_options_step::<T>(None);
        Self {
            services,
            name,
            options: PhantomData,
        }
    }

    /// The options' name ([`DEFAULT_NAME`] for the unnamed ones).
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The provider the options are registered on.
    pub fn services(&mut self) -> &mut ServiceProvider {
        self.services
    }

    fn step(self, step: Step<T>) -> Self {
        self.services.add_options_step(Some(step));
        self
    }

    /// Binds the configuration's section at `path` (.NET's `BindConfiguration`): its values go
    /// over the options' ([`Configuration::bind`]) when the options are made, at this step's
    /// place among the configure steps. The configuration is the provider's
    /// [`Configuration`] service, which the app builder registers. An options monitor makes
    /// the options again when the configuration changes.
    ///
    /// [`Configuration`]: crate::configuration::Configuration
    /// [`Configuration::bind`]: crate::configuration::Configuration::bind
    pub fn bind(self, path: impl Into<String>) -> Self
    where
        T: Serialize + DeserializeOwned,
    {
        let step = Step::bind(self.name.clone(), path.into());
        self.step(step)
    }

    /// Configures the options.
    pub fn configure(self, configure: impl Fn(&mut T) + Send + Sync + 'static) -> Self {
        let step = Step::configure(Some(self.name.clone()), Stage::Configure, configure);
        self.step(step)
    }

    /// Configures the options with the service `D`.
    pub fn configure_with<D>(self, configure: impl Fn(&mut T, &D) + Send + Sync + 'static) -> Self
    where
        D: ?Sized + Send + Sync + 'static,
    {
        let step = Step::configure_with(Some(self.name.clone()), Stage::Configure, configure);
        self.step(step)
    }

    /// Configures the options after every configure step.
    pub fn post_configure(self, configure: impl Fn(&mut T) + Send + Sync + 'static) -> Self {
        let step = Step::configure(Some(self.name.clone()), Stage::PostConfigure, configure);
        self.step(step)
    }

    /// Configures the options with the service `D` after every configure step.
    pub fn post_configure_with<D>(
        self,
        configure: impl Fn(&mut T, &D) + Send + Sync + 'static,
    ) -> Self
    where
        D: ?Sized + Send + Sync + 'static,
    {
        let step = Step::configure_with(Some(self.name.clone()), Stage::PostConfigure, configure);
        self.step(step)
    }

    /// Validates the options: when `validate` returns `false`, they fail with
    /// `failure_message`.
    pub fn validate(
        self,
        validate: impl Fn(&T) -> bool + Send + Sync + 'static,
        failure_message: impl Into<String>,
    ) -> Self {
        let step = Step::validate(Some(self.name.clone()), validate, failure_message.into());
        self.step(step)
    }

    /// Validates the options with the service `D`.
    pub fn validate_with<D>(
        self,
        validate: impl Fn(&T, &D) -> bool + Send + Sync + 'static,
        failure_message: impl Into<String>,
    ) -> Self
    where
        D: ?Sized + Send + Sync + 'static,
    {
        let step = Step::validate_with(Some(self.name.clone()), validate, failure_message.into());
        self.step(step)
    }

    /// Makes the options when the app starts (.NET's `ValidateOnStart`), so that invalid
    /// options fail startup ([`ServiceProvider::validate_options_on_start`]) rather than their
    /// first use.
    pub fn validate_on_start(self) -> Self {
        let name = self.name.clone();
        self.services
            .options_registrations_mut()
            .validate_on_start::<T>(
                self.name.clone(),
                Arc::new(
                    move |services: &ServiceProvider| -> Result<(), OptionsError> {
                        services.options_monitor::<T>().get(&name).map(drop)
                    },
                ),
            );
        self
    }
}

impl<T> std::fmt::Debug for OptionsBuilder<'_, T> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OptionsBuilder")
            .field("type", &std::any::type_name::<T>())
            .field("name", &self.name)
            .finish()
    }
}

/// Options registration (.NET's `services.AddOptions<T>()`, `services.Configure<T>(..)`, ...).
impl ServiceProvider {
    /// Registers the unnamed options `T` and returns their builder.
    pub fn add_options<T>(&mut self) -> OptionsBuilder<'_, T>
    where
        T: Default + Send + Sync + 'static,
    {
        OptionsBuilder::new(self, DEFAULT_NAME.to_owned())
    }

    /// Registers the options `T` named `name` and returns their builder.
    pub fn add_named_options<T>(&mut self, name: impl Into<String>) -> OptionsBuilder<'_, T>
    where
        T: Default + Send + Sync + 'static,
    {
        OptionsBuilder::new(self, name.into())
    }

    /// Configures the unnamed options `T`.
    pub fn configure<T>(&mut self, configure: impl Fn(&mut T) + Send + Sync + 'static) -> &mut Self
    where
        T: Default + Send + Sync + 'static,
    {
        self.configure_named(DEFAULT_NAME, configure)
    }

    /// Configures the options `T` named `name`.
    pub fn configure_named<T>(
        &mut self,
        name: impl Into<String>,
        configure: impl Fn(&mut T) + Send + Sync + 'static,
    ) -> &mut Self
    where
        T: Default + Send + Sync + 'static,
    {
        self.add_options_step(Some(Step::configure(
            Some(name.into()),
            Stage::Configure,
            configure,
        )));
        self
    }

    /// Configures the options `T` of every name.
    pub fn configure_all<T>(
        &mut self,
        configure: impl Fn(&mut T) + Send + Sync + 'static,
    ) -> &mut Self
    where
        T: Default + Send + Sync + 'static,
    {
        self.add_options_step(Some(Step::configure(None, Stage::Configure, configure)));
        self
    }

    /// Configures the unnamed options `T` after every configure step.
    pub fn post_configure<T>(
        &mut self,
        configure: impl Fn(&mut T) + Send + Sync + 'static,
    ) -> &mut Self
    where
        T: Default + Send + Sync + 'static,
    {
        self.post_configure_named(DEFAULT_NAME, configure)
    }

    /// Configures the options `T` named `name` after every configure step.
    pub fn post_configure_named<T>(
        &mut self,
        name: impl Into<String>,
        configure: impl Fn(&mut T) + Send + Sync + 'static,
    ) -> &mut Self
    where
        T: Default + Send + Sync + 'static,
    {
        self.add_options_step(Some(Step::configure(
            Some(name.into()),
            Stage::PostConfigure,
            configure,
        )));
        self
    }

    /// Configures the options `T` of every name after every configure step.
    pub fn post_configure_all<T>(
        &mut self,
        configure: impl Fn(&mut T) + Send + Sync + 'static,
    ) -> &mut Self
    where
        T: Default + Send + Sync + 'static,
    {
        self.add_options_step(Some(Step::configure(None, Stage::PostConfigure, configure)));
        self
    }

    /// Adds a step to `T`'s pipeline; the first one registers `T`'s readers: the app's options
    /// and the monitor as singletons, the snapshot as a scoped service.
    fn add_options_step<T>(&mut self, step: Option<Step<T>>)
    where
        T: Default + Send + Sync + 'static,
    {
        if self.options_registrations_mut().add(step) {
            let services = self.service_registry_mut();
            services.insert_singleton_factory::<AppOptions<T>>(|services| {
                Arc::new(AppOptions(OptionsSnapshot::new(services.options_factory())))
            });
            services.insert_singleton_factory::<OptionsMonitor<T>>(|services| {
                Arc::new(OptionsMonitor::new(services.options_factory()))
            });
            services.insert_scoped::<OptionsSnapshot<T>>(|services| {
                Arc::new(OptionsSnapshot::new(services.options_factory()))
            });
        }
    }

    fn options_factory<T>(&self) -> OptionsFactory<T>
    where
        T: Default + Send + Sync + 'static,
    {
        OptionsFactory::new(
            self.options_registrations().pipeline::<T>().as_deref(),
            self,
        )
    }
}
