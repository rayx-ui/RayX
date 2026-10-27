use super::SelectionService;
use crate::services::navigation::NavigationService;
use rayx_core::AppBuilder;

/// Why [`SelectionAppBuilderExt::add_selection`] could not register the service.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum SelectionStartupError {
    /// The selection follows the navigation, which is not registered.
    #[error("NavigationService must be registered before SelectionService")]
    MissingNavigation,
}

/// Registers the [`SelectionService`] on an app builder.
pub trait SelectionAppBuilderExt: AppBuilder {
    /// Registers the app's [`SelectionService`] (a startup singleton, cleared on every route
    /// change) unless one is registered. The [`NavigationService`] must be registered first.
    fn add_selection(mut self) -> Result<Self, SelectionStartupError> {
        if !self.services_mut().contains_service::<NavigationService>() {
            return Err(SelectionStartupError::MissingNavigation);
        }
        self.services_mut()
            .try_add_app_service_factory::<SelectionService>(|services| {
                let navigation = match services.required_service::<NavigationService>() {
                    Ok(navigation) => navigation,
                    Err(error) => panic!(
                        "NavigationService registration was validated before SelectionService: {error}"
                    ),
                };
                SelectionService::new(navigation.as_ref().clone())
            });
        Ok(self)
    }

    /// Registers `service` as the app's [`SelectionService`], replacing any other.
    fn with_selection_service(mut self, service: SelectionService) -> Self {
        self.services_mut().replace_app_service(service);
        self
    }
}

impl<T> SelectionAppBuilderExt for T where T: AppBuilder {}
