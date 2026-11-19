//! The theme service for TSX through NSH's engine, headless: `builder.addTheme(..)` (the set's
//! class extension) registers RayX's service with its options, which the `ThemeService` class
//! reaches (the snapshot, picks and the color mode as promises, hearing changes); a TS
//! implementation of the contract registered under the class replaces RayX's, Rust uses it, and
//! what it throws reaches Rust as `ThemeError::Failed` and the class's promise as a rejection.
//! The class, its options, the contract and `addTheme` are declared. Under a dark scheme, a JS
//! app's first frame already shows `addTheme`'s pick (made in its call, before `mount`), and
//! without the service it shows the theme's default, `light`.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::Arc;

use nsh_core::element::Registry;
use nsh_core::prelude::*;
use rayx_app::services::theme::{ThemeError, ThemeService};
use rayx_app_nsh::testing;
use rayx_app_nsh::ts::ServiceProvider;

/// The set's theme document.
const THEME: &str = r#"{
  "tokens": { "shade": { "kind": "number", "value": 1 } },
  "modifiers": {
    "colorMode": { "default": "light", "contexts": { "light": {}, "dark": { "shade": 2 } } },
    "accent": { "default": "blue", "contexts": { "blue": {}, "cyan": {}, "sky": {} } }
  },
  "resolutionOrder": ["colorMode", "accent"]
}"#;

fn theme(services: &Instance<ServiceProvider>) -> Arc<dyn ThemeService> {
    services
        .with(|services| services.services().required_service::<dyn ThemeService>())
        .unwrap()
}

/// What the test reads of the registered theme service from Rust.
#[module]
struct Probe;

#[module]
impl Probe {
    /// The context `modifier` shows, by the snapshot.
    pub(crate) fn selected(services: Instance<ServiceProvider>, modifier: String) -> String {
        format!("{:?}", theme(&services).selected(&modifier))
    }

    /// Picks through the contract: `ok <changed>`, `failed <message>` or the error.
    pub(crate) async fn pick(
        services: Instance<ServiceProvider>,
        modifier: String,
        context: String,
    ) -> String {
        match theme(&services).set_context(modifier, context).await {
            Ok(changed) => format!("ok {changed}"),
            Err(ThemeError::Failed(failure)) => format!("failed {}", failure.message),
            Err(error) => format!("error {error}"),
        }
    }
}

fn set() -> Registry {
    testing::set(|set| {
        rayx_services_theme::register(set);
        set.module::<Probe>();
        set.theme_document(THEME);
    })
}

const RAYX: &str = r#"import { mount, signal, Text } from "nsh";
import { Probe, RayXAppBuilder, ThemeService } from "test/app";

const wait = () => new Promise((resolve) => setTimeout(resolve, 30));
const shows = (snapshot) =>
  `${snapshot.modifiers.map((m) => `${m.name}=${m.selected}`).join(",")} ${snapshot.color_mode}`;

