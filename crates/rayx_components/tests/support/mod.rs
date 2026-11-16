//! A headless engine and the tree it mounts, for the component tests: texts, frames, styles,
//! the events each view hears, and presses by label.

#![allow(dead_code, clippy::expect_used, clippy::unwrap_used)]

use std::collections::HashMap;
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, Sender};
use nsh_core::ApproxTextEngine;
use nsh_core::ViewStyle;
use nsh_core::element::Registry;
use nsh_core::{EventKind, EventMask, EventPayload, MountBatch, MountItem, NodeId};
use nsh_engine::{App, EngineConfig, LocalEngine, MountSink};

pub struct ChannelSink(pub Sender<MountBatch>);

impl MountSink for ChannelSink {
    fn submit(&self, batch: MountBatch) {
        let _ = self.0.send(batch);
    }
}

/// The UI sets of an app that registers RayX's library in its set `test/native`.
pub fn ui_sets() -> Vec<Registry> {
    let mut set = Registry::ui_set("test/native");
    rayx_components::register(&mut set);
    vec![nsh_base::registry(), set]
}

/// What the engine mounted so far.
#[derive(Default)]
pub struct Screen {
    pub parents: HashMap<NodeId, NodeId>,
    pub texts: HashMap<NodeId, String>,
    /// Each text's first span's font size, line height and letter spacing.
    pub text_styles: HashMap<NodeId, (f32, f32, f32)>,
    pub frames: HashMap<NodeId, (f32, f32)>,
    pub origins: HashMap<NodeId, (f32, f32)>,
    pub events: HashMap<NodeId, EventMask>,
    pub styles: HashMap<NodeId, ViewStyle>,
}

impl Screen {
    pub fn apply(&mut self, batch: &MountBatch) {
        for item in &batch.items {
            match item {
                MountItem::Insert { parent, child, .. } => {
                    self.parents.insert(*child, *parent);
                }
                MountItem::Remove { id } => {
                    self.parents.remove(id);
                }
                MountItem::Delete { id } => {
                    self.parents.remove(id);
                    self.texts.remove(id);
                    self.frames.remove(id);
                    self.origins.remove(id);
                    self.events.remove(id);
                    self.styles.remove(id);
                }
                MountItem::Text { id, text, .. } => {
                    self.texts.insert(*id, text.text.to_string());
                    if let Some(span) = text.spans.first() {
                        let s = &span.style;
                        self.text_styles
                            .insert(*id, (s.font_size.0, s.line_height.0, s.letter_spacing.0));
                    }
                }
                MountItem::Frame { id, frame } => {
                    self.frames
                        .insert(*id, (frame.size.width.0, frame.size.height.0));
                    self.origins
                        .insert(*id, (frame.origin.x.0, frame.origin.y.0));
                }
                MountItem::Events { id, events } => {
                    self.events.insert(*id, *events);
                }
                MountItem::ViewStyle { id, style } => {
                    self.styles.insert(*id, (**style).clone());
                }
                _ => {}
            }
        }
    }

    /// The texts on screen (attached to the tree).
    pub fn shown(&self) -> Vec<String> {
        let mut texts: Vec<String> = self
            .texts
            .iter()
            .filter(|(id, _)| self.parents.contains_key(id))
            .map(|(_, t)| t.clone())
            .collect();
        texts.sort();
        texts
    }

    /// The text view showing exactly `label`.
    pub fn text_node(&self, label: &str) -> Option<NodeId> {
        self.texts
            .iter()
            .find(|(id, t)| t.as_str() == label && self.parents.contains_key(id))
            .map(|(id, _)| *id)
    }

    /// The nearest view around `label` that hears presses: its button.
    pub fn pressable(&self, label: &str) -> Option<NodeId> {
        let mut at = self.text_node(label)?;
        loop {
            if self
                .events
                .get(&at)
                .is_some_and(|e| e.contains(EventKind::Press))
            {
                return Some(at);
            }
            at = *self.parents.get(&at)?;
        }
    }

    /// The button labelled `label`: the view its label is in (a button's root holds its label).
    pub fn button(&self, label: &str) -> Option<NodeId> {
        self.parents.get(&self.text_node(label)?).copied()
    }

    /// The children of `parent`, in no order.
    pub fn children(&self, parent: NodeId) -> Vec<NodeId> {
        self.parents
            .iter()
            .filter(|(_, p)| **p == parent)
            .map(|(c, _)| *c)
            .collect()
    }
}

/// An app on a headless engine, and what it mounted.
pub struct Running {
    pub engine: LocalEngine<ApproxTextEngine>,
    pub rx: Receiver<MountBatch>,
    pub screen: Screen,
}

impl Running {
    /// Starts `app` with RayX's library in its UI sets ([`ui_sets`]).
    pub fn start(app: App) -> Running {
        let (tx, rx) = crossbeam_channel::unbounded();
        let engine = LocalEngine::start(
            EngineConfig {
                ui_sets: ui_sets(),
                ..EngineConfig::new(app)
            },
            ApproxTextEngine,
            ChannelSink(tx),
        )
        .unwrap();
        Running {
            engine,
            rx,
            screen: Screen::default(),
        }
    }

