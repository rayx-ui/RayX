//! `tr!` in a Rust component: the message in the app's language, formatted with arguments that
//! keep their types; the text shows the id until the app is ready and follows every change of
//! the language or the messages.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::{cell::RefCell, time::Duration};

use crossbeam_channel::{Receiver, Sender};
use nsh_core::{ApproxTextEngine, MountBatch, MountItem, prelude::*};
use nsh_engine::{App, EngineConfig, LocalEngine, MountSink};
use rayx_app::rayx_runtime::services::i18n::{I18nAppBuilderExt, I18nOptions, I18nService};
use rayx_app::{RayXAppBuilder, ServiceProvider, app_services, set_app, tr};

thread_local! {
    static ROOT: RefCell<Option<ServiceProvider>> = const { RefCell::new(None) };
}

fn build_app(builder: RayXAppBuilder) -> RayXAppBuilder {
    let mut options = I18nOptions::default();
    options.auto_load_assets(false);
    let service = I18nService::new(options).unwrap();
    service
        .add_resource(
            "en",
            None,
            r#"{
              "hello": "Hello, {$name}!",
              "items": ".input {$count :number}\n.match $count\none {{One item}}\n* {{{$count} items}}",
              "profile": "User {$user-id} is {$type}."
            }"#,
        )
        .unwrap();
    service
        .add_resource(
            "hr",
            None,
            r#"{
              "hello": "Bok, {$name}!",
              "items": ".input {$count :number}\n.match $count\none {{{$count} artikl}}\nfew {{{$count} artikla}}\n* {{{$count} artikala}}"
            }"#,
        )
        .unwrap();
    builder.with_i18n_service(service)
}

#[component]
fn Root() -> impl IntoElement {
    rayx_app::start();
    let name = "Ana";
    let count = 1200;
    text(move || {
        if let Ok(root) = app_services() {
            ROOT.with_borrow_mut(|kept| *kept = Some(root));
        }
        format!(
            "{} | {} | {} | {}",
            tr!("hello", name),
            tr!("items", count),
            tr!("profile", "user-id" = 7, r#type = "new",),
            tr!("missing"),
        )
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
fn tr_shows_the_apps_messages_and_follows_its_language() {
    set_app(build_app);
    let (tx, rx) = crossbeam_channel::unbounded();
    let mut engine = LocalEngine::start(
        EngineConfig::new(App::Rust(nsh_core::element::Root::of::<Root>())),
        ApproxTextEngine,
        ChannelSink(tx),
    )
    .unwrap();
    // Once the app is ready, the text is its messages.
    pump_until(
        &mut engine,
        &rx,
        "Hello, Ana! | 1,200 items | User 7 is new. | missing",
    );
    let root = ROOT.with_borrow(|root| root.clone()).unwrap();
    let i18n = root.required_service::<I18nService>().unwrap();

    // A language change shows at once, with no state of the component's own.
    i18n.set_language("hr").unwrap();
    pump_until(
        &mut engine,
        &rx,
        "Bok, Ana! | 1.200 artikala | User 7 is new. | missing",
    );

    // So do messages added while the app runs, from another thread too.
    std::thread::spawn(move || {
        i18n.add_resource("hr", None, r#"{ "missing": "Pronađeno" }"#)
            .unwrap();
    })
    .join()
    .unwrap();
    pump_until(
        &mut engine,
        &rx,
        "Bok, Ana! | 1.200 artikala | User 7 is new. | Pronađeno",
    );
}

#[test]
fn tr_is_the_id_outside_an_engine() {
    assert_eq!(tr!("hello", name = "Ana"), "hello");
    assert_eq!(tr!(&format!("{}.{}", "cart", "title")), "cart.title");
}
