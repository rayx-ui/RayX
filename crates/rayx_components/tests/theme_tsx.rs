//! RayX's tokens as TSX classes, headless: with the library in the app's UI set, a TSX screen
//! names RayX's tokens in classes (`bg-surfaces-panel1`, `p-insets-inset4`,
//! `rounded-borders-radius4`, the parts of a text style, line height and letter spacing through
//! `leading-(--spacing-…)` and `tracking-(--spacing-…)`) and gets their values, and
//! `Themes.set` gives them the values of another mode.

// Loading the compiled TSX app takes `Bundle::bytecode`.
#![allow(unsafe_code, clippy::expect_used, clippy::unwrap_used)]

mod support;

use std::time::{Duration, Instant};

use nsh_core::{ApproxTextEngine, Color, MountBatch, MountItem};
use nsh_engine::{App, Bundle, Engine, EngineConfig};
use rayx_components::theme::{Accent, Density, RayXTheme};
use support::{ChannelSink, ui_sets};

const TSX: &str = r#"import { mount, Text, View } from "nsh";
import { Button, Themes } from "test/native";

mount(() => (
  <View class="bg-surfaces-panel1 p-insets-inset4 rounded-borders-radius4 gap-gap2">
    <Text class="text-surfaces-text text-typography-h3-font-size leading-(--spacing-typography-h3-line-height) font-typography-h3-font-weight tracking-(--spacing-typography-h3-letter-spacing)">
      Heading
    </Text>
    <Text class="text-surfaces-text-muted text-size-default-text-regular-font-size leading-(--spacing-size-default-text-regular-line-height) font-size-default-text-regular-font-weight tracking-(--spacing-size-default-text-regular-letter-spacing)">
      Body
    </Text>
    <Button text="Touch" onPress={() => Themes.set("blue", "touch")} />
  </View>
));
"#;

/// The style of each text, by its content, and the panel's.
#[derive(Default)]
struct Seen {
    texts: std::collections::HashMap<String, (f32, f32, u16, Color, f32)>,
    panel: Option<nsh_core::ViewStyle>,
    panel_id: Option<nsh_core::NodeId>,
    padding: Option<f32>,
    screen: support::Screen,
}

impl Seen {
    fn apply(&mut self, batch: &MountBatch) {
        self.screen.apply(batch);
        for item in &batch.items {
            if let MountItem::Text { text, .. } = item
                && let Some(span) = text.spans.first()
            {
                let s = &span.style;
                self.texts.insert(
                    text.text.to_string(),
                    (
                        s.font_size.0,
                        s.line_height.0,
                        s.font_weight,
                        s.color,
                        s.letter_spacing.0,
                    ),
                );
            }
        }
        if self.panel_id.is_none() {
            let heading = self.screen.text_node("Heading");
            self.panel_id = heading.and_then(|h| self.screen.parents.get(&h).copied());
        }
        if let Some(panel) = self.panel_id {
            self.panel = self.screen.styles.get(&panel).cloned();
            let heading = self.screen.text_node("Heading").unwrap();
            self.padding = self.screen.origins.get(&heading).map(|o| o.0);
        }
    }
}

fn start() -> (Engine, crossbeam_channel::Receiver<MountBatch>) {
    let dir = std::env::temp_dir().join(format!("rayx-theme-tsx-{}", std::process::id()));
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
    (engine, rx)
}

fn wait(
    rx: &crossbeam_channel::Receiver<MountBatch>,
    seen: &mut Seen,
    done: impl Fn(&Seen) -> bool,
) -> bool {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !done(seen) {
        match rx.recv_deadline(deadline) {
            Ok(batch) => seen.apply(&batch),
            Err(_) => return false,
        }
    }
    while let Ok(batch) = rx.recv_timeout(Duration::from_millis(50)) {
        seen.apply(&batch);
    }
    true
}

#[test]
fn rayx_tokens_are_tsx_classes_and_follow_the_modes() {
    let (engine, rx) = start();
    let mut seen = Seen::default();
    assert!(wait(&rx, &mut seen, |s| s.texts.contains_key("Body")
        && s.panel.is_some()));
    let check = |seen: &Seen, theme: &RayXTheme| {
        let colors = &theme.theme.colors;
        let density = &theme.platform.density;
        let panel = seen.panel.as_ref().unwrap();
        assert_eq!(
            panel.background_color,
            Color::from(colors.surfaces.panel1.light)
        );
        assert_eq!(panel.border_radius.0, density.borders.radius4);
        assert_eq!(seen.padding, Some(density.insets.inset4));
        let h3 = &density.typography.h3;
        let (size, leading, weight, color, tracking) = seen.texts["Heading"];
        assert_eq!(
            (size, leading, weight),
            (h3.font_size, h3.line_height, h3.font_weight as u16)
        );
        assert_eq!(tracking, h3.letter_spacing);
        assert_eq!(color, Color::from(colors.surfaces.text.light));
        let regular = &density.size.default.text.regular;
        let (size, leading, weight, color, tracking) = seen.texts["Body"];
        assert_eq!(tracking, regular.letter_spacing);
        assert_eq!(
            (size, leading, weight),
            (
                regular.font_size,
                regular.line_height,
                regular.font_weight as u16
            )
        );
        assert_eq!(color, Color::from(colors.surfaces.text_muted.light));
    };
    check(&seen, &RayXTheme::default());

    // Another mode: the classes take its values.
    let touch = support::Screen::pressable(&seen.screen, "Touch").unwrap();
    for kind in [
        nsh_core::EventKind::PressDown,
        nsh_core::EventKind::PressUp,
        nsh_core::EventKind::Press,
    ] {
        engine
            .control()
            .dispatch_event(touch, kind, nsh_core::EventPayload::None);
    }
    let blue_touch = RayXTheme::new(Accent::Blue, Density::Touch);
    let inset = blue_touch.platform.density.insets.inset4;
    assert!(
        wait(&rx, &mut seen, |s| s.padding == Some(inset)),
        "padding {:?}",
        seen.padding
    );
    check(&seen, &blue_touch);
    drop(engine);
}
