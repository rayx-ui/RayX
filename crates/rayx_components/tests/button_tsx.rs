//! RayX's `Button` and `Themes` from TSX, headless: an app's UI set that registers the library
//! exports them with typed declarations, a TSX screen sets the Figma props in camel case and fills
//! the slots with JSX, presses reach `onPress`, RayX's tokens are classes (`bg-surfaces-app`,
//! `p-gap4`), and `Themes.set` switches the modes.

// Loading the compiled TSX app takes `Bundle::bytecode`.
#![allow(unsafe_code, clippy::expect_used, clippy::unwrap_used)]

mod support;

use std::time::{Duration, Instant};

use crossbeam_channel::Receiver;
use nsh_core::element::{Registry, declarations};
use nsh_core::{ApproxTextEngine, Color, MountBatch};
use nsh_engine::{App, Bundle, Engine, EngineConfig};
use rayx_components::theme::{Accent, ComponentSize, Density, RayXTheme};
use support::{ChannelSink, Screen, ui_sets};

const TSX: &str = r#"import { mount, signal, Text, View } from "nsh";
import { Button, Themes } from "test/native";

const presses = signal(0);
mount(() => (
  <View class="bg-surfaces-app p-gap4 gap-gap2 items-start">
    <Button text="Save" variant="primary" onPress={() => presses.set(presses() + 1)} />
    <Button text="Inbox" iconLeft={<Text>+</Text>} showSlotRight slotRight={<Text>3</Text>} />
    <Button text="Small" size="small" outlined />
    <Button text="Off" disabled onPress={() => presses.set(presses() + 100)} />
    <Button text="Touch" variant="ghost" onPress={() => Themes.set("sky", "touch")} />
    <Text class="text-surfaces-text-muted">{`presses ${presses()}`}</Text>
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
            entry,
            minify: false,
            split: false,
            ui_sets: sets.iter().map(Into::into).collect(),
        })
        .unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        let described: Vec<_> = sets.iter().map(Into::into).collect();
        let bytecode = nsh_js::compile_bytecode_with(&bundle.code, false, &described).unwrap();
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
fn the_ui_set_declares_the_figma_props_for_tsx() {
    let mut set = Registry::ui_set("test/native");
    rayx_components::register(&mut set);
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
        "  set(accent?: Accent | null, density?: Density | null, os?: Os | null): void;",
        "export declare const Themes: ThemesModule;",
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
    let theme = RayXTheme::default();
    let density = &theme.platform.density;

    // The props: sizes from lvl5, slots filled with JSX.
    let height = |s: &Screen, label: &str| s.frames.get(&s.button(label).unwrap()).unwrap().1;
    assert_eq!(
        height(&app.screen, "Save"),
        density.size.of(ComponentSize::Default).lvl5.height
    );
    assert_eq!(
        height(&app.screen, "Small"),
        density.size.of(ComponentSize::Small).lvl5.height
    );
    assert!(
        app.wait(|s| s.text_node("+").is_some() && s.text_node("3").is_some()),
        "{:?}",
        app.screen.shown()
    );

    // RayX's tokens are the app's classes: the page's fill and padding.
    let save = app.screen.button("Save").unwrap();
    let page = *app.screen.parents.get(&save).unwrap();
    let fill = app.screen.styles.get(&page).unwrap().background_color;
    assert_eq!(fill, Color::from(theme.theme.colors.surfaces.app.light));
    assert_eq!(
        app.screen.origins.get(&save).unwrap().0,
        density.spacing.gap4
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

    // `Themes.set("sky", "touch")`: Touch's lvl5 and Sky's primary, on the same views.
    app.press("Touch");
    let touch = RayXTheme::new(Accent::Sky, Density::Touch);
    let expected = touch
        .platform
        .density
        .size
        .of(ComponentSize::Default)
        .lvl5
        .height;
    assert!(
        app.wait(|s| s.frames.get(&save).is_some_and(|f| f.1 == expected)),
        "{:?}",
        app.screen.frames.get(&save)
    );
    let primary = Color::from(touch.theme.colors.roles.primary.bg.light);
    assert!(app.wait(|s| {
        s.styles
            .get(&save)
            .is_some_and(|st| st.background_color == primary)
    }));
    drop(app.engine);
}
