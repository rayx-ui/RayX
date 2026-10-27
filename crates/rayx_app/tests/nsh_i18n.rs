//! The i18n service for TSX through NSH's engine, headless: the `I18n` class formats messages,
//! changes the language and tells TSX about it.

// Loading the compiled TSX app takes `Bundle::bytecode`.
#![allow(unsafe_code, clippy::expect_used, clippy::unwrap_used)]

use std::time::Duration;

use crossbeam_channel::{Receiver, Sender};
use nsh_core::{ApproxTextEngine, MountBatch, MountItem, element::Registry};
use nsh_engine::{App, Bundle, Engine, EngineConfig, MountSink};
use rayx_app::rayx_runtime::services::i18n::{I18nAppBuilderExt, I18nOptions, I18nService};
use rayx_app::{AppServices, I18n, RayXAppBuilder, set_app};

struct ChannelSink(Sender<MountBatch>);

impl MountSink for ChannelSink {
    fn submit(&self, batch: MountBatch) {
        let _ = self.0.send(batch);
    }
}

fn build_app(builder: RayXAppBuilder) -> RayXAppBuilder {
    let mut options = I18nOptions::default();
    options.auto_load_assets(false).follow_platform(false);
    let service = I18nService::new(options).unwrap();
    service
        .add_resource(
            "en",
            None,
            r#"{ "hello": "Hello, {$name}!", "cart": ".input {$n :number}\n.match $n\none {{One item}}\n* {{{$n} items}}" }"#,
        )
        .unwrap();
    service
        .add_resource(
            "hr",
            None,
            r#"{ "cart": ".input {$n :number}\n.match $n\none {{{$n} artikl}}\nfew {{{$n} artikla}}\n* {{{$n} artikala}}" }"#,
        )
        .unwrap();
    builder.with_i18n_service(service)
}

const TSX: &str = r#"import { mount, signal, Text } from "nsh";
import { AppServices, I18n } from "test/app";

const shown = signal("waiting");
setTimeout(async () => {
  const lines = [];
  const app = await AppServices.load();
  const i18n = I18n.of(app);
  lines.push(i18n.t("hello", { name: "Ana" }));
  lines.push(i18n.t("cart", { n: 1 }));
  lines.push(i18n.has("cart") && !i18n.has("nope") ? "has" : "lacks");
  lines.push(i18n.languages().sort().join(","));
  i18n.onChange((locale) => {
    lines.push(`changed ${locale} ${i18n.direction()}`);
    lines.push(i18n.t("cart", { n: 3 }));
    shown.set(`done ${lines.join("|")}`);
  });
  i18n.setLanguage("hr");
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

/// The last text shown once one starts with `prefix` (or after 10 s).
fn shown(rx: &Receiver<MountBatch>, prefix: &str) -> String {
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    let mut last = String::new();
    while let Ok(batch) = rx.recv_deadline(deadline) {
        for item in &batch.items {
            if let MountItem::Text { text, .. } = item {
                last = text.text.clone();
            }
        }
        if last.starts_with(prefix) {
            break;
        }
    }
    last
}

#[test]
fn nsh_i18n_gives_tsx_messages_and_language_changes() {
    set_app(build_app);
    let mut set = Registry::ui_set("test/app");
    set.class::<AppServices>().class::<I18n>();
    let dir = std::env::temp_dir().join(format!("rayx-app-i18n-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let entry = dir.join("App.tsx");
    std::fs::write(&entry, TSX).unwrap();
    let ui_sets = vec![set];
    let bundle = nsh_bundler::bundle(&nsh_bundler::BundleOptions {
        entry,
        minify: false,
        split: false,
        ui_sets: ui_sets.iter().map(Into::into).collect(),
    })
    .unwrap();
    let described: Vec<_> = ui_sets.iter().map(Into::into).collect();
    let bytecode = nsh_js::compile_bytecode_with(&bundle.code, false, &described).unwrap();
    let (tx, rx) = crossbeam_channel::unbounded();
    // SAFETY: compiled just now by this runtime.
    let app = App::Js(unsafe { Bundle::bytecode(bytecode) });
    let config = EngineConfig {
        ui_sets,
        ..EngineConfig::new(app)
    };
    let _engine = Engine::start(config, || ApproxTextEngine, ChannelSink(tx)).unwrap();
    let last = shown(&rx, "done");
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(
        last,
        "done Hello, Ana!|One item|has|en,hr|changed hr ltr|3 artikla"
    );
}
