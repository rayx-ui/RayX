//! [`tr!`](crate::tr): the app's messages in a Rust component, following its language.

use futures::StreamExt as _;
use futures::channel::mpsc::unbounded;
use nsh_core::prelude::{Owner, global, spawn, try_global};
use rayx_core::ServiceProvider;
use rayx_runtime::services::i18n::{Args, I18nService};

/// Moves on with every change of the surface's i18n service, so reading it subscribes to them.
#[derive(Clone, Copy, Default)]
struct I18nRevision(u64);

/// Moves the surface's [`I18nRevision`] on with every change of the root's [`I18nService`]
/// (language, time zone, messages), from any thread, for the surface's life. The app's startup
/// calls it once the services are ready, where the engine runs Rust code.
pub(crate) fn follow_i18n_changes(root: &ServiceProvider) {
    let Some(i18n) = root.service::<I18nService>() else {
        return;
    };
    let revision = global::<I18nRevision>();
    let (send, mut changes) = unbounded::<()>();
    let subscription = i18n.subscribe(move |_| {
        let _ = send.unbounded_send(());
    });
    Owner::detached().run(move || {
        drop(spawn(async move {
            let _subscription = subscription;
            while changes.next().await.is_some() {
                revision.update(|revision| revision.0 += 1);
            }
        }));
    });
}

/// The message `id` in the app's language, formatted with `args` by the surface's
/// [`I18nService`]: what [`tr!`](crate::tr) calls.
///
/// Reading it subscribes, as reading a signal does: in a `text(move || …)` or `dynamic`, the
/// text follows the app's start and every change of the language, time zone or messages. Until
/// the app is ready, and in an app without the i18n service or outside an engine, it is `id`.
pub fn translate(id: &str, args: &Args) -> String {
    if let Some(revision) = try_global::<I18nRevision>() {
        revision.with(|_| ());
    }
    crate::surface::tracked_root()
        .and_then(|root| root.service::<I18nService>())
        .map_or_else(|| id.to_owned(), |i18n| i18n.format(id, args))
}

/// The message `id` in the app's language: `tr!(id, name = value, …)`, in a Rust component on
/// the engine's thread.
///
/// The arguments keep their types, so a number stays a number for `.match` and `:number`; a
/// name is an identifier (a raw one without its `r#`) or, for a variable name Rust cannot spell,
/// a string literal (`"user-id" = 7`), and a lone identifier passes the variable of that name, as
/// `format!` does. Reading it subscribes (see [`translate`]), so the text follows the app's
/// language:
///
/// ```no_run
/// use nsh_core::prelude::*;
/// use rayx_app::tr;
///
/// fn cart_summary(count: usize) -> impl IntoElement {
///     let name = "Ana";
///     text(move || tr!("cart.summary", name, count))
/// }
/// ```
///
/// It is named `tr!`, not `t!`, so a glob import of NSH's prelude (whose `t!` reads NSH's own
/// rust-i18n translations) and RayX's keeps both.
#[macro_export]
macro_rules! tr {
    ($id:expr $(,)?) => {
        $crate::translate($id, &$crate::rayx_runtime::services::i18n::Args::new())
    };
    ($id:expr, $($args:tt)+) => {{
        let mut args = $crate::rayx_runtime::services::i18n::Args::new();
        $crate::__tr_args!(args; $($args)+);
        $crate::translate($id, &args)
    }};
}

#[doc(hidden)]
#[macro_export]
macro_rules! __tr_args {
    ($args:ident;) => {};
    ($args:ident; $name:ident = $value:expr $(, $($rest:tt)*)?) => {
        $args.set($crate::__tr_name!($name), $value);
        $crate::__tr_args!($args; $($($rest)*)?);
    };
    ($args:ident; $name:literal = $value:expr $(, $($rest:tt)*)?) => {
        $args.set($name, $value);
        $crate::__tr_args!($args; $($($rest)*)?);
    };
    ($args:ident; $name:ident $(, $($rest:tt)*)?) => {
        $args.set($crate::__tr_name!($name), $name);
        $crate::__tr_args!($args; $($($rest)*)?);
    };
}

/// An argument's name as `stringify!` spells an identifier, without a raw identifier's `r#`.
#[doc(hidden)]
#[macro_export]
macro_rules! __tr_name {
    ($name:ident) => {{
        let name: &'static str = ::core::stringify!($name);
        name.strip_prefix("r#").unwrap_or(name)
    }};
}
