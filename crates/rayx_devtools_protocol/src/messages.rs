//! The messages' parameters and results. Fields are camelCase on the wire; an absent optional
//! field is left out, and a peer ignores fields it does not know.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::Version;

fn is_false(value: &bool) -> bool {
    !*value
}

/// A node's id: the engine's, exact for as long as the node lives in one engine run.
pub type NodeId = u64;

/// A rectangle in points.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// A size in points.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Size {
    pub width: f32,
    pub height: f32,
}

/// The four sides of a box, in points.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Edges {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

/// A surface the app shows (a window, or the page on the web).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceInfo {
    pub id: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub size: Size,
}

/// A surface by id (`surface.closed`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SurfaceRef {
    pub id: u64,
}

/// The app's `hello`: who it is, where it runs, and which build.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Hello {
    pub protocol_version: Version,
    /// The app's id (its bundle identifier).
    pub app_id: String,
    pub app_name: String,
    /// `macos`, `windows`, `linux`, `ios`, `android`, `web`, …
    pub platform: String,
    /// Changes with every engine start, so a hub tells a restarted app from the one before.
    pub build_id: String,
    pub surfaces: Vec<SurfaceInfo>,
    /// The directory of the app's `nsh.json` as the dev host knows it; `None` when it does not.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root: Option<String>,
    /// An opaque string the hub may ask for; empty, and left out, by default.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub credential: String,
}

/// The hub's answer to `hello`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Welcome {
    pub protocol_version: Version,
}

/// The answer of a request that returns nothing but its tree's revision after it ran.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Done {
    pub revision: u64,
}

/// `tree`: the tree, or the subtree at `root`, bounded.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TreeParams {
    /// The subtree's root; `None` for the whole tree.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root: Option<NodeId>,
    /// How many levels below the root; `None` for all.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub depth: Option<u32>,
    /// Only nodes a person can act on (and the ones holding them).
    #[serde(default, skip_serializing_if = "is_false")]
    pub interactive_only: bool,
    /// At most this many characters of node lines (their JSON); the rest is cut, `truncated`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_chars: Option<u32>,
}

/// Where an element or a component use was written, as the source says now.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceStamp {
    /// The module's path in the app.
    pub module: String,
    /// The named statement around it (a component, a function).
    pub anchor: String,
    pub line: u32,
    pub column: u32,
}

/// A component instance that rendered a node.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComponentRef {
    /// The instance's id in this engine run.
    pub id: u32,
    /// The component's name.
    pub name: String,
    /// Where it was used.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub call_site: Option<SourceStamp>,
}

/// One node as a tree, find or node answer lists it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeLine {
    pub id: NodeId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<NodeId>,
    /// Levels below the answer's root.
    pub depth: u32,
    /// The core's kind (`view`, `text`, `input`, `scroll`, …), or `instance` for a component
    /// instance's pseudo-node.
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub test_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<SourceStamp>,
    /// The component instances that rendered it, innermost first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub components: Vec<ComponentRef>,
    /// Where it is on its surface.
    #[serde(default)]
    pub frame: Rect,
    /// Its marked states: `hidden`, `culled`, `overlay`, `overridden`, …
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub flags: Vec<String>,
}

/// The answer of `tree`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TreeResult {
    /// Increases with every commit that changed the tree.
    pub revision: u64,
    pub root: NodeId,
    /// The nodes in tree order.
    pub nodes: Vec<NodeLine>,
    /// Whether the bounds cut nodes off.
    #[serde(default, skip_serializing_if = "is_false")]
    pub truncated: bool,
}

/// `tree.watch`: notify `tree.changed` after every commit that changes the tree, or stop.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WatchParams {
    pub on: bool,
}

/// `tree.changed`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TreeChanged {
    pub revision: u64,
}

/// `node`: one node in full.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeParams {
    pub id: NodeId,
}

/// A node's box from its last layout.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct BoxModel {
    pub frame: Rect,
    pub padding: Edges,
    pub border: Edges,
    pub margin: Edges,
}

/// One style declaration.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StyleEntry {
    pub name: String,
    pub value: String,
}

/// The answer of `node`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeResult {
    pub revision: u64,
    pub node: NodeLine,
    pub layout: BoxModel,
    /// Its class names as authored.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub class: Option<String>,
    /// Its inline style declarations.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub style: Vec<StyleEntry>,
    /// Its accessibility label.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// The events it listens to (`press`, `longPress`, `hover`, `scroll`, …).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub events: Vec<String>,
}

/// `find`: the nodes that match every given field.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FindParams {
    /// A text the node reads, or that its subtree reads, as a substring.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub test_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// At most this many; `None` for the protocol's default of 50.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
}

