//! RayX's `Button` from TSX, headless: the library's set, `rayx/components`, exports it with typed
//! declarations, an app's UI set serves it, a TSX screen imports it from `rayx/components`, sets
//! the Figma props in camel case and fills the slots with JSX, presses reach `onPress`, RayX's roles are classes (`bg-surface-app`, `p-gap4` reading
//! `spacing-gap4`), and `setModifier` switches the modes. The expected values are those RayX's
//! default theme gives the roles.

// Loading the compiled TSX app takes `Bundle::bytecode`.
#![allow(unsafe_code, clippy::expect_used, clippy::unwrap_used)]

mod support;

use std::time::{Duration, Instant};

use crossbeam_channel::Receiver;
use nsh_core::color::rgba;
use nsh_core::element::declarations;
use nsh_core::{ApproxTextEngine, Color, MountBatch};
use nsh_engine::{App, Bundle, Engine, EngineConfig};
use support::{ChannelSink, Screen, described, ui_sets};

const TSX: &str = r#"import { mount, setModifier, signal, Text, View } from "nsh";
import { Button } from "rayx/components";

const presses = signal(0);
mount(() => (
  <View class="bg-surface-app p-gap4 gap-gap2 items-start">
    <Button text="Save" variant="primary" onPress={() => presses.set(presses() + 1)} />
    <Button text="Inbox" iconLeft={<Text>+</Text>} showSlotRight slotRight={<Text>3</Text>} />
    <Button text="Small" size="small" outlined />
    <Button text="Off" disabled onPress={() => presses.set(presses() + 100)} />
    <Button text="Touch" variant="ghost" onPress={() => { setModifier("accent", "sky"); setModifier("density", "touch"); }} />
    <Text class="text-surface-text-muted">{`presses ${presses()}`}</Text>
  </View>
));
"#;

struct Tsx {
    engine: Engine,
    rx: Receiver<MountBatch>,
    screen: Screen,
}

impl Tsx {
    fn start() -> Tsx {
        let dir = std::env::temp_dir().join(format!("rayx-components-tsx-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let entry = dir.join("App.tsx");
        std::fs::write(&entry, TSX).unwrap();
        let sets = ui_sets();
        let bundle = nsh_bundler::bundle(&nsh_bundler::BundleOptions {
            development: false,
            app: nsh_bundler::AppInfo::default(),
            asset_root_from: None,
            app_dir: None,
            entry,
            minify: false,
            split: false,
            ui_sets: described(),
            ..Default::default()
        })
        .unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        let bytecode = nsh_js::compile_bytecode_with(&bundle.code, false, &described()).unwrap();
        let (tx, rx) = crossbeam_channel::unbounded();
        // SAFETY: compiled just now by this runtime.
        let app = App::Js(unsafe { Bundle::bytecode(bytecode) });
        let config = EngineConfig {
            ui_sets: sets,
            ..EngineConfig::new(app)
        };
        let engine = Engine::start(config, || ApproxTextEngine, ChannelSink(tx)).unwrap();
        Tsx {
            engine,
            rx,
            screen: Screen::default(),
        }
    }

    fn wait(&mut self, done: impl Fn(&Screen) -> bool) -> bool {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !done(&self.screen) {
            match self.rx.recv_deadline(deadline) {
                Ok(batch) => self.screen.apply(&batch),
                Err(_) => return false,
            }
        }
        while let Ok(batch) = self.rx.recv_timeout(Duration::from_millis(50)) {
            self.screen.apply(&batch);
        }
        true
    }

    fn press(&self, label: &str) {
        let id = self
            .screen
            .pressable(label)
            .unwrap_or_else(|| panic!("no button `{label}`"));
        for kind in [
            nsh_core::EventKind::PressDown,
            nsh_core::EventKind::PressUp,
            nsh_core::EventKind::Press,
        ] {
            self.engine
                .control()
                .dispatch_event(id, kind, nsh_core::EventPayload::None);
        }
    }
}

#[test]
fn the_library_set_declares_the_figma_props_for_tsx() {
    let set = rayx_components::set();
    assert_eq!(set.specifier(), Some("rayx/components"));
    assert_eq!(set.undocumented(), Vec::<String>::new());
    let ts = declarations(&set, &[]);
    for line in [
        r#"export type ButtonVariant = "neutral" | "primary" | "danger" | "success" | "ghost" | "link";"#,
        r#"export type ComponentSize = "small" | "default" | "large";"#,
        "  text: string;",
        "  variant?: ButtonVariant;",
        "  outlined?: boolean;",
        "  size?: ComponentSize;",
        "  disabled?: boolean;",
        "  showLeftIcon?: boolean;",
        "  iconLeft?: JSX.Element;",
        "  showRightIcon?: boolean;",
        "  iconRight?: JSX.Element;",
        "  showSlotLeft?: boolean;",
        "  slotLeft?: JSX.Element;",
        "  showSlotRight?: boolean;",
        "  slotRight?: JSX.Element;",
        "  onPress?: () => void;",
        "export declare const Button: ((props: ButtonProps) => JSX.Element);",
    ] {
        assert!(ts.contains(line), "no `{line}` in:\n{ts}");
    }
}

#[test]
fn a_tsx_screen_uses_buttons_tokens_and_modes() {
    let mut app = Tsx::start();
    assert!(
        app.wait(|s| s
            .pressable("Touch")
            .is_some_and(|id| s.frames.contains_key(&id))),
        "{:?}",
        app.screen.shown()
    );
    let hex = |value: u32| Color::from(rgba(value));

    // The props: sizes from lvl5, slots filled with JSX.
    let height = |s: &Screen, label: &str| s.frames.get(&s.button(label).unwrap()).unwrap().1;
    assert_eq!(height(&app.screen, "Save"), 32.0, "default lvl5, desktop");
    assert_eq!(height(&app.screen, "Small"), 24.0, "small lvl5, desktop");
    assert!(
        app.wait(|s| s.text_node("+").is_some() && s.text_node("3").is_some()),
        "{:?}",
        app.screen.shown()
    );

    // RayX's roles are the app's classes: the page's fill and padding.
    let save = app.screen.button("Save").unwrap();
    let page = *app.screen.parents.get(&save).unwrap();
    let fill = app.screen.styles.get(&page).unwrap().background_color;
    assert_eq!(fill, hex(0xf5f5f5ff), "surface-app, light");
    assert_eq!(
        app.screen.origins.get(&save).unwrap().0,
        12.0,
        "spacing-gap4"
    );

    // Presses: `onPress` hears the enabled button only.
    app.press("Save");
    assert!(
        app.wait(|s| s.text_node("presses 1").is_some()),
        "{:?}",
        app.screen.shown()
    );
    assert!(
        app.screen.pressable("Off").is_none(),
        "a disabled button hears no presses"
    );

    // `setModifier`: Touch's lvl5 and Sky's primary, on the same views.
    app.press("Touch");
    assert!(
        app.wait(|s| s.frames.get(&save).is_some_and(|f| f.1 == 48.0)),
        "{:?}",
        app.screen.frames.get(&save)
    );
    let primary = hex(0x075e8eff);
    assert!(app.wait(|s| {
        s.styles
            .get(&save)
            .is_some_and(|st| st.background_color == primary)
    }));
    drop(app.engine);
}
