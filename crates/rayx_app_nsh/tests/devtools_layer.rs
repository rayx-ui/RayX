//! The dev tools' layer on the headless engine: one element puts it in a dev build; the hub turns
//! pick mode on and off; in pick mode a full-surface view over the app takes the pointer, a press
//! reports the innermost node under it and a modifier press the next one outward; the selected
//! node is outlined; the toolbar asks for pick mode, opens the note box and steps outward. The
//! layer keeps no tool state of its own, and its subtree, from its host down, is in no answer of
//! the agent. It covers the surface while taking no place in its parent's layout.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod support;

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, Sender};
use nsh_core::{
    ApproxTextEngine, EventKind, EventMask, EventPayload, MountBatch, MountItem, PressEvent, Size,
    prelude::*,
};
use nsh_engine::{App, Engine, EngineConfig, MountSink};
use rayx_app_nsh::{
    AppState, DevToolsOptions, RayXAppBuilder, StartAppBuilderExt as _, app_state, start_devtools,
};
use serde_json::json;
use support::hub::Hub;

static URL: Mutex<String> = Mutex::new(String::new());
/// Whether the root mounts the layer.
static WITH_LAYER: Mutex<bool> = Mutex::new(true);

fn build_app(builder: RayXAppBuilder) -> RayXAppBuilder {
    builder
}

#[component]
fn Root() -> impl IntoElement {
    build_app(::rayx_app::testing::add_host_fakes(RayXAppBuilder::new())).start();
    effect(move || {
        if app_state() == AppState::Ready {
            start_devtools(DevToolsOptions {
                url: Some(URL.lock().unwrap().clone()),
                credential: None,
            });
        }
    });
    // A root that centres with a gap: an in-flow layer would move the save button.
    view()
        .flex_1()
        .gap_4()
        .justify_center()
        .items_center()
        .child(
            view()
                .test_id("save")
                .p_3()
                .on_press(|_: &PressEvent| {})
                .child(text("Save")),
        )
        .child(WITH_LAYER.lock().unwrap().then(rayx_app_nsh::dev_layer))
}

struct Sink(Sender<MountBatch>);

impl MountSink for Sink {
    fn submit(&self, batch: MountBatch) {
        let _ = self.0.send(batch);
    }
}

/// What the mount batches built: each view's events, texts, parents and whether it is there.
#[derive(Default)]
struct Screen {
    events: HashMap<u32, EventMask>,
    texts: HashMap<u32, String>,
    parents: HashMap<u32, u32>,
    gone: HashSet<u32>,
    created: usize,
    frames: HashMap<u32, Bounds<Points>>,
}

impl Screen {
    fn take(&mut self, batch: &MountBatch) {
        for item in &batch.items {
            match item {
                MountItem::Create { id, .. } => {
                    self.created += 1;
                    self.gone.remove(id);
                }
                MountItem::Events { id, events } => {
                    self.events.insert(*id, *events);
                }
                MountItem::Text { id, text, .. } => {
                    self.texts.insert(*id, text.text.to_string());
                }
                MountItem::Insert { parent, child, .. } => {
                    self.parents.insert(*child, *parent);
                }
                MountItem::Delete { id } => {
                    self.gone.insert(*id);
                }
                MountItem::Frame { id, frame } => {
                    self.frames.insert(*id, *frame);
                }
                _ => {}
            }
        }
    }

    /// The pick surface: the view listening to the mouse's moves.
    fn surface(&self) -> Option<u32> {
        self.events
            .iter()
            .find(|(id, events)| events.contains(EventKind::MouseMove) && !self.gone.contains(id))
            .map(|(id, _)| *id)
    }

    /// View `id`'s frame on the surface: `(x, y, width, height)`.
    fn page(&self, id: u32) -> (f32, f32, f32, f32) {
        let frame = self.frames.get(&id).copied().unwrap_or_default();
        let (mut x, mut y) = (f32::from(frame.origin.x), f32::from(frame.origin.y));
        let mut at = id;
        while let Some(parent) = self.parents.get(&at) {
            if let Some(above) = self.frames.get(parent) {
                x += f32::from(above.origin.x);
                y += f32::from(above.origin.y);
            }
            at = *parent;
        }
        (
            x,
            y,
            f32::from(frame.size.width),
            f32::from(frame.size.height),
        )
    }

    /// The view whose text reads `label`.
    fn text(&self, label: &str) -> u32 {
        self.texts
            .iter()
            .find(|(_, text)| text.as_str() == label)
            .map(|(id, _)| *id)
            .unwrap()
    }

