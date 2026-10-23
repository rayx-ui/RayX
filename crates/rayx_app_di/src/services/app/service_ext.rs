use super::AppBuilder;
use crate::di::{
    DiReady, IntoServiceArc, IntoSingletonService, ServiceProvider, StartupInitializable,
};
use std::sync::Arc;

pub trait ServiceAppBuilderExt: AppBuilder {
    /// The init timeout of startup services that set none of their own.
    fn default_initialization_timeout(mut self, timeout: std::time::Duration) -> Self {
        self.services_mut()
            .set_default_initialization_timeout(timeout);
        self
    }

    fn add_singleton<T>(mut self, service: impl IntoSingletonService<T>) -> Self
    where
        T: DiReady + ?Sized,
    {
        self.services_mut().add_singleton::<T>(service);
        self
    }

    fn add_singleton_as<T>(mut self, service: impl IntoServiceArc<T>) -> Self
    where
        T: DiReady + ?Sized,
    {
        self.services_mut().add_singleton_as::<T>(service);
        self
    }

    fn add_scoped<T>(
        mut self,
        factory: impl Fn(&ServiceProvider) -> T + Send + Sync + 'static,
    ) -> Self
    where
        T: DiReady,
    {
        self.services_mut().add_scoped::<T>(factory);
        self
    }

    fn add_scoped_as<T>(
        mut self,
        factory: impl Fn(&ServiceProvider) -> Arc<T> + Send + Sync + 'static,
    ) -> Self
    where
        T: DiReady + ?Sized,
    {
        self.services_mut().add_scoped_as::<T>(factory);
        self
    }

    fn add_transient<T>(
        mut self,
        factory: impl Fn(&ServiceProvider) -> T + Send + Sync + 'static,
    ) -> Self
    where
        T: DiReady,
    {
        self.services_mut().add_transient::<T>(factory);
        self
    }

    fn add_transient_as<T>(
        mut self,
        factory: impl Fn(&ServiceProvider) -> Arc<T> + Send + Sync + 'static,
    ) -> Self
    where
        T: DiReady + ?Sized,
    {
        self.services_mut().add_transient_as::<T>(factory);
        self
    }

    fn add_startup_singleton<T>(mut self, service: impl IntoSingletonService<T>) -> Self
    where
        T: StartupInitializable + ?Sized,
    {
        self.services_mut().add_startup_singleton::<T>(service);
        self
    }

    fn add_startup_singleton_as<T>(mut self, service: impl IntoServiceArc<T>) -> Self
    where
        T: StartupInitializable + ?Sized,
    {
        self.services_mut().add_startup_singleton_as::<T>(service);
        self
    }

    fn add_startup_scoped<T>(
        mut self,
        factory: impl Fn(&ServiceProvider) -> T + Send + Sync + 'static,
    ) -> Self
    where
        T: StartupInitializable,
    {
        self.services_mut().add_startup_scoped::<T>(factory);
        self
    }

    fn add_startup_scoped_as<T>(
        mut self,
        factory: impl Fn(&ServiceProvider) -> Arc<T> + Send + Sync + 'static,
    ) -> Self
    where
        T: StartupInitializable + ?Sized,
    {
        self.services_mut().add_startup_scoped_as::<T>(factory);
        self
    }

    fn add_startup_transient<T>(
        mut self,
        factory: impl Fn(&ServiceProvider) -> T + Send + Sync + 'static,
    ) -> Self
    where
        T: StartupInitializable,
    {
        self.services_mut().add_startup_transient::<T>(factory);
        self
    }

    fn add_startup_transient_as<T>(
        mut self,
        factory: impl Fn(&ServiceProvider) -> Arc<T> + Send + Sync + 'static,
    ) -> Self
    where
        T: StartupInitializable + ?Sized,
    {
        self.services_mut().add_startup_transient_as::<T>(factory);
        self
    }
}

impl<T> ServiceAppBuilderExt for T where T: AppBuilder {}
