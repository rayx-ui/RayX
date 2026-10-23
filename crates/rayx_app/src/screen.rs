//! A screen's scope, from its mount to its cleanup.

use std::{cell::RefCell, rc::Rc, sync::Arc};

use dxn_reactive::ActivatableViewModel;
use nsh_core::ui::{cleanup, provide_context, use_context};
use rayx_app_di::ServiceProvider;

use crate::{AppError, app_services};

/// The scope of the screen a component is on: provided by [`use_screen_scope`] to what the
/// screen renders, emptied when the screen cleans up.
#[derive(Clone)]
pub struct ScreenScope(Rc<RefCell<Option<ServiceProvider>>>);

impl ScreenScope {
    /// The scope's provider, while the screen is mounted.
    pub fn services(&self) -> Option<ServiceProvider> {
        self.0.borrow().clone()
    }

    /// Whether the screen is still mounted.
    pub fn is_live(&self) -> bool {
        self.0.borrow().is_some()
    }
}

/// The provider for what runs now: the nearest enclosing screen's scope, else the surface's
/// root.
pub fn services() -> Result<ServiceProvider, AppError> {
    match use_context::<ScreenScope>().and_then(|scope| scope.services()) {
        Some(provider) => Ok(provider),
        None => app_services(),
    }
}

/// Makes this component a screen: a new scope, a child of the enclosing screen's (or of the
/// surface root), provided to what it renders. The component's cleanup drops the scope, and with
/// it the scoped services nothing else holds. Call it in a component's body.
pub fn use_screen_scope() -> Result<ScreenScope, AppError> {
    let scope = ScreenScope(Rc::new(RefCell::new(Some(services()?.create_scope()))));
    provide_context(scope.clone());
    let dropped = scope.clone();
    cleanup(move || {
        dropped.0.borrow_mut().take();
    });
    Ok(scope)
}

/// `T` resolved from [`services`] (a screen's scope, for scoped view models), activated until
/// the component's cleanup. Call it in a component's body.
pub fn use_view_model<T>() -> Result<Arc<T>, AppError>
where
    T: ?Sized + ActivatableViewModel + Send + Sync + 'static,
{
    let view_model = services()?.required_service::<T>()?;
    let token = view_model.activator().activate()?;
    cleanup(move || drop(token));
    Ok(view_model)
}