const shown = signal("waiting");
setTimeout(async () => {
  const lines = [];
  try {
    const services = await new RayXAppBuilder().addHostFakes()
      .addTheme((options) => options.context("accent", "cyan").colorMode("Dark"))
      .start();
    const theme = services.getRequiredService(ThemeService);
    lines.push(String(theme === services.getRequiredService(ThemeService)));
    lines.push(shows(theme.current()));

    const heard = [];
    const stop = theme.onChange((snapshot) => heard.push(shows(snapshot)));
    lines.push(String(await theme.setContext("accent", "sky")));
    await wait();
    lines.push(String(await theme.setContext("accent", "sky")));
    lines.push(Probe.selected(services, "accent"));
    try {
      await theme.setContext("brand", "forest");
      lines.push("picked");
    } catch (error) {
      lines.push(error.message.includes("no modifier `brand`") ? "refused" : `rejected ${error.message}`);
    }
    lines.push(String(await theme.setColorMode("System")));
    await wait();
    lines.push(shows(theme.current()));
    try {
      await theme.setColorMode("Auto");
      lines.push("chosen");
    } catch (error) {
      lines.push(error.message.includes("no color mode") ? "refused" : `rejected ${error.message}`);
    }
    stop();
    lines.push(heard.join(","));
  } catch (error) {
    lines.push(`threw ${error}`);
  }
  shown.set(`done ${lines.join("|")}`);
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn add_theme_and_the_class_reach_rayx_s_service() {
    let shown = testing::run(set(), RAYX);
    let parts: Vec<&str> = shown.split('|').collect();
    assert_eq!(parts[0], "done true", "{shown}");
    // The options' start picks and choice, applied before the app was ready.
    assert_eq!(parts[1], "colorMode=dark,accent=cyan Dark", "{shown}");
    assert_eq!(parts[2], "true", "{shown}");
    assert_eq!(parts[3], "false", "{shown}");
    assert_eq!(parts[4], "Some(\"sky\")", "{shown}");
    assert_eq!(parts[5], "refused", "{shown}");
    assert_eq!(parts[6], "true", "{shown}");
    // The host's fake platform is light.
    assert_eq!(parts[7], "colorMode=light,accent=sky System", "{shown}");
    assert_eq!(parts[8], "refused", "{shown}");
    let heard = parts[9];
    assert!(
        heard.starts_with("colorMode=dark,accent=sky Dark"),
        "{shown}"
    );
    assert!(
        heard.ends_with("colorMode=light,accent=sky System"),
        "{shown}"
    );
    assert_eq!(parts.len(), 10, "{shown}");
}

const MINE: &str = r#"import { mount, signal, Text } from "nsh";
import { ContractState, Probe, RayXAppBuilder, ThemeService } from "test/app";

const snapshot = (accent) => ({
  modifiers: [{ name: "accent", contexts: ["blue", "cyan"], default: "blue", selected: accent }],
  color_mode: "System",
  revision: 0,
});

class MyTheme {
  theme = new ContractState(snapshot("blue"));
  asked = [];
  isInitialized() { return true; }
  async initialize() {}
  async setContext(modifier, context) {
    this.asked.push(`${modifier}=${context}`);
    if (modifier !== "accent") throw new Error(`mine has no ${modifier}`);
    this.theme.publish(snapshot(context));
    return true;
  }
  async setColorMode(choice) {
    this.asked.push(choice);
    return false;
  }
}

const shown = signal("waiting");
setTimeout(async () => {
  const lines = [];
  let mine;
  try {
    const services = await new RayXAppBuilder().addHostFakes()
      .addTheme()
      .addStartupSingletonAs(ThemeService, () => (mine = new MyTheme()))
      .start();
    lines.push(Probe.selected(services, "accent"));
    lines.push(await Probe.pick(services, "accent", "cyan"));
    lines.push(Probe.selected(services, "accent"));
    lines.push(await Probe.pick(services, "density", "compact"));
    const theme = services.getRequiredService(ThemeService);
    lines.push(String(await theme.setColorMode("Dark")));
    try {
      await theme.setContext("os", "macos");
      lines.push("picked");
    } catch (error) {
      lines.push(`rejected ${error.message}`);
    }
    lines.push(mine.asked.join(","));
  } catch (error) {
    lines.push(`threw ${error}`);
  }
  shown.set(`done ${lines.join("|")}`);
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn a_ts_implementation_replaces_rayx_s_and_its_throw_reaches_rust_as_failed() {
    let shown = testing::run(set(), MINE);
    let parts: Vec<&str> = shown.split('|').collect();
    assert_eq!(parts[0], "done Some(\"blue\")", "{shown}");
    assert_eq!(parts[1], "ok true", "{shown}");
    assert_eq!(parts[2], "Some(\"cyan\")", "{shown}");
    assert!(parts[3].starts_with("failed "), "{shown}");
    assert!(parts[3].contains("mine has no density"), "{shown}");
    assert_eq!(parts[4], "false", "{shown}");
    assert!(parts[5].starts_with("rejected "), "{shown}");
    assert!(parts[5].contains("mine has no os"), "{shown}");
    assert_eq!(
        parts[6], "accent=cyan,density=compact,Dark,os=macos",
        "{shown}"
    );
    assert_eq!(parts.len(), 7, "{shown}");
}

#[test]
fn the_class_its_options_the_contract_and_add_theme_are_declared() {
    let ts = testing::declarations(&set());
    let add = "addTheme(configure?: ((options: ThemeOptions) => void) | null): RayXAppBuilder;";
    for declared in [
        "export declare class ThemeService {",
        "  current(): ThemeSnapshot;",
        "  setContext(modifier: string, context: string): Promise<boolean>;",
        "  setColorMode(choice: ColorModeChoice): Promise<boolean>;",
        "  onChange(listener: (snapshot: ThemeSnapshot) => void): () => void;",
        "export declare class ThemeOptions {",
        "  colorMode(choice: ColorModeChoice): ThemeOptions;",
        "export interface ThemeServiceContract {",
        "export type ThemeSnapshot = ",
        "export type ThemeModifier = ",
        "export type ColorModeChoice = \"System\" | \"Light\" | \"Dark\";",
        add,
    ] {
        assert!(ts.contains(declared), "no `{declared}` in\n{ts}");
    }
    let before = ts[..ts.find(add).unwrap()].trim_end();
    assert!(
        before.ends_with("*/"),
        "no doc comment on addTheme in\n{ts}"
    );
    let doc = &before[before.rfind("/**").unwrap()..];
    assert!(doc.contains("theme service"), "{doc}");
    assert_eq!(set().undocumented(), Vec::<String>::new());
}

#[test]
fn without_the_set_there_is_no_add_theme() {
    let ts = testing::declarations(&testing::set(|_| {}));
    assert!(!ts.contains("addTheme("), "{ts}");
    assert!(!ts.contains("class ThemeService"), "{ts}");
}

/// A JS app's frames: the texts of each batch, on a surface whose scheme is `scheme` from its
/// start (the app's NSH platform service follows it).
fn frames(tsx: &str, scheme: nsh_core::platform::ColorScheme) -> Vec<String> {
    use std::time::{Duration, Instant};

    use crossbeam_channel::Sender;
    use nsh_core::{ApproxTextEngine, MountBatch, MountItem};
    use nsh_engine::{App, Bundle, Engine, EngineConfig, Environment, MountSink};

    struct Sink(Sender<MountBatch>);
    impl MountSink for Sink {
        fn submit(&self, batch: MountBatch) {
            let _ = self.0.send(batch);
        }
    }

    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "rayx-theme-first-frame-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let entry = dir.join("App.tsx");
    std::fs::write(&entry, tsx).unwrap();
    let sets = vec![testing::set(|set| {
        rayx_services_platform::register(set);
        rayx_services_theme::register(set);
        set.theme_document(THEME);
    })];
    let bundle = nsh_bundler::bundle(&nsh_bundler::BundleOptions {
        development: false,
        app: nsh_bundler::AppInfo::default(),
        asset_root_from: None,
        entry,
        minify: false,
        split: false,
        ui_sets: sets.iter().map(Into::into).collect(),
    })
    .unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    let (tx, rx) = crossbeam_channel::unbounded();
    let config = EngineConfig {
        ui_sets: sets,
        environment: Environment {
            color_scheme: scheme,
            ..Environment::default()
        },
        ..EngineConfig::new(App::Js(Bundle::source(bundle.code)))
    };
    let _engine = Engine::start(config, || ApproxTextEngine, Sink(tx)).unwrap();
    let mut frames = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(10);
    while let Ok(batch) = rx.recv_deadline(deadline) {
        let texts: Vec<String> = batch
            .items
            .iter()
            .filter_map(|item| match item {
                MountItem::Text { text, .. } => Some(text.text.to_string()),
                _ => None,
            })
            .collect();
        if texts.is_empty() {
            continue;
        }
        let done = texts.iter().any(|text| text.starts_with("ready"));
        frames.push(texts.join(";"));
        if done {
            break;
        }
    }
    frames
}

/// An app that builds `builder` before `mount` and shows the light/dark modifier and the
/// scheme, then `ready` once it started.
fn first_frame_app(builder: &str) -> String {
    format!(
        r#"import {{ colorScheme, modifier, mount, signal, Text }} from "nsh";
import {{ RayXAppBuilder }} from "test/app";

const state = signal("starting");
{builder}.start().then(() => setTimeout(() => state.set("ready"), 30));
mount(() => <Text>{{`${{state()}} ${{modifier("colorMode")}} ${{colorScheme()}}`}}</Text>);
"#
    )
}

#[test]
fn under_a_dark_scheme_a_js_app_s_first_frame_shows_add_theme_s_pick() {
    use nsh_core::platform::ColorScheme;

    let frames = frames(
        &first_frame_app("new RayXAppBuilder().addPlatform().addHostFakes().addTheme()"),
        ColorScheme::Dark,
    );
    assert_eq!(frames[0], "starting dark dark", "{frames:?}");
    assert!(
        frames.iter().all(|frame| frame.ends_with("dark dark")),
        "{frames:?}"
    );
    assert!(frames.last().unwrap().starts_with("ready"), "{frames:?}");
}

#[test]
fn a_light_choice_shows_light_from_the_first_frame_under_a_dark_scheme() {
    use nsh_core::platform::ColorScheme;

    let frames = frames(
        &first_frame_app(
            "new RayXAppBuilder().addPlatform().addHostFakes().addTheme((options) => options.colorMode(\"Light\"))",
        ),
        ColorScheme::Dark,
    );
    // The tokens show light from the first frame. (JS's `colorScheme()` is JS's own appearance,
    // which only JS's `setColorScheme` sets: it keeps the system's `dark`.)
    assert!(frames[0].starts_with("starting light "), "{frames:?}");
    assert!(
        frames
            .iter()
            .all(|frame| frame.split(' ').nth(1) == Some("light")),
        "{frames:?}"
    );
}

#[test]
fn without_the_service_the_theme_shows_its_default_light() {
    use nsh_core::platform::ColorScheme;

    let frames = frames(
        &first_frame_app("new RayXAppBuilder().addPlatform().addHostFakes()"),
        ColorScheme::Dark,
    );
    assert_eq!(frames[0], "starting light dark", "{frames:?}");
    assert!(
        frames.last().unwrap().starts_with("ready light dark"),
        "{frames:?}"
    );
}