    /// Runs the engine until `done` holds of the screen (or 10 s pass); whether it held.
    pub fn wait(&mut self, done: impl Fn(&Screen) -> bool) -> bool {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            self.engine.pump();
            while let Ok(batch) = self.rx.recv_timeout(Duration::from_millis(5)) {
                self.screen.apply(&batch);
            }
            if done(&self.screen) {
                return true;
            }
            if Instant::now() > deadline {
                return false;
            }
        }
    }

    /// Presses the button around `label`, as the platforms send a press.
    pub fn press(&mut self, label: &str) {
        let id = self
            .screen
            .pressable(label)
            .unwrap_or_else(|| panic!("no button `{label}`: {:?}", self.screen.shown()));
        self.press_node(id);
    }

    /// Presses node `id`.
    pub fn press_node(&mut self, id: NodeId) {
        for kind in [EventKind::PressDown, EventKind::PressUp, EventKind::Press] {
            self.engine
                .control()
                .dispatch_event(id, kind, EventPayload::None);
        }
    }
}

/// RayX's theme document as the tests' oracle (`theme.json`): what each token's value is with
/// some of its modifiers' contexts picked.
pub struct Document(serde_json::Value);

impl Document {
    pub fn load() -> Document {
        Document(serde_json::from_str(rayx_components::theme::DOCUMENT).expect("theme.json"))
    }

    /// Every declared token: its name and kind.
    pub fn tokens(&self) -> Vec<(String, String)> {
        self.0["tokens"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(name, d)| (name.clone(), d["kind"].as_str().unwrap().to_owned()))
            .collect()
    }

    /// The contexts of `modifier`, in the document's order.
    pub fn contexts(&self, modifier: &str) -> Vec<String> {
        self.0["modifiers"][modifier]["contexts"]
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect()
    }

    /// The token the role `role` is mapped onto.
    pub fn role(&self, role: &str) -> String {
        self.0["roles"][role].as_str().unwrap().to_owned()
    }

    /// `name`'s (light, dark) values with the contexts `picked` (`("accent", "sky")`), every
    /// other modifier at its default.
    pub fn value(
        &self,
        name: &str,
        picked: &[(&str, &str)],
    ) -> (serde_json::Value, serde_json::Value) {
        let mut light = None;
        let mut dark = None;
        let mut over = |css: &serde_json::Value| {
            if let Some(v) = css.get("theme").and_then(|s| s.get(name)) {
                light = Some(v.clone());
                dark = Some(v.clone());
            }
            if let Some(v) = css.get("light").and_then(|s| s.get(name)) {
                light = Some(v.clone());
            }
            if let Some(v) = css.get("dark").and_then(|s| s.get(name)) {
                dark = Some(v.clone());
            }
        };
        over(&self.0["cssVars"]);
        for (modifier, m) in self.0["modifiers"].as_object().unwrap() {
            let context = picked
                .iter()
                .find(|(n, _)| n == modifier)
                .map(|(_, c)| (*c).to_owned())
                .unwrap_or_else(|| m["default"].as_str().unwrap().to_owned());
            over(&m["contexts"][context.as_str()]);
        }
        (light.expect(name), dark.expect(name))
    }

    /// `name`'s color in Light and Dark.
    pub fn color(&self, name: &str, picked: &[(&str, &str)]) -> (nsh_core::Color, nsh_core::Color) {
        let (light, dark) = self.value(name, picked);
        (hex(light.as_str().unwrap()), hex(dark.as_str().unwrap()))
    }

    /// `name`'s number (a length in points, a weight).
    pub fn number(&self, name: &str, picked: &[(&str, &str)]) -> f32 {
        self.value(name, picked).0.as_f64().unwrap() as f32
    }
}

/// `"#rrggbbaa"` as a color.
pub fn hex(text: &str) -> nsh_core::Color {
    let value = u32::from_str_radix(text.trim_start_matches('#'), 16).unwrap();
    nsh_core::Color::from(nsh_core::color::rgba(value))
}

/// Each declared token of `tokens` whose value is not the document's with `picked`, read by its
/// kind (colors in both appearances, numbers in Light; a shadow only has to be there).
pub fn mismatches(
    tokens: &nsh_core::element::Tokens,
    doc: &Document,
    picked: &[(&str, &str)],
) -> Vec<String> {
    use nsh_core::element::{AdaptiveColor, AdaptiveShadow, SharedString};
    use nsh_core::geometry::Points;
    let mut out = Vec::new();
    for (name, kind) in doc.tokens() {
        let wrong = match kind.as_str() {
            "color" => match tokens.get_as::<AdaptiveColor>(&name) {
                Ok(c) => {
                    (
                        nsh_core::Color::from(c.light),
                        nsh_core::Color::from(c.dark),
                    ) != doc.color(&name, picked)
                }
                Err(_) => true,
            },
            "number" | "weight" => tokens
                .get_as::<f32>(&name)
                .map_or(true, |v| v != doc.number(&name, picked)),
            "font" => tokens.get_as::<SharedString>(&name).map_or(true, |v| {
                doc.value(&name, picked).0.as_str() != Some(v.as_ref())
            }),
            "shadow" => tokens.get_as::<AdaptiveShadow>(&name).is_err(),
            _ => tokens
                .get_as::<Points>(&name)
                .map_or(true, |v| v.0 != doc.number(&name, picked)),
        };
        if wrong {
            out.push(name);
        }
    }
    out
}