/// The answer of `find`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FindResult {
    pub revision: u64,
    pub nodes: Vec<NodeLine>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub truncated: bool,
}

/// `pick`: the nodes under a point of a surface, in points.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PickParams {
    pub x: f32,
    pub y: f32,
}

/// The answer of `pick`: innermost and topmost first.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PickResult {
    pub revision: u64,
    pub nodes: Vec<NodeId>,
}

/// `act`: what to do to a node. Each waits for the screen to settle before it answers.
///
/// On the wire one object, `action` first (`{"action":"scroll","node":30,"distance":240.0}`),
/// read through [`ActWire`] so that it decodes whatever serde_json features a build turns on.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "ActWire", into = "ActWire")]
pub enum Act {
    Press {
        node: NodeId,
    },
    LongPress {
        node: NodeId,
    },
    Hover {
        node: NodeId,
    },
    /// Scrolls the node's scroll view by `distance` points (positive: towards the end).
    Scroll {
        node: NodeId,
        distance: f32,
    },
    /// Drags the node by `x`, `y` points.
    Drag {
        node: NodeId,
        x: f32,
        y: f32,
    },
}

/// [`Act`] as written.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ActWire {
    action: String,
    node: NodeId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    distance: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    x: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    y: Option<f32>,
}

impl TryFrom<ActWire> for Act {
    type Error = String;

    fn try_from(wire: ActWire) -> Result<Self, String> {
        let node = wire.node;
        let need = |value: Option<f32>, name: &str| {
            value.ok_or_else(|| format!("`{}` needs `{name}`", wire.action))
        };
        Ok(match wire.action.as_str() {
            "press" => Act::Press { node },
            "longPress" => Act::LongPress { node },
            "hover" => Act::Hover { node },
            "scroll" => Act::Scroll {
                node,
                distance: need(wire.distance, "distance")?,
            },
            "drag" => Act::Drag {
                node,
                x: need(wire.x, "x")?,
                y: need(wire.y, "y")?,
            },
            other => return Err(format!("no action `{other}`")),
        })
    }
}

impl From<Act> for ActWire {
    fn from(act: Act) -> Self {
        let wire = |action: &str, node| ActWire {
            action: action.to_owned(),
            node,
            distance: None,
            x: None,
            y: None,
        };
        match act {
            Act::Press { node } => wire("press", node),
            Act::LongPress { node } => wire("longPress", node),
            Act::Hover { node } => wire("hover", node),
            Act::Scroll { node, distance } => ActWire {
                distance: Some(distance),
                ..wire("scroll", node)
            },
            Act::Drag { node, x, y } => ActWire {
                x: Some(x),
                y: Some(y),
                ..wire("drag", node)
            },
        }
    }
}

impl Act {
    /// The node it acts on.
    pub fn node(&self) -> NodeId {
        match *self {
            Act::Press { node }
            | Act::LongPress { node }
            | Act::Hover { node }
            | Act::Scroll { node, .. }
            | Act::Drag { node, .. } => node,
        }
    }
}

/// `input`: text and keys. Each waits for the screen to settle before it answers. On the wire
/// one object, `input` first, read through [`InputWire`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "InputWire", into = "InputWire")]
pub enum Input {
    /// Types `text` into the input `node`.
    Type { node: NodeId, text: String },
    /// Submits the input `node` (its return key).
    Submit { node: NodeId },
    /// Empties the input `node`.
    Clear { node: NodeId },
    /// Presses a keystroke with its modifiers: `"mod+k"`, `"escape"`.
    Key { keys: String },
}

/// [`Input`] as written.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InputWire {
    input: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    node: Option<NodeId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    keys: Option<String>,
}

impl TryFrom<InputWire> for Input {
    type Error = String;

    fn try_from(wire: InputWire) -> Result<Self, String> {
        let node = || {
            wire.node
                .ok_or_else(|| format!("`{}` needs `node`", wire.input))
        };
        Ok(match wire.input.as_str() {
            "type" => Input::Type {
                node: node()?,
                text: wire.text.clone().ok_or("`type` needs `text`")?,
            },
            "submit" => Input::Submit { node: node()? },
            "clear" => Input::Clear { node: node()? },
            "key" => Input::Key {
                keys: wire.keys.clone().ok_or("`key` needs `keys`")?,
            },
            other => return Err(format!("no input `{other}`")),
        })
    }
}

