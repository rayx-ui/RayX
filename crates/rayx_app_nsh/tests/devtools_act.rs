//! Act, input, wait and screenshot run through the dev session's driver (observe mode's test
//! agent, `nsh_core::inspect::DevDriver`; a fake one here) on the agent's worker thread, never on
//! the engine's or the main thread. An action names a node id; an id whose node is gone fails
//! with the subtree to look at again and takes no step. Each answers once its step is done (the
//! driver's steps wait for the screen to settle), with the tree's revision; a screenshot is the
//! surface's, or cropped to a node's bounds.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod support;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use base64::Engine as _;
use nsh_core::inspect::DevDriver;
use rayx_devtools_protocol::{NodeLine, TreeResult, codes};
use serde_json::{Value, json};
use support::app::{serial, start};
use support::hub::Hub;

/// The steps the fake driver took, with the thread each ran on.
static TAKEN: Mutex<Vec<(Value, String)>> = Mutex::new(Vec::new());

/// A 40 × 80 picture, red, as the fake driver's screenshot.
fn picture() -> String {
    let mut png = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut png, 40, 80);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer
            .write_image_data(&[255, 0, 0, 255].repeat(40 * 80))
            .unwrap();
    }
    base64::engine::general_purpose::STANDARD.encode(png)
}

struct Fake;

impl DevDriver for Fake {
    fn step(&self, step: Value) -> Value {
        let thread = std::thread::current().name().unwrap_or("").to_owned();
        TAKEN.lock().unwrap().push((step.clone(), thread));
        match step["step"].as_str() {
            Some("screenshot") => json!({ "outcome": "done", "value": picture() }),
            Some("hover") => json!({ "outcome": "failed", "message": "nothing to hover there" }),
            _ => json!({ "outcome": "done", "value": null }),
        }
    }
}

fn line<'a>(tree: &'a TreeResult, test_id: &str) -> &'a NodeLine {
    tree.nodes
        .iter()
        .find(|line| line.test_id.as_deref() == Some(test_id))
        .unwrap()
}

#[test]
fn steps_run_on_the_worker_through_the_driver() {
    let _serial = serial();
    let mut hub = Hub::listen();
    let (engine, dir) = start(&hub);
    hub.accept(Duration::from_secs(20));
    // The host's dev session hands its driver over (observe mode), here a fake one. (Handed
    // before the app is ready, it would start the agent on the hub's default URL by itself.)
    nsh_core::inspect::hand_driver(Arc::new(Fake));
    let tree: TreeResult = serde_json::from_value(hub.ask("tree", json!({})).unwrap()).unwrap();
    let save = line(&tree, "save").id;
    let taken = || TAKEN.lock().unwrap().clone();

    // Act, input and wait: one step each, by node id, answered with a revision.
    let done = hub
        .ask("act", json!({ "action": "press", "node": save }))
        .unwrap();
    assert!(done["revision"].as_u64().unwrap() >= tree.revision);
    hub.ask(
        "act",
        json!({ "action": "scroll", "node": save, "distance": 120.0 }),
    )
    .unwrap();
    hub.ask(
        "input",
        json!({ "input": "type", "node": save, "text": "Groceries" }),
    )
    .unwrap();
    hub.ask("input", json!({ "input": "key", "keys": "mod+s" }))
        .unwrap();
    hub.ask(
        "wait",
        json!({ "for": "text", "text": "Saved", "timeoutMs": 2000 }),
    )
    .unwrap();
    let steps: Vec<Value> = taken().into_iter().map(|(step, _)| step).collect();
    let view = format!("view:{save}");
    assert_eq!(
        steps,
        [
            json!({ "step": "press", "selector": view }),
            json!({ "step": "scroll", "selector": view, "distance": 120.0 }),
            json!({ "step": "type", "selector": view, "text": "Groceries" }),
            json!({ "step": "press_key", "keys": "mod+s" }),
            json!({ "step": "wait_for", "selector": "text:Saved", "timeout_ms": 2000, "gone": false }),
        ]
    );
    // On the agent's worker: never the engine's thread or the main one.
    assert!(
        taken()
            .iter()
            .all(|(_, thread)| thread == "rayx-devtools-act"),
        "{:?}",
        taken()
    );

    // A failed step is the request's error.
    let failed = hub
        .ask("act", json!({ "action": "hover", "node": save }))
        .unwrap_err();
    assert_eq!(failed.code, codes::FAILED);
    assert_eq!(failed.message, "nothing to hover there");

    // A node that is gone: the error names a subtree to look at again, and nothing is taken.
    let before = taken().len();
    let gone = hub
        .ask("act", json!({ "action": "press", "node": 999_999 }))
        .unwrap_err();
    assert_eq!(gone.code, codes::STALE_NODE);
    assert!(
        gone.message.contains("take a new snapshot"),
        "{}",
        gone.message
    );
    assert_eq!(taken().len(), before);

    // Screenshots: the whole surface, and cropped to the save button.
    let whole = hub.ask("screenshot", json!({})).unwrap();
    assert_eq!(
        (
            whole["mime"].as_str(),
            whole["width"].as_u64(),
            whole["height"].as_u64()
        ),
        (Some("image/png"), Some(40), Some(80))
    );
    let cropped = hub.ask("screenshot", json!({ "node": save })).unwrap();
    let (width, height) = (
        cropped["width"].as_u64().unwrap(),
        cropped["height"].as_u64().unwrap(),
    );
    // The picture is 40 px for the surface's 390 points: the button's frame, scaled.
    let frame = line(&tree, "save").frame;
    let scale = 40.0 / 390.0;
    let span = |from: f32, size: f32, max: f32| {
        let edge = |at: f32| (at * scale).round().clamp(0.0, max);
        (edge(from + size) - edge(from)).max(1.0) as u64
    };
    assert_eq!(width, span(frame.x, frame.width, 40.0), "{cropped}");
    assert_eq!(height, span(frame.y, frame.height, 80.0), "{cropped}");
    assert!(height < 80);
    let png = base64::engine::general_purpose::STANDARD
        .decode(cropped["data"].as_str().unwrap())
        .unwrap();
    let info = png::Decoder::new(std::io::Cursor::new(png))
        .read_info()
        .unwrap();
    assert_eq!(
        (u64::from(info.info().width), u64::from(info.info().height)),
        (width, height)
    );

    drop(engine);
    let _ = std::fs::remove_dir_all(dir);
}
