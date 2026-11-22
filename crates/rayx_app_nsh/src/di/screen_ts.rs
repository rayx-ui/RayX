//! TSX's screen hooks: `useServices`, `useScreenScope` and `provideServices`, the twins of
//! [`services`](super::screen::services), [`use_screen_scope`](super::screen::use_screen_scope)
//! and [`provide_services`](super::screen::provide_services).
//!
//! Each runs the Rust hook's own code in NSH's Rust graph, under the Rust owner NSH ties to the
//! calling TSX component's owner (`CallCx::with_owner`): what `useScreenScope` provides there
//! is what `useServices` (and a Rust component's `services()`) finds under it, and its cleanup
//! runs when that owner goes.

use nsh_core::prelude::*;

use crate::AppError;
use crate::di::provider_ts::{ServiceProvider, root_of};
use crate::di::screen::{provide_ts_services, screen, use_screen_scope};

/// What a hook called outside a component's body (no reactive owner owns the call) throws.
pub(crate) const NO_OWNER: &str = "a RayX hook is called in a component's body: this call has no \
     reactive owner (a timer's or an event handler's own code, a module's top level)";

/// The provider a TSX component's body sees: its screen's one TS object, else the surface
/// root's.
pub(crate) fn ts_services(sys: &System) -> Result<Instance<ServiceProvider>, AppError> {
    match screen().and_then(|screen| screen.ts_services()) {
        Some(services) => Ok(services),
        None => crate::app_services().map(|root| root_of(sys, root)),
    }
}

/// RayX's screen hooks for TSX.
#[module]
pub struct ScreenHooks;

#[module(flat)]
impl ScreenHooks {
    /// The `ServiceProvider` of the nearest enclosing screen (`useScreenScope`,
    /// `provideServices`), else the surface's root: the same object for every component under
    /// one screen. Call it in a component's body; it throws while RayX is starting or before it
    /// started, and outside a component's body.
    pub fn use_services(cx: CallCx, sys: System) -> Result<Instance<ServiceProvider>, String> {
        if !cx.has_owner() {
            return Err(NO_OWNER.into());
        }
        cx.with_owner(|| ts_services(&sys))
            .map_err(|error| error.to_string())
    }

    /// Makes the calling component a screen: a child scope of `useServices()`, provided to
    /// everything it renders (TSX and Rust components alike) and disposed when it cleans up,
    /// with the scoped services it resolved and the view models activated through it. Call it
    /// in a component's body.
    pub fn use_screen_scope(cx: CallCx) -> Result<Instance<ServiceProvider>, String> {
        if !cx.has_owner() {
            return Err(NO_OWNER.into());
        }
        cx.with_owner(|| {
            let scope = use_screen_scope()?;
            scope.ts_services().ok_or(AppError::NotStarted)
        })
        .map_err(|error| error.to_string())
    }

    /// Provides `provider`, made elsewhere, to everything the calling component renders, as a
    /// screen's scope is provided: `useServices()` under it gives this object. The component's
    /// cleanup does not dispose it. Call it in a component's body.
    pub fn provide_services(provider: Instance<ServiceProvider>, cx: CallCx) -> Result<(), String> {
        if !cx.has_owner() {
            return Err(NO_OWNER.into());
        }
        if provider.is_disposed() {
            return Err("ServiceProvider is disposed".into());
        }
        cx.with_owner(|| {
            provide_ts_services(provider);
        });
        Ok(())
    }
}
