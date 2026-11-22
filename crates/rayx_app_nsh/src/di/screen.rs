//! A screen's scope, from its mount to its cleanup.

use std::{cell::RefCell, rc::Rc, sync::Arc};

use dxn_reactive::ActivatableViewModel;
use nsh_core::prelude::{Instance, cleanup, provide_context, use_context};
use rayx_app::ServiceProvider;

use crate::{AppError, app_services};

/// The scope of the screen a component is on: provided by [`use_screen_scope`] (or
/// [`provide_services`]) to what the screen renders, emptied when the screen cleans up.
#[derive(Clone)]
pub struct ScreenScope(Rc<Screen>);

/// What a [`ScreenScope`] holds: its provider while the screen is mounted, and the one TS
/// `ServiceProvider` object TSX gets for it.
struct Screen {
    provider: RefCell<Option<ServiceProvider>>,
    ts: RefCell<Option<Instance<crate::ts::ServiceProvider>>>,
}

impl ScreenScope {
    fn new(provider: ServiceProvider, ts: Option<Instance<crate::ts::ServiceProvider>>) -> Self {
        Self(Rc::new(Screen {
            provider: RefCell::new(Some(provider)),
            ts: RefCell::new(ts),
        }))
    }

    /// The scope's provider, while the screen is mounted.
    pub fn services(&self) -> Option<ServiceProvider> {
        self.0.provider.borrow().clone()
    }

    /// Whether the screen is still mounted.
    pub fn is_live(&self) -> bool {
        self.0.provider.borrow().is_some()
    }

    /// The scope's one TS `ServiceProvider` object (made the first time), while the screen is
    /// mounted: what `useServices()` gives every TSX component under it.
    pub(crate) fn ts_services(&self) -> Option<Instance<crate::ts::ServiceProvider>> {
        let provider = self.services()?;
        let mut ts = self.0.ts.borrow_mut();
        if let Some(kept) = ts.as_ref().filter(|kept| !kept.is_disposed()) {
            return Some(kept.clone());
        }
        let made = Instance::new(crate::ts::ServiceProvider::of(provider));
        *ts = Some(made.clone());
        Some(made)
    }

    /// The screen goes: its provider is dropped, and its TS object disposed (which deactivates
    /// the view models activated through it).
    fn close(&self) {
        let provider = self.0.provider.borrow_mut().take();
        let ts = self.0.ts.borrow_mut().take();
        drop(provider);
        if let Some(ts) = ts {
            ts.dispose();
        }
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

/// The nearest enclosing screen, while it is mounted.
pub(crate) fn screen() -> Option<ScreenScope> {
    use_context::<ScreenScope>().filter(ScreenScope::is_live)
}

/// Makes this component a screen: a new scope, a child of the enclosing screen's (or of the
/// surface root), provided to what it renders. The component's cleanup drops the scope, and with
/// it the scoped services nothing else holds. Call it in a component's body.
pub fn use_screen_scope() -> Result<ScreenScope, AppError> {
    let scope = ScreenScope::new(services()?.create_scope(), None);
    provide_context(scope.clone());
    let closed = scope.clone();
    cleanup(move || closed.close());
    Ok(scope)
}

/// Provides `provider`, made elsewhere (a scope a test or a design tool made, a provider
/// another surface built), to what this component renders, as a screen's scope is provided:
/// [`services`] under it returns it. The component's cleanup does not dispose it; whoever made
/// it does. Call it in a component's body.
pub fn provide_services(provider: ServiceProvider) -> ScreenScope {
    let scope = ScreenScope::new(provider, None);
    provide_context(scope.clone());
    scope
}

/// [`provide_services`] for TSX's `provideServices(provider)`: the TS object TSX passed is the
/// one `useServices()` gives under it.
pub(crate) fn provide_ts_services(provider: Instance<crate::ts::ServiceProvider>) -> ScreenScope {
    let services = provider.with(|provider| provider.services().clone());
    let scope = ScreenScope::new(services, Some(provider));
    provide_context(scope.clone());
    scope
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