    /// `id` and every view above it.
    fn ancestors(&self, id: u32) -> Vec<u32> {
        let mut chain = vec![id];
        while let Some(parent) = self.parents.get(chain.last().unwrap()) {
            chain.push(*parent);
        }
        chain
    }

    /// The toolbar's button labelled `label`.
    fn button(&self, label: &str) -> u32 {
        let text = self
            .texts
            .iter()
            .find(|(_, text)| text.as_str() == label)
            .map(|(id, _)| *id)
            .unwrap();
        self.parents[&text]
    }
}

/// Pumps the batches into `screen` until `done` says so.
fn until(rx: &Receiver<MountBatch>, screen: &Arc<Mutex<Screen>>, done: impl Fn(&Screen) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !done(&screen.lock().unwrap()) {
        assert!(Instant::now() < deadline, "the screen never got there");
        if let Ok(batch) = rx.recv_timeout(Duration::from_millis(20)) {
            screen.lock().unwrap().take(&batch);
        }
    }
}

fn press_at(engine: &Engine, id: u32, x: f32, y: f32, shift: bool) {
    let mut press = PressEvent {
        position: Point {
            x: px(0.0),
            y: px(0.0),
        },
        page_position: Point { x: px(x), y: px(y) },
        modifiers: Default::default(),
        clicks: 1,
    };
    press.modifiers.shift = shift;
    engine
        .control()
        .dispatch_event(id, EventKind::Press, EventPayload::Press(press));
}

fn picked(hub: &mut Hub, from: usize) -> Option<u64> {
    hub.listen_for(Duration::from_millis(400));
    hub.notifications[from..]
        .iter()
        .rev()
        .find(|(method, _)| method == "layer.picked")
        .map(|(_, params)| params["node"].as_u64().unwrap())
}

/// Starts the app on a 390 × 844 surface; its batches go to the receiver.
fn start_engine() -> (Engine, Receiver<MountBatch>) {
    let mut config = EngineConfig::new(App::Rust(nsh_core::element::Root::of::<Root>()));
    config.viewport = Size::new(px(390.0), px(844.0));
    let (tx, rx) = crossbeam_channel::unbounded();
    let engine = Engine::start(config, || ApproxTextEngine, Sink(tx)).unwrap();
    (engine, rx)
}

