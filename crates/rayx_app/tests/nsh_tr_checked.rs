//! `checked::tr!`: `tr!` with the id and the argument names checked against the crate's messages
//! when it builds (`[package.metadata.dxn_i18n]` points this crate's at `tests/fixtures/locales`;
//! the default locale `en` and the default namespace `translation` are RayX's). This file builds
//! only if every call below passes the check; at run time it shows what `tr!` shows.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::{cell::RefCell, time::Duration};

use crossbeam_channel::{Receiver, Sender};
use nsh_core::{ApproxTextEngine, MountBatch, MountItem, prelude::*};
use nsh_engine::{App, EngineConfig, LocalEngine, MountSink};
use rayx_app::checked::tr;
use rayx_app::rayx_runtime::services::i18n::{I18nAppBuilderExt, I18nOptions, I18nService};
use rayx_app::{RayXAppBuilder, ServiceProvider, app_services, set_app};

thread_local! {
    static ROOT: RefCell<Option<ServiceProvider>> = const { RefCell::new(None) };
}

/// The service with the same messages the check reads.
fn build_app(builder: RayXAppBuilder) -> RayXAppBuilder {
    let mut options = I18nOptions::default();
    options.auto_load_assets(false);
    let service = I18nService::new(options).unwrap();
    let resources = [
        (
            "en",
            None,
            include_str!("fixtures/locales/en/translation.json"),
        ),
        (
            "en",
            Some("checkout"),
            include_str!("fixtures/locales/en/checkout.json"),
        ),
        (
            "hr",
            None,
            include_str!("fixtures/locales/hr/translation.json"),
        ),
    ];
    for (language, namespace, json) in resources {
        service.add_resource(language, namespace, json).unwrap();
    }
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
            tr!("checkout.title"),
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
fn checked_tr_shows_the_apps_messages_and_follows_its_language() {
    set_app(build_app);
    let (tx, rx) = crossbeam_channel::unbounded();
    let mut engine = LocalEngine::start(
        EngineConfig::new(App::Rust(nsh_core::element::Root::of::<Root>())),
        ApproxTextEngine,
        ChannelSink(tx),
    )
    .unwrap();
    pump_until(
        &mut engine,
        &rx,
        "Hello, Ana! | 1,200 items | User 7 is new. | Checkout",
    );
    let root = ROOT.with_borrow(|root| root.clone()).unwrap();
    let i18n = root.required_service::<I18nService>().unwrap();
    i18n.set_language("hr").unwrap();
    pump_until(
        &mut engine,
        &rx,
        "Bok, Ana! | 1.200 artikala | User 7 is new. | Checkout",
    );
}

#[test]
fn checked_tr_is_the_id_outside_an_engine() {
    assert_eq!(tr!("hello", name = "Ana"), "hello");
    assert_eq!(tr!("checkout.title"), "checkout.title");
}