impl From<Input> for InputWire {
    fn from(input: Input) -> Self {
        let wire = |name: &str| InputWire {
            input: name.to_owned(),
            node: None,
            text: None,
            keys: None,
        };
        match input {
            Input::Type { node, text } => InputWire {
                node: Some(node),
                text: Some(text),
                ..wire("type")
            },
            Input::Submit { node } => InputWire {
                node: Some(node),
                ..wire("submit")
            },
            Input::Clear { node } => InputWire {
                node: Some(node),
                ..wire("clear")
            },
            Input::Key { keys } => InputWire {
                keys: Some(keys),
                ..wire("key")
            },
        }
    }
}

/// `wait`: until what, at most how long. On the wire one object, `for` first
/// (`{"for":"text","text":"Saved","timeoutMs":2000}`), read through [`WaitWire`].
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "WaitWire", into = "WaitWire")]
pub struct WaitParams {
    pub until: WaitFor,
    /// `None` for the protocol's default of 5000.
    pub timeout_ms: Option<u64>,
}

/// What `wait` waits for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WaitFor {
    /// A text on screen.
    Text { text: String },
    /// A node that matches a test id.
    TestId { test_id: String },
    /// Node `node`'s end.
    Gone { node: NodeId },
    /// The app quiet: nothing pending, no frame scheduled.
    Idle,
}

/// [`WaitParams`] as written.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WaitWire {
    #[serde(rename = "for")]
    until: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    test_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    node: Option<NodeId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    timeout_ms: Option<u64>,
}

impl TryFrom<WaitWire> for WaitParams {
    type Error = String;

    fn try_from(wire: WaitWire) -> Result<Self, String> {
        let until = match wire.until.as_str() {
            "text" => WaitFor::Text {
                text: wire.text.ok_or("`text` needs `text`")?,
            },
            "testId" => WaitFor::TestId {
                test_id: wire.test_id.ok_or("`testId` needs `testId`")?,
            },
            "gone" => WaitFor::Gone {
                node: wire.node.ok_or("`gone` needs `node`")?,
            },
            "idle" => WaitFor::Idle,
            other => return Err(format!("no wait for `{other}`")),
        };
        Ok(WaitParams {
            until,
            timeout_ms: wire.timeout_ms,
        })
    }
}

impl From<WaitParams> for WaitWire {
    fn from(wait: WaitParams) -> Self {
        let mut wire = WaitWire {
            until: String::new(),
            text: None,
            test_id: None,
            node: None,
            timeout_ms: wait.timeout_ms,
        };
        wire.until = match wait.until {
            WaitFor::Text { text } => {
                wire.text = Some(text);
                "text"
            }
            WaitFor::TestId { test_id } => {
                wire.test_id = Some(test_id);
                "testId"
            }
            WaitFor::Gone { node } => {
                wire.node = Some(node);
                "gone"
            }
            WaitFor::Idle => "idle",
        }
        .to_owned();
        wire
    }
}

/// `screenshot`: the whole surface, or cropped to a node's bounds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScreenshotParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node: Option<NodeId>,
}

/// A picture.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Image {
    /// `image/png`.
    pub mime: String,
    pub width: u32,
    pub height: u32,
    /// The bytes, base64.
    pub data: String,
}

/// `layer.pickMode` (hub to app) and `layer.pickModeRequested` (the layer's toolbar to the hub).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PickMode {
    pub on: bool,
}

/// `layer.select`: the node the layer outlines as selected; `None` for none.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Selection {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node: Option<NodeId>,
}

/// `layer.picked`: the node a press in pick mode chose, or the one stepping outward reached.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Picked {
    pub node: NodeId,
    pub revision: u64,
}

/// `layer.note`: a note written in the layer's note box, about a node or none.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Note {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node: Option<NodeId>,
    pub text: String,
}

/// `logs`: the app's log lines after sequence number `after`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogsParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
}

/// A log line.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogEntry {
    /// Its sequence number in this engine run.
    pub seq: u64,
    /// `error`, `warn`, `info`, `debug`, `trace`.
    pub level: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub target: String,
    pub message: String,
}

/// The answer of `logs`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogsResult {
    pub entries: Vec<LogEntry>,
    /// How many lines the app no longer keeps before the first one here.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub dropped: u64,
}

fn is_zero(value: &u64) -> bool {
    *value == 0
}

/// A tool the app registered.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolInfo {
    pub name: String,
    pub description: String,
    /// The JSON Schema of its arguments.
    pub input_schema: Value,
}

/// The answer of `extension.list`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ExtensionList {
    pub tools: Vec<ToolInfo>,
}

/// `extension.call`: run tool `name` with `arguments`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ExtensionCall {
    pub name: String,
    #[serde(default)]
    pub arguments: Value,
}

/// The answer of `extension.call`: what the tool returned.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ExtensionResult {
    pub content: Value,
}

/// An empty parameter object (`extension.list`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Empty {}
