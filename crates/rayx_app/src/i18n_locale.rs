//! The app's language choice in NSH: while the app shows a language it picked
//! ([`I18nService::set_language`]), NSH shows it too ([`set_locale`]: its `locale()`, JS's
//! `Intl` and its own translations); once the app follows the platform's languages again, so
//! does NSH. While the app follows the platform, RayX leaves NSH's locale alone, so a change
//! of the device's languages still reaches both.

use futures::StreamExt as _;
use futures::channel::mpsc::unbounded;
use nsh_core::platform::Locale;
use nsh_core::prelude::{Owner, set_locale, spawn};
use rayx_core::ServiceProvider;
use rayx_runtime::services::i18n::I18nService;

/// Keeps NSH's locale on the language the root's [`I18nService`] picked, for the surface's
/// life. The app's startup calls it once the services are ready, where the engine runs Rust
/// code.
pub(crate) fn keep_nsh_locale(root: &ServiceProvider) {
    let Some(i18n) = root.service::<I18nService>() else {
        return;
    };
    let (send, mut changes) = unbounded::<Option<String>>();
    // A language changed on any thread reaches NSH on the engine's.
    let subscription = i18n.subscribe(move |event| {
        let chosen = event.chosen.then(|| event.locale.id.to_string());
        let _ = send.unbounded_send(chosen);
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

/// Tells NSH to show `chosen` (`None`: the device's languages), unless it shows it already.
fn show(shown: &mut Option<String>, chosen: Option<String>) {
    if *shown == chosen {
        return;
    }
    set_locale(chosen.as_deref().and_then(Locale::parse));
    *shown = chosen;
}
