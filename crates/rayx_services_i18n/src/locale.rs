//! The app's language choice in NSH: while the app shows a language it picked
//! ([`I18nService::set_language`]), NSH shows it too ([`set_locale`]: its `locale()`, JS's
//! `Intl` and its own translations); once the app follows the platform's languages again, so
//! does NSH. While the app follows the platform, RayX leaves NSH's locale alone, so a change
//! of the device's languages still reaches both.

use std::sync::Arc;

use futures::StreamExt as _;
use futures::channel::mpsc::unbounded;
use nsh_core::platform::Locale;
use nsh_core::prelude::{Owner, set_locale, spawn, try_global};
use rayx_app::services::i18n::I18nService;

/// Keeps NSH's locale on the language `i18n` picked, for the surface's life. The formatter's
/// startup calls it, where the engine runs Rust code; outside an engine it does nothing.
pub(crate) fn keep_nsh_locale(i18n: &Arc<dyn I18nService>) {
    if !in_engine() {
        return;
    }
    let (send, mut changes) = unbounded::<Option<String>>();
    // A language changed on any thread reaches NSH on the engine's.
    let subscription = i18n.i18n().subscribe(move |state| {
        let _ = send.unbounded_send(state.chosen_language.clone());
    });
    let initial = i18n.chosen_language();
    Owner::detached().run(move || {
        drop(spawn(async move {
            let _subscription = subscription;
            // NSH follows the device until told otherwise.
            let mut shown: Option<String> = None;
            show(&mut shown, initial);
            while let Some(chosen) = changes.next().await {
                show(&mut shown, chosen);
            }
        }));
    });
}

/// Marks the surfaces whose NSH locale follows the app's choice.
#[derive(Default)]
struct KeptLocale;

/// Whether this runs where the engine runs Rust code (else there is no NSH to tell).
fn in_engine() -> bool {
    try_global::<KeptLocale>().is_some()
}

/// Tells NSH to show `chosen` (`None`: the device's languages), unless it shows it already.
fn show(shown: &mut Option<String>, chosen: Option<String>) {
    if *shown == chosen {
        return;
    }
    set_locale(chosen.as_deref().and_then(Locale::parse));
    *shown = chosen;
}