fn serial() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[test]
fn pick_mode_takes_the_pointer_and_reports_what_it_points_at() {
    let _serial = serial();
    *WITH_LAYER.lock().unwrap() = true;
    let mut hub = Hub::listen();
    *URL.lock().unwrap() = hub.url();
    let (engine, rx) = start_engine();
    let screen = Arc::new(Mutex::new(Screen::default()));
    hub.accept(Duration::from_secs(20));
    until(&rx, &screen, |screen| {
        screen.texts.values().any(|t| t == "Pick")
    });

    // The layer is in no answer, from its host down: no overlay, nothing beside the save button
    // under the root; and pick mode is off: nothing takes the pointer.
    let tree = hub.ask("tree", json!({})).unwrap();
    let lines = tree["nodes"].as_array().unwrap();
    assert!(
        lines.iter().all(|line| line["text"] != "Pick"
            && line["testId"] != "rayx-devtools-layer"
            && line["kind"] != "overlay"),
        "{lines:#?}"
    );
    let layer_ids: HashSet<u64> = {
        let screen = screen.lock().unwrap();
        screen
            .ancestors(screen.text("Pick"))
            .into_iter()
            .filter(|id| !screen.ancestors(screen.text("Save")).contains(id))
            .map(u64::from)
            .collect()
    };
    assert!(
        lines
            .iter()
            .all(|line| !layer_ids.contains(&line["id"].as_u64().unwrap())),
        "the layer's host or overlay is in the tree: {layer_ids:?} in {lines:#?}"
    );
    let overlays = hub.ask("find", json!({ "kind": "overlay" })).unwrap();
    assert_eq!(overlays["nodes"], json!([]), "{overlays}");
    let save = lines.iter().find(|line| line["testId"] == "save").unwrap();
    let frame = &save["frame"];
    let (x, y) = (
        frame["x"].as_f64().unwrap() as f32 + 4.0,
        frame["y"].as_f64().unwrap() as f32 + 4.0,
    );
    assert_eq!(screen.lock().unwrap().surface(), None);

    // The toolbar is on the surface, inside its bounds.
    {
        let screen = screen.lock().unwrap();
        let toolbar = screen.parents[&screen.button("Pick")];
        let (tx, ty, tw, th) = screen.page(toolbar);
        assert!(
            tx >= 0.0 && ty >= 0.0 && tx + tw <= 390.0 && ty + th <= 844.0 && tw > 0.0,
            "the toolbar is off the surface: {:?}",
            (tx, ty, tw, th)
        );
    }

    // The hub turns pick mode on: a surface over the whole app takes the pointer, the save
    // button's point included.
    hub.ask("layer.pickMode", json!({ "on": true })).unwrap();
    until(&rx, &screen, |screen| screen.surface().is_some());
    let surface = screen.lock().unwrap().surface().unwrap();
    until(&rx, &screen, |screen| screen.page(surface).2 > 0.0);
    assert_eq!(
        screen.lock().unwrap().page(surface),
        (0.0, 0.0, 390.0, 844.0),
        "the pick surface covers the surface"
    );

    // A press reports the innermost node under it, as `pick` lists them.
    let under: Vec<u64> = serde_json::from_value(
        hub.ask("pick", json!({ "x": x, "y": y })).unwrap()["nodes"].clone(),
    )
    .unwrap();
    assert!(under.len() >= 2, "{under:?}");
    assert!(
        under
            .iter()
            .all(|id| !layer_ids.contains(id) && *id != u64::from(surface)),
        "a layer node under the pointer: {under:?}"
    );
    let from = hub.notifications.len();
    press_at(&engine, surface, x, y, false);
    assert_eq!(picked(&mut hub, from), Some(under[0]));

    // The hub selects it; the layer outlines it.
    let created = screen.lock().unwrap().created;
    hub.ask("layer.select", json!({ "node": under[0] }))
        .unwrap();
    until(&rx, &screen, |screen| screen.created > created);

    // A press with a modifier steps outward from the selection.
    let from = hub.notifications.len();
    press_at(&engine, surface, x, y, true);
    assert_eq!(picked(&mut hub, from), Some(under[1]));

    // The toolbar: "Out" steps outward from the selection, "Pick" asks for pick mode off.
    let from = hub.notifications.len();
    let out = screen.lock().unwrap().button("Out");
    engine
        .control()
        .dispatch_event(out, EventKind::Press, EventPayload::None);
    assert_eq!(picked(&mut hub, from), Some(under[1]));
    let from = hub.notifications.len();
    let pick = screen.lock().unwrap().button("Pick");
    engine
        .control()
        .dispatch_event(pick, EventKind::Press, EventPayload::None);
    hub.listen_for(Duration::from_millis(400));
    assert!(
        hub.notifications[from..]
            .iter()
            .any(|(method, params)| method == "layer.pickModeRequested" && params["on"] == false),
        "{:?}",
        hub.notifications
    );
    // The layer keeps no state of its own: pick mode stays on until the hub turns it off.
    assert_eq!(screen.lock().unwrap().surface(), Some(surface));
    hub.ask("layer.pickMode", json!({ "on": false })).unwrap();
    until(&rx, &screen, |screen| screen.surface().is_none());

    // The note box: open it, and a note reaches the hub about the selection.
    let note = screen.lock().unwrap().button("Note");
    engine
        .control()
        .dispatch_event(note, EventKind::Press, EventPayload::None);
    until(&rx, &screen, |screen| {
        screen.texts.values().any(|t| t == "Send")
    });

    drop(engine);
}

#[test]
fn the_layer_takes_no_place_in_its_parent_s_layout() {
    let _serial = serial();
    *URL.lock().unwrap() = "ws://127.0.0.1:9".to_owned();
    // The save button's frame, without the layer and with it, in a root that centres with a gap.
    let save_frame = |with_layer: bool| {
        *WITH_LAYER.lock().unwrap() = with_layer;
        let (engine, rx) = start_engine();
        let screen = Arc::new(Mutex::new(Screen::default()));
        until(&rx, &screen, |screen| {
            screen.texts.values().any(|t| t == "Save")
                && (!with_layer || screen.texts.values().any(|t| t == "Pick"))
        });
        // Let layout settle.
        let deadline = Instant::now() + Duration::from_millis(500);
        while Instant::now() < deadline {
            if let Ok(batch) = rx.recv_timeout(Duration::from_millis(20)) {
                screen.lock().unwrap().take(&batch);
            }
        }
        let screen = screen.lock().unwrap();
        let save = screen.parents[&screen.text("Save")];
        let frame = screen.page(save);
        drop(engine);
        frame
    };
    let without = save_frame(false);
    let with = save_frame(true);
    assert_eq!(with, without, "the layer moved the app");
}
