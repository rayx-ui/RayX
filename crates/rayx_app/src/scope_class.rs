//! The surface's services and a screen's scope, for TSX.

use std::sync::Arc;

use dxn_reactive::{ActivatableViewModel, ActivationToken};
use nsh_core::ui::prelude::*;
use rayx_app_di::ServiceProvider;

use crate::{AppError, start_app};

/// The app on this surface, for TSX: `await AppServices.load()` once, at the app's start,
/// then `app.scope()` for each screen.
///
/// `load` starts the app ([`start_app`]): it builds the root provider and awaits every startup
/// service's async initialization, each within its timeout, and rejects with the services that
/// failed or timed out. The app is ready once it resolves. It is a class's `async` member
/// because those run as the core's tasks, where NSH's timers and the surface's slot are reachable
/// (a class's synchronous calls run outside the reactive runtime). Everything after it is
/// synchronous.
#[class]
pub struct AppServices {
    provider: ServiceProvider,
}

#[class]
impl AppServices {
    /// Starts the app on this surface (once; later calls resolve with the same root).
    pub async fn load() -> Result<Instance<AppServices>, AppError> {
        Ok(Instance::new(AppServices {
            provider: start_app().await?,
        }))
    }

    /// A new scope for a screen: make it when the screen mounts, dispose it in its cleanup.
    pub fn scope(&self) -> Instance<ServiceScope> {
        Instance::new(ServiceScope::of(self.provider.create_scope()))
    }
}

impl AppServices {
    /// The root provider, for Rust bridges that take an `Instance<AppServices>`.
    pub fn services(&self) -> &ServiceProvider {
        &self.provider
    }
}

/// A screen's scope, for TSX: made by `app.scope()` (or `parent.child()` for a screen inside
/// another) when the screen mounts and disposed in its cleanup (`scope.dispose()`), which drops
/// its scoped services and deactivates the view models it activated.
///
/// Rust bridges take it as an `Instance<ServiceScope>` and resolve in it:
/// `scope.with_mut(|scope| scope.view_model::<dyn MyViewModel>())`.
#[class]
pub struct ServiceScope {
    provider: ServiceProvider,
    /// Activations held until the scope is disposed.
    activations: Vec<ActivationToken>,
}

#[class]
impl ServiceScope {
    /// A scope inside this one: it shares the root's singletons and has scoped services of its
    /// own.
    pub fn child(&self) -> Instance<ServiceScope> {
        Instance::new(Self::of(self.provider.create_scope()))
    }
}

impl ServiceScope {
    fn of(provider: ServiceProvider) -> Self {
        Self {
            provider,
            activations: Vec::new(),
        }
    }

    /// The scope's provider.
    pub fn services(&self) -> &ServiceProvider {
        &self.provider
    }

    /// `T` resolved in this scope and activated until the scope is disposed.
    pub fn view_model<T>(&mut self) -> Result<Arc<T>, AppError>
    where
        T: ?Sized + ActivatableViewModel + Send + Sync + 'static,
    {
        let view_model = self.provider.required_service::<T>()?;
        self.activations.push(view_model.activator().activate()?);
        Ok(view_model)
    }
}
