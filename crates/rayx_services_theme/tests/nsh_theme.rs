//! RayX's theme service on NSH's engine, headless (each app on an engine thread of its own, the
//! test on another): the builder's pick reaches the first frame, the start picks are applied
//! before the app is ready, picks from other threads reach NSH, the snapshot is published once
//! NSH shows a pick (and follows a change of document), each surface keeps its own picks, and the
//! light/dark modifier follows the color scheme NSH shows, or the app's choice.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, Sender};
use futures::executor::block_on;
use nsh_core::element::{Root, modifier, modifiers, runtime_entered, set_modifier};
use nsh_core::platform::ColorScheme;
use nsh_core::{ApproxTextEngine, MountBatch, MountItem, prelude::*};
use nsh_engine::{App, Engine, EngineConfig, Environment, MountSink};
use rayx_app::services::theme::{ColorModeChoice, ThemeError, ThemeService};
use rayx_app::testing::add_host_fakes;
use rayx_app_nsh::{
    AppState, EngineRunner, RayXAppBuilder, ServiceProvider, StartAppBuilderExt as _, app_services,
    app_state,
};
use rayx_services_platform::PlatformAppBuilderExt as _;
use rayx_services_theme::ThemeAppBuilderExt as _;
use serde_json::json;

/// The app's theme: `shade` shows the light/dark modifier's values, `space` the density's.
const THEME: &str = r#"{
  "tokens": {
    "shade": { "kind": "number", "value": 1 },
    "space": { "kind": "number", "value": 4 }
  },
  "modifiers": {
    "colorMode": { "default": "light", "contexts": { "light": {}, "dark": { "shade": 2 } } },
    "accent": { "default": "blue", "contexts": { "blue": {}, "cyan": {}, "sky": {} } },
    "density": { "default": "regular", "contexts": { "regular": {}, "compact": { "space": 2 } } }
  },
  "resolutionOrder": ["colorMode", "accent", "density"]
}"#;

/// Another document: no `density`.
const NO_DENSITY: &str = r#"{
  "tokens": { "shade": { "kind": "number", "value": 1 } },
  "modifiers": {
    "colorMode": { "default": "light", "contexts": { "light": {}, "dark": { "shade": 2 } } },
    "accent": { "default": "blue", "contexts": { "blue": {}, "cyan": {}, "sky": {} } }
  },
  "resolutionOrder": ["colorMode", "accent"]
}"#;

/// Another document: no light/dark modifier.
const NO_COLOR_MODE: &str = r#"{
  "tokens": { "shade": { "kind": "number", "value": 1 } },
  "modifiers": {
    "accent": { "default": "blue", "contexts": { "blue": {}, "cyan": {}, "sky": {} } }
  },
  "resolutionOrder": ["accent"]
}"#;

/// What the surface shows: the app's state, NSH's modifiers, the tokens and the color scheme.
fn line(state: &str) -> String {
    let shown = modifiers()
        .iter()
        .map(|axis| format!("{}={}", axis.name, axis.selected))
        .collect::<Vec<_>>()
        .join(",");
    let tokens = tokens();
    let number = |name: &str| {
        tokens
            .number(name)
            .map_or("-".to_owned(), |n| n.to_string())
    };
    format!(
        "{state} {shown} shade={} space={} scheme={}",
        number("shade"),
        number("space"),
        if dark() { "dark" } else { "light" }
    )
}

/// The root's view: its line, and the app's services in `slot` once it is ready.
fn view(slot: &'static Mutex<Option<ServiceProvider>>) -> impl IntoElement {
    dynamic(move || match app_state() {
        AppState::Ready => {
            *slot.lock().unwrap() = Some(app_services().unwrap());
            text(move || line("ready")).into_element()
        }
        AppState::Failed(message) => text(format!("failed: {message}")).into_element(),
        _ => text(move || line("starting")).into_element(),
    })
}

/// A root component `$root` that starts the app `$build` makes of `$builder` (with the host's
/// fakes but NSH's platform service), keeping its services in `$slot`.
macro_rules! surface {
    ($root:ident, $slot:ident, |$builder:ident| $build:expr) => {
        static $slot: Mutex<Option<ServiceProvider>> = Mutex::new(None);

        #[component]
        fn $root() -> impl IntoElement {
            let $builder = RayXAppBuilder::new().add_platform();
            add_host_fakes($build).start();
            view(&$slot)
        }
    };
}

struct ChannelSink(Sender<MountBatch>);

impl MountSink for ChannelSink {
    fn submit(&self, batch: MountBatch) {
        let _ = self.0.send(batch);
    }
}

/// A running app, on an engine thread of its own.
struct Surface {
    engine: Engine,
    rx: Receiver<MountBatch>,
    slot: &'static Mutex<Option<ServiceProvider>>,
    /// Every line shown, in order.
    lines: Vec<String>,
}

