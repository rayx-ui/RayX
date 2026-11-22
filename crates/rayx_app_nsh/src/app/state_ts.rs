//! TSX's `appState()`: the twin of [`app_state`](super::surface::app_state), an NSH signal of
//! where the app is on the surface, written by RayX as the app starts.

use std::{cell::RefCell, rc::Rc};

use nsh_core::element::{JsHost, JsIn, JsOwned, Local, PropError, TsAlias, TsTypes, create_root};
use nsh_core::prelude::*;

use super::surface::{AppState, app_state};

/// `AppState` for TS: where the app is on the surface.
pub const APP_STATE: TsAlias = TsAlias {
    name: "AppState",
    ts: "{ state: \"notStarted\" | \"starting\" | \"ready\" } | { state: \"failed\"; message: string }",
    doc: "Where the app is on the surface (`appState()`): not started, starting, ready, or failed with \
          the startup error's message.",
};

/// [`AppState`] as TS reads it: `{ state, message? }`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppStateValue(AppState);

impl JsValue for AppStateValue {
    fn ts_type(_types: &mut TsTypes) -> String {
        "AppState".into()
    }

    fn from_js(value: JsIn<'_>) -> Result<Self, PropError> {
        Err(value.expected("no value: RayX writes the app's state"))
    }

    fn to_js<'a>(&self, host: &'a dyn JsHost) -> Result<JsOwned<'a>, PropError> {
        let object = JsOwned::new(host, host.new_object()?);
        let (state, message) = match &self.0 {
            AppState::NotStarted => ("notStarted", None),
            AppState::Starting => ("starting", None),
            AppState::Ready => ("ready", None),
            AppState::Failed(message) => ("failed", Some(message.as_str())),
        };
        // `set_field` takes the value.
        host.set_field(object.get().raw(), "state", host.new_string(state)?)?;
        if let Some(message) = message {
            host.set_field(object.get().raw(), "message", host.new_string(message)?)?;
        }
        Ok(object)
    }
}

/// The VM's one `appState()` signal, made by the first call, and shared with what writes it:
/// it goes with the VM, before the VM's values do (what writes it then finds none).
#[derive(Default)]
struct AppStateSignal(Rc<RefCell<Option<JsSignal<AppStateValue>>>>);

impl Local for AppStateSignal {
    fn close(&self) {
        let signal = self.0.borrow_mut().take();
        drop(signal);
    }
}

/// The app's state for TSX.
#[module]
pub struct AppStateHooks;

#[module(flat)]
impl AppStateHooks {
    /// Where the app is on this surface: an NSH signal (`appState().state`, tracked) that follows
    /// the app's start (`"notStarted"`, `"starting"`, `"ready"`, or `"failed"` with the startup
    /// error's `message`), as Rust's `app_state()` does; mount the components bound to view
    /// models once it is `"ready"`. Every call gives the same signal.
    pub fn app_state(cx: CallCx, sys: System) -> Result<JsSignal<AppStateValue>, String> {
        let slot = sys.locals.get::<AppStateSignal>();
        if let Some(signal) = slot.0.borrow().as_ref() {
            return Ok(signal.clone());
        }
        let signal = cx
            .signal(AppStateValue(AppState::NotStarted))
            .map_err(|error| error.0)?;
        *slot.0.borrow_mut() = Some(signal.clone());
        let kept = Rc::clone(&slot.0);
        cx.with_owner(|| {
            // Its own root, which nothing disposes: the signal follows the surface's state for
            // as long as the VM keeps it.
            let (_root, ()) = Owner::detached().run(|| {
                create_root(|| {
                    effect(move || {
                        let state = AppStateValue(app_state());
                        let signal = kept.borrow().clone();
                        if let Some(signal) = signal {
                            signal.set(state);
                        }
                    });
                })
            });
        });
        Ok(signal)
    }
}
