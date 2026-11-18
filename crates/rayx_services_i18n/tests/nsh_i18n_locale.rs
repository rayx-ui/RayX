//! The app's language choice reaches NSH: a language the app picks with the i18n service is
//! NSH's `locale()` (and so JS's `Intl` and NSH's own translations); once the app follows the
//! platform again, NSH shows the device's language again.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::{cell::RefCell, time::Duration};

use crossbeam_channel::{Receiver, Sender};
use futures::executor::block_on;
use nsh_core::{ApproxTextEngine, MountBatch, MountItem, prelude::*};
use nsh_engine::{App, EngineConfig, LocalEngine, MountSink};
use rayx_app::services::i18n::{I18nService, MessageFormatter};
use rayx_app_nsh::{
    AppState, RayXAppBuilder, ServiceProvider, StartAppBuilderExt as _, app_services, app_state,
};
use rayx_services_i18n::I18nAppBuilderExt as _;

mod messages;

thread_local! {
    static ROOT: RefCell<Option<ServiceProvider>> = const { RefCell::new(None) };
}

fn build_app(builder: RayXAppBuilder) -> RayXAppBuilder {
    messages::add_messages(
        builder,
        &[
            ("locales/en.json", r#"{ "hello": "Hello" }"#),
            ("locales/hr.json", r#"{ "hello": "Bok" }"#),
        ],
    )
    .add_i18n(|_| {})
}

/// Starts the app and shows NSH's locale once it is ready.
#[component]
fn Root() -> impl IntoElement {
    ::rayx_app::testing::add_host_fakes(build_app(RayXAppBuilder::new())).start();
    dynamic(move || match app_state() {
        AppState::Ready => {
            ROOT.with_borrow_mut(|root| *root = Some(app_services().unwrap()));
            text(move || format!("nsh {}", locale())).into_element()
        }
        AppState::Failed(message) => text(format!("failed: {message}")).into_element(),
        _ => text("starting").into_element(),
    })
}

struct ChannelSink(Sender<MountBatch>);

impl MountSink for ChannelSink {
    fn submit(&self, batch: MountBatch) {
        let _ = self.0.send(batch);
    }
}

/// Pumps until `shown` is the last text shown.
fn pump_until(engine: &mut LocalEngine<ApproxTextEngine>, rx: &Receiver<MountBatch>, shown: &str) {
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    let mut last = String::new();
    while std::time::Instant::now() < deadline {
        engine.pump();
        while let Ok(batch) = rx.recv_timeout(Duration::from_millis(5)) {
            for item in &batch.items {
                if let MountItem::Text { text, .. } = item {
                    last = text.text.to_string();
                }
            }
        }
        if last == shown {
            return;
        }
    }
    panic!("`{shown}` never showed; last: `{last}`");
}

#[test]
fn the_apps_language_choice_reaches_nsh_and_lets_go_of_it() {
    let (tx, rx) = crossbeam_channel::unbounded();
    let mut engine = LocalEngine::start(
        EngineConfig::new(App::Rust(nsh_core::element::Root::of::<Root>())),
        ApproxTextEngine,
        ChannelSink(tx),
    )
    .unwrap();
    // The app follows the device's languages, and so does NSH.
    pump_until(&mut engine, &rx, "nsh en-US");
    let root = ROOT.with_borrow(|root| root.clone()).unwrap();
    let i18n = root.required_service::<dyn I18nService>().unwrap();
    let formatter = root.required_service::<dyn MessageFormatter>().unwrap();
    assert_eq!(formatter.t("hello"), "Hello");
    assert_eq!(i18n.chosen_language(), None);

    // A language the app picks is NSH's too.
    block_on(i18n.set_language("hr".into())).unwrap();
    pump_until(&mut engine, &rx, "nsh hr");
    assert_eq!(formatter.t("hello"), "Bok");
    assert_eq!(i18n.chosen_language().as_deref(), Some("hr"));

    // Following the platform again lets NSH follow the device again.
    block_on(i18n.follow_platform()).unwrap();
    pump_until(&mut engine, &rx, "nsh en-US");
    assert_eq!(formatter.t("hello"), "Hello");
}
