//! A headless engine and the tree it mounts, for the component tests: texts, frames, styles,
//! the events each view hears, and presses by label.

#![allow(dead_code, clippy::expect_used, clippy::unwrap_used)]

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, Sender};
use nsh_core::ApproxTextEngine;
use nsh_core::ViewStyle;
use nsh_core::color::{Hsla, Rgba};
use nsh_core::element::description::SetDescription;
use nsh_core::element::{Registry, RoleSpec, Tokens};
use nsh_core::{EventKind, EventMask, EventPayload, MountBatch, MountItem, NodeId};
use nsh_engine::{App, EngineConfig, LocalEngine, MountSink};

pub struct ChannelSink(pub Sender<MountBatch>);

impl MountSink for ChannelSink {
    fn submit(&self, batch: MountBatch) {
        let _ = self.0.send(batch);
    }
}

/// The UI sets of an app whose set `test/native` serves RayX's library (`rayx/components`), as
/// `#[rayx::native_module]` does.
pub fn ui_sets() -> Vec<Registry> {
    let mut set = Registry::ui_set("test/native");
    set.serve(rayx_components::set());
    vec![nsh_base::registry(), set]
}

/// The sets a host of [`ui_sets`] serves, the library's among them, described: what the bundler
/// and the compiler take.
pub fn described() -> Vec<SetDescription> {
    Registry::served_sets(&ui_sets())
        .into_iter()
        .map(SetDescription::from)
        .collect()
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

/// RayX's theme document, read (`nsh_theme::read`), and what reading it said.
pub fn document() -> (Arc<nsh_theme::ThemeDocument>, Vec<nsh_theme::Diagnostic>) {
    let read = nsh_theme::read(rayx_components::theme::DOCUMENT);
    (Arc::new(read.document), read.diagnostics)
}

/// The roles the app's UI sets ([`ui_sets`]) register, merged: nsh-base's and RayX's.
pub fn declarations() -> Vec<RoleSpec> {
    Registry::merged(&ui_sets()).role_specs().to_vec()
}

/// The tokens of `document` with the contexts `picks` (`("accent", "sky")`, every other modifier
/// at its default), its roles resolved as a core serving [`ui_sets`] resolves them, with the
/// diagnostics of resolving and of checking its roles.
pub fn tokens_of(
    document: &Arc<nsh_theme::ThemeDocument>,
    picks: &[(&str, &str)],
) -> (Tokens, Vec<nsh_theme::Diagnostic>) {
    let mut selection = nsh_theme::Selection::new();
    for (modifier, context) in picks {
        selection = selection.with(*modifier, *context);
    }
    Tokens::resolve_with_roles(document.clone(), selection, Vec::new(), &declarations())
}

/// A color as `0xRRGGBBAA`.
pub fn rgba(color: Hsla) -> u32 {
    u32::from(Rgba::from(color))
}

/// `"#rrggbb"` or `"#rrggbbaa"` as `0xRRGGBBAA`.
pub fn hex(text: &str) -> u32 {
    let digits = text.trim_start_matches('#');
    let value = u32::from_str_radix(digits, 16).unwrap();
    if digits.len() == 6 {
        (value << 8) | 0xff
    } else {
        value
    }
}

/// Whether two colors are within one step of each other on every 8-bit channel.
pub fn near(a: u32, b: u32) -> bool {
    (0..4).all(|i| {
        let shift = i * 8;
        (((a >> shift) & 0xff) as i32 - ((b >> shift) & 0xff) as i32).abs() <= 1
    })
}
