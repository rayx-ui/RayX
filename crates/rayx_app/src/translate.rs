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
/// The arguments are `dxn_i18n`'s [`args!`](rayx_runtime::services::i18n::args): they keep
/// their types, so a number stays a number for `.match` and `:number`; a name is an identifier
/// (a raw one without its `r#`) or, for a variable name Rust cannot spell, a string literal
/// (`"user-id" = 7`), and a lone identifier passes the variable of that name, as `format!` does.
/// Reading it subscribes (see [`translate`]), so the text follows the app's language:
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
/// rust-i18n translations) and RayX's keeps both. [`checked::tr!`](crate::checked::tr) is the
/// same with the id and the argument names checked against the app's messages when it builds.
#[macro_export]
macro_rules! tr {
    ($id:expr $(,)?) => {
        $crate::translate($id, &$crate::rayx_runtime::services::i18n::Args::new())
    };
    ($id:expr, $($args:tt)+) => {
        $crate::translate($id, &$crate::rayx_runtime::services::i18n::args!($($args)+))
    };
}

/// [`checked::tr!`](crate::checked::tr), under the name `#[macro_export]` gives it at the crate's
/// root.
#[doc(hidden)]
#[macro_export]
macro_rules! __checked_tr {
    ($id:expr $(,)?) => {
        $crate::__private::check!(
            $id, (),
            messages = "assets/locales", default_locale = "en", default_namespace = "translation"
            => $crate::tr!($id)
        )
    };
    ($id:expr, $($args:tt)+) => {
        $crate::__private::check!(
            $id, ($($args)+),
            messages = "assets/locales", default_locale = "en", default_namespace = "translation"
            => $crate::tr!($id, $($args)+)
        )
    };
}

/// [`tr!`](crate::tr) with the call checked when the app builds: a message id that is not in
/// the app's messages, an argument no translation of the message reads, or one a translation
/// needs and the call does not pass, fails the build instead of showing the id or a fallback.
///
/// ```ignore
/// use rayx::checked::tr;
///
/// // assets/locales/en.json: { "cart": { "items": ".input {$count :number} .match $count one {{One item}} * {{{$count} items}}" } }
/// text(move || tr!("cart.items", count));
/// text(move || tr!("cart.itmes", count)); // error: no message `cart.itmes`; did you mean `cart.items`?
/// text(move || tr!("cart.items"));        // error: message `cart.items` needs the argument `$count`
/// ```
///
/// It reads the messages of the crate being built from the asset catalog's `locales/` beside its
/// `Cargo.toml`, as the i18n service's defaults have them: `assets/locales/<language>.json` or
/// `assets/locales/<language>/<namespace>.json`, every id in `en`, the `translation` namespace's
/// ids unprefixed. An app whose `I18nOptions` say otherwise says the same to the check:
///
/// ```toml
/// [package.metadata.dxn_i18n]
/// messages = "assets/i18n"   # resources_path, under assets/
/// default-locale = "de"      # default_language
/// default-namespace = "app"  # default_namespace
/// ```
///
/// The id is a string literal; [`tr!`](crate::tr) takes one made at run time. A message file
/// that changes rebuilds the crate.
pub mod checked {
    #[doc(inline)]
    pub use crate::__checked_tr as tr;
}

/// What the macros use; not part of the API.
#[doc(hidden)]
pub mod __private {
    pub use dxn_i18n_macros::check;
}