impl Surface {
    fn start(
        root: nsh_core::element::Root,
        slot: &'static Mutex<Option<ServiceProvider>>,
        scheme: ColorScheme,
    ) -> Self {
        let (tx, rx) = crossbeam_channel::unbounded();
        let engine = Engine::start(
            EngineConfig {
                environment: Environment {
                    color_scheme: scheme,
                    ..Environment::default()
                },
                ..EngineConfig::new(App::Rust(root.theme(THEME)))
            },
            || ApproxTextEngine,
            ChannelSink(tx),
        )
        .unwrap();
        let mut surface = Self {
            engine,
            rx,
            slot,
            lines: Vec::new(),
        };
        surface.wait(|line| line.starts_with("ready") || line.starts_with("failed"));
        surface
    }

    /// Waits for a line `until` holds for (or fails after 10 s); the line.
    fn wait(&mut self, until: impl Fn(&str) -> bool) -> String {
        if let Some(last) = self.lines.last()
            && until(last)
        {
            return last.clone();
        }
        let deadline = Instant::now() + Duration::from_secs(10);
        while let Ok(batch) = self.rx.recv_deadline(deadline) {
            for item in &batch.items {
                if let MountItem::Text { text, .. } = item {
                    self.lines.push(text.text.to_string());
                }
            }
            if let Some(last) = self.lines.last()
                && until(last)
            {
                return last.clone();
            }
        }
        panic!("no such line; shown: {:#?}", self.lines);
    }

    /// Waits until the last line shown contains each of `parts`.
    fn shows(&mut self, parts: &[&str]) -> String {
        self.wait(|line| parts.iter().all(|part| line.contains(part)))
    }

    fn services(&self) -> ServiceProvider {
        self.slot.lock().unwrap().clone().unwrap()
    }

    fn theme(&self) -> Arc<dyn ThemeService> {
        self.services()
            .required_service::<dyn ThemeService>()
            .unwrap()
    }

    /// Runs `work` on the surface's engine thread.
    fn on_engine(&self, work: impl FnOnce() + Send + 'static) {
        let runner = self.services().required_service::<EngineRunner>().unwrap();
        block_on(runner.run(move || async move { work() })).unwrap();
    }

