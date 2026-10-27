use super::OperationService;
use rayx_core::AppBuilder;

/// Registers the [`OperationService`] on an app builder.
pub trait OperationsAppBuilderExt: AppBuilder {
    /// Registers the app's [`OperationService`], a startup singleton, unless one is registered.
    fn add_operations(mut self) -> Self {
        self.services_mut()
            .try_add_app_service_factory::<OperationService>(|_| OperationService::new());
        self
    }

    /// Registers `service` as the app's [`OperationService`], replacing any other.
    fn with_operation_service(mut self, service: OperationService) -> Self {
        self.services_mut().replace_app_service(service);
        self
    }
}

impl<T> OperationsAppBuilderExt for T where T: AppBuilder {}
