//! TSX listeners of RayX's services: each one is stopped on its own by the [`Stop`] its `on…`
//! member returns (a function TS calls, which also runs when the reactive owner of the call
//! goes), and every listener an object registered stops when the object is disposed.
//!
//! A service's listeners must be `Send`, a TSX function is not, and both live on the engine's
//! thread: the function and the service's subscription are kept in this thread's table by id,
//! and what the service calls posts to the function found under that id.

use std::any::Any;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;

use nsh_core::element::CallbackArgs;
use nsh_core::prelude::*;

/// A kept listener: its TSX function and the service's subscription.
struct Kept {
    callback: Box<dyn Any>,
    _subscription: Option<Box<dyn Any>>,
}

thread_local! {
    static KEPT: RefCell<HashMap<u64, Kept>> = RefCell::new(HashMap::new());
    static NEXT_ID: Cell<u64> = const { Cell::new(0) };
}

/// Removes listener `id`; its subscription and function go after the table is released (their
/// drop may reach it again).
fn remove(id: u64) {
    let kept = KEPT.with_borrow_mut(|kept| kept.remove(&id));
    drop(kept);
}

/// The listeners an object registered: stopped when it goes.
#[derive(Default)]
pub(crate) struct Listeners(Vec<u64>);

impl Listeners {
    /// Keeps `listener` and subscribes it with `subscribe`, which gets the function the service
    /// calls (it posts to `listener` on the engine's thread) and gives the service's
    /// subscription, dropped when the listener stops. The [`Stop`] stops this listener alone.
    pub(crate) fn listen<A, S>(
        &mut self,
        listener: Callback<A>,
        subscribe: impl FnOnce(Box<dyn Fn(A) + Send + Sync>) -> Result<S, String>,
    ) -> Result<Stop, String>
    where
        A: CallbackArgs + 'static,
        S: 'static,
    {
        let id = NEXT_ID.with(|next| {
            let id = next.get();
            next.set(id + 1);
            id
        });
        KEPT.with_borrow_mut(|kept| {
            kept.insert(
                id,
                Kept {
                    callback: Box::new(listener),
                    _subscription: None,
                },
            )
        });
        let post = Box::new(move |value: A| {
            KEPT.with_borrow(|kept| {
                if let Some(listener) = kept
                    .get(&id)
                    .and_then(|kept| kept.callback.downcast_ref::<Callback<A>>())
                {
                    listener.post(value);
                }
            });
        });
        let subscription = match subscribe(post) {
            Ok(subscription) => subscription,
            Err(error) => {
                remove(id);
                return Err(error);
            }
        };
        KEPT.with_borrow_mut(|kept| {
            if let Some(kept) = kept.get_mut(&id) {
                kept._subscription = Some(Box::new(subscription));
            }
        });
        self.0
            .retain(|id| KEPT.with_borrow(|kept| kept.contains_key(id)));
        self.0.push(id);
        Ok(Stop::new(move || remove(id)))
    }
}

impl Drop for Listeners {
    fn drop(&mut self) {
        for id in self.0.drain(..) {
            remove(id);
        }
    }
}
