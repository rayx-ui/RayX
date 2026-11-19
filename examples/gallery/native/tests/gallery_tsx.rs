//! The gallery's TSX screens on the gallery's crate, headless: `App.tsx` bundles against the
//! crate's UI set (RayX's library and the glyphs), starts RayX with the four services the host
//! requires and the theme service, shows the Button page, its playground sets the Figma
//! properties, and its toolbar switches the theme's modes. (`gallery_theme` checks the theme's
//! switches in detail.) (`nsh test` runs `App.test.tsx` the same way, with the nsh the
//! project pins.)

// Loading the compiled TSX app takes `Bundle::bytecode`.
#![allow(unsafe_code, clippy::expect_used, clippy::unwrap_used)]

use std::collections::HashMap;
use std::path::Path;
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, Sender};
use nsh_core::{ApproxTextEngine, EventKind, EventPayload, MountBatch, MountItem, NodeId};
use nsh_engine::{App, Bundle, Engine, EngineConfig, MountSink};

struct ChannelSink(Sender<MountBatch>);

impl MountSink for ChannelSink {
    fn submit(&self, batch: MountBatch) {
        let _ = self.0.send(batch);
    }
}

#[derive(Default)]
struct Screen {
    parents: HashMap<NodeId, NodeId>,
    texts: HashMap<NodeId, String>,
    heights: HashMap<NodeId, f32>,
    ids: HashMap<String, NodeId>,
}

impl Screen {
    fn apply(&mut self, batch: &MountBatch) {
        for item in &batch.items {
            match item {
                MountItem::Insert { parent, child, .. } => {
                    self.parents.insert(*child, *parent);
                }
                MountItem::Remove { id } | MountItem::Delete { id } => {
                    self.parents.remove(id);
                }
                MountItem::Text { id, text, .. } => {
                    self.texts.insert(*id, text.text.to_string());
                }
                MountItem::Frame { id, frame } => {
                    self.heights.insert(*id, frame.size.height.0);
                }
                MountItem::ViewProps { id, props } => {
                    if let Some(test_id) = &props.test_id {
                        self.ids.insert(test_id.to_string(), *id);
                    }
                }
                _ => {}
            }
        }
    }

    fn shows(&self, text: &str) -> bool {
        self.texts
            .iter()
            .any(|(id, t)| t == text && self.parents.contains_key(id))
    }
}

struct Gallery {
    engine: Engine,
    rx: Receiver<MountBatch>,
    screen: Screen,
}

impl Gallery {
    fn start() -> Gallery {
        let entry = Path::new(env!("CARGO_MANIFEST_DIR")).join("../App.tsx");
        let sets = vec![nsh_base::registry(), rayx_gallery::registry()];
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
        let mut gallery = Gallery {
            engine,
            rx,
            screen: Screen::default(),
        };
        assert!(gallery.wait(|s| s.shows("Examples") && s.ids.contains_key("playground")));
        gallery
    }

    fn wait(&mut self, done: impl Fn(&Screen) -> bool) -> bool {
        let deadline = Instant::now() + Duration::from_secs(20);
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

    fn press(&self, test_id: &str) {
        let id = *self
            .screen
            .ids
            .get(test_id)
            .unwrap_or_else(|| panic!("no `{test_id}`"));
        for kind in [EventKind::PressDown, EventKind::PressUp, EventKind::Press] {
            self.engine
                .control()
                .dispatch_event(id, kind, EventPayload::None);
        }
    }
}

#[test]
fn the_gallery_starts_rayx_with_the_services_the_host_requires_and_the_theme_service() {
    let mut gallery = Gallery::start();
    let settled =
        gallery.wait(|s| s.ids.contains_key("rayx-started") || s.ids.contains_key("rayx-failed"));
    let failure: Vec<_> = gallery
        .screen
        .texts
        .values()
        .filter(|t| t.starts_with("RayX did not start"))
        .collect();
    assert!(settled, "RayX's start did not settle");
    assert!(
        gallery.screen.ids.contains_key("rayx-started"),
        "{failure:?}"
    );
    assert!(failure.is_empty(), "{failure:?}");
    drop(gallery.engine);
}

const DEFAULT: &str = r#"<Button text="Button" iconLeft={<Glyph name="plus" />} iconRight={<Glyph name="chevronRight" />} showRightIcon={false} />"#;

#[test]
fn the_button_page_shows_the_playground_variants_and_examples() {
    let gallery = Gallery::start();
    for text in [
        "Button",
        "Playground",
        "Variants",
        "Sizes",
        "Examples",
        "Save changes",
        "Delete project",
        "Send a reset link",
        "Size large",
        DEFAULT,
    ] {
        assert!(gallery.screen.shows(text), "no `{text}`");
    }
    // Desktop's default size: lvl5 is 32.
    let playground = gallery.screen.ids["playground"];
    assert_eq!(gallery.screen.heights.get(&playground), Some(&32.0));
    drop(gallery.engine);
}

#[test]
fn the_playground_sets_the_figma_properties() {
    let mut gallery = Gallery::start();
    gallery.press("variant-primary");
    gallery.press("size-large");
    gallery.press("outlined-on");
    let code = r#"<Button text="Button" variant="primary" outlined size="large" iconLeft={<Glyph name="plus" />} iconRight={<Glyph name="chevronRight" />} showRightIcon={false} />"#;
    assert!(
        gallery.wait(|s| s.shows(code)),
        "the snippet did not follow"
    );
    let playground = gallery.screen.ids["playground"];
    assert!(gallery.wait(|s| s.heights.get(&playground) == Some(&40.0)));
    gallery.press("playground");
    assert!(gallery.wait(|s| s.shows("Pressed 1 time")));
    drop(gallery.engine);
}

#[test]
fn the_toolbar_switches_the_themes_modes() {
    let mut gallery = Gallery::start();
    // The pickers come from the theme service's snapshot, once RayX started.
    assert!(gallery.wait(|s| s.ids.contains_key("accent-sky")));
    gallery.press("accent-sky");
    gallery.press("density-touch");
    gallery.press("colormode-Dark");
    assert!(gallery.wait(|s| s.shows("rayx · dark · sky · touch · windows")));
    // Touch's default lvl5 is 48.
    let playground = gallery.screen.ids["playground"];
    assert!(gallery.wait(|s| s.heights.get(&playground) == Some(&48.0)));
    drop(gallery.engine);
}
