use super::SelectionService;
use crate::services::navigation::NavigationService;
use rayx_core::AppBuilder;

/// Registers the [`SelectionService`] on an app builder.
pub trait SelectionAppBuilderExt: AppBuilder {
    /// Registers the app's [`SelectionService`] (a startup singleton, cleared on every route
    /// change) unless one is registered. It follows the app's [`NavigationService`], registered
    /// before or after it; without one, startup fails.
    fn add_selection(mut self) -> Self {
        self.services_mut()
            .try_add_app_service_factory::<SelectionService>(|services| {
                match services.service::<NavigationService>() {
                    Some(navigation) => SelectionService::new(navigation.as_ref().clone()),
                    None => SelectionService::without_navigation(),
                }
            });
        self
    }

    /// Registers `service` as the app's [`SelectionService`], replacing any other.
    fn with_selection_service(mut self, service: SelectionService) -> Self {
        self.services_mut().replace_app_service(service);
        self
    }
}

impl<T> SelectionAppBuilderExt for T where T: AppBuilder {}
