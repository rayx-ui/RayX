//! Act, input, wait and screenshot: the hub's requests that drive the app, natively, through
//! the dev session's driver (observe mode's test agent, `nsh_core::inspect::DevDriver`).
//!
//! The driver blocks on the host's main thread, so its steps never run on the engine's or the
//! main thread: the agent checks on the engine's thread that the node is still there (a node
//! that is gone fails with the subtree to look at again), hands the step to a worker thread of
//! its own, and answers on the engine's thread with the tree's revision once the step is done
//! and the screen settled (the driver's step waits for that).
//!
//! A node is named to the driver as `view:<id>`, its node id.

use std::sync::Arc;
use std::sync::mpsc::{Sender, channel};

use nsh_core::inspect::DevDriver;
use rayx_devtools_protocol::{self as proto, Act, Image, Input, RpcError, WaitFor};
use serde_json::{Value, json};

/// How long `wait` waits when the request says nothing.
const WAIT_MS: u64 = 5000;

/// A step for the worker: what to take, and what to do with its outcome on the engine's thread.
pub(crate) struct Job {
    pub(crate) step: Value,
    pub(crate) done: Box<dyn FnOnce(Result<Value, String>) + Send>,
}

/// The worker thread that takes the driver's steps, one at a time.
#[derive(Clone)]
pub(crate) struct Worker {
    jobs: Sender<Job>,
}

impl Worker {
    /// Starts the worker on `driver`.
    pub(crate) fn start(driver: Arc<dyn DevDriver>) -> Option<Worker> {
        let (jobs, queue) = channel::<Job>();
        std::thread::Builder::new()
            .name("rayx-devtools-act".into())
            .spawn(move || {
                while let Ok(job) = queue.recv() {
                    let outcome = driver.step(job.step);
                    (job.done)(outcome_of(outcome));
                }
            })
            .ok()?;
        Some(Worker { jobs })
    }

    /// Queues `job`.
    pub(crate) fn take(&self, job: Job) {
        let _ = self.jobs.send(job);
    }
}

/// A step's outcome (`{"outcome":"done","value":…}` or `{"outcome":"failed","message":…}`).
fn outcome_of(outcome: Value) -> Result<Value, String> {
    match outcome["outcome"].as_str() {
        Some("done") => Ok(outcome.get("value").cloned().unwrap_or(Value::Null)),
        _ => Err(outcome["message"]
            .as_str()
            .unwrap_or("the step failed")
            .to_owned()),
    }
}

fn view(node: u64) -> String {
    format!("view:{node}")
}

/// The driver's step for an action.
pub(crate) fn act_step(act: &Act) -> Value {
    match *act {
        Act::Press { node } => json!({ "step": "press", "selector": view(node) }),
        Act::LongPress { node } => json!({ "step": "long_press", "selector": view(node) }),
        Act::Hover { node } => json!({ "step": "hover", "selector": view(node) }),
        Act::Scroll { node, distance } => {
            json!({ "step": "scroll", "selector": view(node), "distance": distance })
        }
        Act::Drag { node, x, y } => {
            json!({ "step": "drag", "selector": view(node), "x": x, "y": y })
        }
    }
}

/// The driver's step for an input, and the node it names (`None` for keys).
pub(crate) fn input_step(input: &Input) -> (Value, Option<u64>) {
    match input {
        Input::Type { node, text } => (
            json!({ "step": "type", "selector": view(*node), "text": text }),
            Some(*node),
        ),
        Input::Submit { node } => (
            json!({ "step": "submit", "selector": view(*node) }),
            Some(*node),
        ),
        Input::Clear { node } => (
            json!({ "step": "clear", "selector": view(*node) }),
            Some(*node),
        ),
        Input::Key { keys } => (json!({ "step": "press_key", "keys": keys }), None),
    }
}

/// The driver's step for a wait.
pub(crate) fn wait_step(wait: &proto::WaitParams) -> Value {
    let timeout_ms = wait.timeout_ms.unwrap_or(WAIT_MS);
    match &wait.until {
        WaitFor::Text { text } => json!({
            "step": "wait_for", "selector": format!("text:{text}"), "timeout_ms": timeout_ms, "gone": false
        }),
        WaitFor::TestId { test_id } => json!({
            "step": "wait_for", "selector": format!("id:{test_id}"), "timeout_ms": timeout_ms, "gone": false
        }),
        WaitFor::Gone { node } => json!({
            "step": "wait_for", "selector": view(*node), "timeout_ms": timeout_ms, "gone": true
        }),
        // The driver's every step waits for the app to be quiet; `ready` waits for nothing more.
        WaitFor::Idle => json!({ "step": "ready", "timeout_ms": timeout_ms }),
    }
}

/// The screenshot the driver took (base64 PNG), cropped to `crop` (points on the surface) when
/// given; `viewport` is the surface's size in points, which the picture's pixels scale.
pub(crate) fn image(
    value: &Value,
    crop: Option<proto::Rect>,
    viewport: proto::Size,
) -> Result<Image, RpcError> {
    use base64::Engine as _;
    let encoded = value
        .as_str()
        .ok_or_else(|| RpcError::failed("the driver took no picture"))?;
    let png = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|error| RpcError::failed(format!("the picture is not base64: {error}")))?;
    let decoder = png::Decoder::new(std::io::Cursor::new(&png));
    let mut reader = decoder
        .read_info()
        .map_err(|error| RpcError::failed(format!("the picture is not a PNG: {error}")))?;
    let (width, height) = (reader.info().width, reader.info().height);
    let Some(crop) = crop else {
        return Ok(Image {
            mime: "image/png".into(),
            width,
            height,
            data: encoded.to_owned(),
        });
    };
    let mut pixels = vec![0; reader.output_buffer_size()];
    let frame = reader
        .next_frame(&mut pixels)
        .map_err(|error| RpcError::failed(format!("the picture does not decode: {error}")))?;
    let channels = frame.color_type.samples();
    if frame.bit_depth != png::BitDepth::Eight {
        return Err(RpcError::failed("the picture is not 8 bits per sample"));
    }
    let scale = if viewport.width > 0.0 {
        width as f32 / viewport.width
    } else {
        1.0
    };
    let clamp = |value: f32, max: u32| (value * scale).round().clamp(0.0, max as f32) as u32;
    let (left, top) = (clamp(crop.x, width), clamp(crop.y, height));
    let right = clamp(crop.x + crop.width, width).max(left);
    let bottom = clamp(crop.y + crop.height, height).max(top);
    let (cut_width, cut_height) = ((right - left).max(1), (bottom - top).max(1));
    let mut cut = Vec::with_capacity((cut_width * cut_height) as usize * channels);
    for row in top..(top + cut_height).min(height) {
        let start = (row as usize * width as usize + left as usize) * channels;
        let end = start + (cut_width as usize).min((width - left) as usize) * channels;
        cut.extend_from_slice(&pixels[start..end]);
    }
    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, cut_width, cut_height);
        encoder.set_color(frame.color_type);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder
            .write_header()
            .map_err(|error| RpcError::failed(error.to_string()))?;
        writer
            .write_image_data(&cut)
            .map_err(|error| RpcError::failed(error.to_string()))?;
    }
    Ok(Image {
        mime: "image/png".into(),
        width: cut_width,
        height: cut_height,
        data: base64::engine::general_purpose::STANDARD.encode(out),
    })
}