    /// Waits until the published snapshot shows `context` for `modifier`.
    fn published(&self, modifier: &str, context: Option<&str>) {
        let theme = self.theme();
        let deadline = Instant::now() + Duration::from_secs(10);
        while theme.selected(modifier).as_deref() != context {
            assert!(
                Instant::now() < deadline,
                "{modifier} never published {context:?}: {:?}",
                theme.snapshot()
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}

surface!(FirstFrame, FIRST_FRAME, |builder| builder.add_theme(|_| {}));

#[test]
fn the_builder_s_pick_reaches_the_first_frame() {
    let surface = Surface::start(Root::of::<FirstFrame>(), &FIRST_FRAME, ColorScheme::Dark);
    let first = &surface.lines[0];
    assert!(
        first.starts_with("starting"),
        "the app was ready at its first frame: {first}"
    );
    assert!(
        first.contains("colorMode=dark") && first.contains("shade=2"),
        "the first frame is not dark: {first}"
    );
    assert!(
        surface
            .lines
            .iter()
            .all(|line| line.contains("colorMode=dark")),
        "{:#?}",
        surface.lines
    );
    assert_eq!(surface.theme().color_mode(), ColorModeChoice::System);
}

surface!(StartPicks, START_PICKS, |builder| {
    let mut builder = builder.add_theme(|_| {});
    builder.configuration.add_value(json!({
        "RayX": { "Services": { "Theme": {
            "contexts": { "accent": "cyan", "brand": "forest", "density": "spacious" }
        } } }
    }));
    builder
});

#[test]
fn the_start_picks_apply_before_ready_and_picks_from_any_thread_reach_nsh() {
    let mut surface = Surface::start(Root::of::<StartPicks>(), &START_PICKS, ColorScheme::Light);
    // The bound start picks are applied before Ready; the ones the document lacks are skipped.
    let ready: Vec<_> = surface
        .lines
        .iter()
        .filter(|line| line.starts_with("ready"))
        .collect();
    assert!(!ready.is_empty(), "{:#?}", surface.lines);
    assert!(
        ready
            .iter()
            .all(|line| line.contains("accent=cyan") && line.contains("density=regular")),
        "{:#?}",
        surface.lines
    );
    surface.published("accent", Some("cyan"));

    // A pick from this thread (not the engine's) reaches NSH, then the snapshot.
    let theme = surface.theme();
    assert!(block_on(theme.set_context("density".into(), "compact".into())).unwrap());
    surface.shows(&["density=compact", "space=2"]);
    surface.published("density", Some("compact"));
    assert!(!block_on(theme.set_context("density".into(), "compact".into())).unwrap());
}

surface!(Ordering, ORDERING, |builder| builder.add_theme(|_| {}));

#[test]
fn the_snapshot_is_published_once_nsh_shows_the_pick() {
    let mut surface = Surface::start(Root::of::<Ordering>(), &ORDERING, ColorScheme::Light);
    let theme = surface.theme();
    let heard = Arc::new(Mutex::new(Vec::<(Option<String>, Option<String>)>::new()));
    let heard_by = Arc::clone(&heard);
    let _subscription = theme.theme().subscribe(move |snapshot| {
        // What NSH shows where the listener runs, the engine thread.
        let nsh = runtime_entered()
            .then(|| untracked(|| modifier("accent")).map(String::from))
            .flatten();
        heard_by
            .lock()
            .unwrap()
            .push((snapshot.selected("accent").map(str::to_owned), nsh));
    });
    assert!(block_on(theme.set_context("accent".into(), "sky".into())).unwrap());
    surface.published("accent", Some("sky"));
    let heard = heard.lock().unwrap().clone();
    assert!(!heard.is_empty());
    assert!(
        heard.iter().all(|(published, nsh)| published == nsh),
        "{heard:?}"
    );

    // Two picks from two threads: the snapshot ends as NSH's selection.
    let picks: Vec<_> = ["cyan", "blue"]
        .into_iter()
        .map(|context| {
            let theme = Arc::clone(&theme);
            std::thread::spawn(move || {
                block_on(theme.set_context("accent".into(), context.into())).unwrap()
            })
        })
        .collect();
    for pick in picks {
        pick.join().unwrap();
    }
    let line = surface.wait(|line| line.contains("accent=cyan") || line.contains("accent=blue"));
    std::thread::sleep(Duration::from_millis(100));
    let line = surface.lines.last().cloned().unwrap_or(line);
    let shown = if line.contains("accent=cyan") {
        "cyan"
    } else {
        "blue"
    };
    surface.published("accent", Some(shown));
    assert_eq!(theme.snapshot().revision, theme.theme().get().revision);
}

surface!(Refusals, REFUSALS, |builder| builder.add_theme(|_| {}));

#[test]
fn unknown_modifiers_and_contexts_are_refused_on_the_engine_too() {
    let surface = Surface::start(Root::of::<Refusals>(), &REFUSALS, ColorScheme::Light);
    let theme = surface.theme();
    let before = theme.snapshot();
    for (modifier, context, error) in [
        (
            "brand",
            "forest",
            ThemeError::UnknownModifier {
                modifier: "brand".into(),
            },
        ),
        (
            "accent",
            "red",
            ThemeError::UnknownContext {
                modifier: "accent".into(),
                context: "red".into(),
            },
        ),
        (
            "colorMode",
            "sepia",
            ThemeError::UnknownContext {
                modifier: "colorMode".into(),
                context: "sepia".into(),
            },
        ),
    ] {
        assert_eq!(
            block_on(theme.set_context(modifier.into(), context.into())),
            Err(error)
        );
    }
    std::thread::sleep(Duration::from_millis(50));
    assert_eq!(theme.snapshot(), before);
}

surface!(Documents, DOCUMENTS, |builder| builder.add_theme(
    |options| {
        options.context("accent", "cyan");
    }
));

#[test]
fn the_snapshot_follows_a_change_of_document() {
    let mut surface = Surface::start(Root::of::<Documents>(), &DOCUMENTS, ColorScheme::Light);
    let theme = surface.theme();
    surface.published("accent", Some("cyan"));
    assert_eq!(theme.selected("density").as_deref(), Some("regular"));

    // A document without `density`: it leaves the snapshot, the accent's pick stays.
    surface.on_engine(|| drop(set_theme(Some(NO_DENSITY))));
    surface.published("density", None);
    assert_eq!(theme.selected("accent").as_deref(), Some("cyan"));
    surface.shows(&["ready colorMode=light,accent=cyan shade"]);

    // Without the light/dark modifier the choice is kept for a document that has it.
    surface.on_engine(|| drop(set_theme(Some(NO_COLOR_MODE))));
    surface.published("colorMode", None);
    assert!(block_on(theme.set_color_mode(ColorModeChoice::Dark)).unwrap());
    assert_eq!(theme.color_mode(), ColorModeChoice::Dark);
    surface.shows(&["ready accent=cyan shade=1"]);
    surface.on_engine(|| drop(set_theme(Some(THEME))));
    surface.shows(&["colorMode=dark", "accent=cyan", "shade=2"]);
    surface.published("colorMode", Some("dark"));
}

surface!(Cyan, CYAN, |builder| builder.add_theme(|options| {
    options.context("accent", "cyan");
}));
surface!(Sky, SKY, |builder| builder.add_theme(|options| {
    options
        .context("accent", "sky")
        .context("density", "compact");
}));

#[test]
fn each_surface_keeps_its_own_picks() {
    let mut cyan = Surface::start(Root::of::<Cyan>(), &CYAN, ColorScheme::Light);
    let mut sky = Surface::start(Root::of::<Sky>(), &SKY, ColorScheme::Dark);
    cyan.shows(&["accent=cyan", "density=regular", "colorMode=light"]);
    sky.shows(&["accent=sky", "density=compact", "colorMode=dark"]);

    assert!(block_on(cyan.theme().set_context("accent".into(), "blue".into())).unwrap());
    cyan.shows(&["accent=blue"]);
    sky.published("accent", Some("sky"));
    assert!(sky.lines.last().unwrap().contains("accent=sky"));
    assert!(!Arc::ptr_eq(&cyan.theme(), &sky.theme()));
}

surface!(ColorMode, COLOR_MODE, |builder| builder.add_theme(|_| {}));

#[test]
fn the_light_dark_modifier_follows_the_scheme_or_the_choice() {
    let mut surface = Surface::start(Root::of::<ColorMode>(), &COLOR_MODE, ColorScheme::Light);
    let theme = surface.theme();
    surface.shows(&["colorMode=light", "scheme=light"]);

    // `Dark` shows dark whatever the system's scheme, and sets NSH's appearance to it.
    assert!(block_on(theme.set_color_mode(ColorModeChoice::Dark)).unwrap());
    surface.shows(&["colorMode=dark", "shade=2", "scheme=dark"]);
    surface.published("colorMode", Some("dark"));

    // Back to `System`: NSH shows the system's scheme again, and the modifier follows it.
    assert!(block_on(theme.set_color_mode(ColorModeChoice::System)).unwrap());
    surface.shows(&["colorMode=light", "shade=1", "scheme=light"]);

    // The system turning dark is followed.
    let control = surface.engine.control();
    control.set_environment(Environment {
        color_scheme: ColorScheme::Dark,
        ..Environment::default()
    });
    surface.shows(&["colorMode=dark", "scheme=dark"]);
    surface.published("colorMode", Some("dark"));

    // `set_context` on the light/dark modifier is a choice.
    assert!(block_on(theme.set_context("colorMode".into(), "light".into())).unwrap());
    assert_eq!(theme.color_mode(), ColorModeChoice::Light);
    surface.shows(&["colorMode=light", "scheme=light"]);
    control.set_environment(Environment {
        color_scheme: ColorScheme::Light,
        ..Environment::default()
    });
    control.set_environment(Environment {
        color_scheme: ColorScheme::Dark,
        ..Environment::default()
    });
    std::thread::sleep(Duration::from_millis(100));
    surface.shows(&["colorMode=light", "scheme=light"]);
    assert_eq!(theme.color_mode(), ColorModeChoice::Light);
}

surface!(ForcedDark, FORCED_DARK, |builder| {
    set_color_scheme(Appearance::Dark);
    builder.add_theme(|_| {})
});

#[test]
fn an_app_appearance_forced_dark_is_kept_under_system() {
    let mut surface = Surface::start(Root::of::<ForcedDark>(), &FORCED_DARK, ColorScheme::Light);
    surface.shows(&["ready", "colorMode=dark", "shade=2", "scheme=dark"]);
    std::thread::sleep(Duration::from_millis(100));
    surface.shows(&["ready", "colorMode=dark", "scheme=dark"]);
    let theme = surface.theme();
    assert_eq!(theme.color_mode(), ColorModeChoice::System);
    surface.published("colorMode", Some("dark"));
}

surface!(Around, AROUND, |builder| builder.add_theme(|_| {}));

#[test]
fn a_light_dark_pick_made_around_the_service_is_adopted() {
    let mut surface = Surface::start(Root::of::<Around>(), &AROUND, ColorScheme::Light);
    let theme = surface.theme();
    surface.shows(&["ready", "colorMode=light", "scheme=light"]);

    // The app's own `set_modifier`: the choice becomes `Dark`, and NSH's appearance with it.
    surface.on_engine(|| set_modifier("colorMode", "dark"));
    surface.shows(&["colorMode=dark", "shade=2", "scheme=dark"]);
    surface.published("colorMode", Some("dark"));
    let deadline = Instant::now() + Duration::from_secs(10);
    while theme.color_mode() != ColorModeChoice::Dark {
        assert!(
            Instant::now() < deadline,
            "not adopted: {:?}",
            theme.snapshot()
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}
