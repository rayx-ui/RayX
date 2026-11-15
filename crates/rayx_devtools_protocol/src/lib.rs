//! The agent protocol of RayX's dev tools: what an app's dev-build agent and a tool's hub say to
//! each other.
//!
//! JSON-RPC 2.0 over WebSocket text frames, one connection per app run, requests both ways. The
//! app dials out to the hub and opens with a `hello` request ([`Hello`]: its identity, platform,
//! surfaces, protocol version, build id and, when known, its root and a credential); the hub
//! answers with its own version ([`Welcome`]) or an error. The app then notifies surface and tree
//! changes and its layer's picks and notes, and answers the hub's `tree`, `node`, `find`, `pick`,
//! `act`, `input`, `wait`, `screenshot`, `tree.watch`, `layer.*`, `logs` and `extension.*`
//! requests. [`methods`] names every method; [`docs/devtools.md`] describes the roles.
//!
//! The crate holds types only: no sockets, threads or clocks, and no RayX or NSH crate, so it
//! builds for `wasm32-unknown-unknown` and any tool can speak the protocol by depending on it.
//!
//! - **Encoding.** [`encode_request`], [`encode_notification`] and [`encode_response`] write a
//!   message's fields in a fixed order, so a typed message always encodes to the same bytes.
//! - **Decoding.** [`route`] reads one frame without decoding its parameters, which
//!   [`Params::decode`] does once the receiver knows the method. An unknown method gets a reply
//!   to send ([`Route::Reply`]), an unknown notification is dropped ([`Route::Ignore`]): neither
//!   closes the connection.
//! - **Versions.** [`PROTOCOL_VERSION`] travels in `hello` and its answer. Peers ignore what they
//!   do not know, so a minor version only adds; only a different major version refuses the
//!   connection, with an error naming both ([`VersionMismatch`]).
//! - **Golden messages.** [`GOLDEN_MESSAGES`] holds one example of every message, embedded so a
//!   pinned tag carries them; `docs/devtools/messages.jsonl` is the same file.
//!
//! [`docs/devtools.md`]: https://github.com/rayx-ui/RayX/blob/main/docs/devtools.md

mod messages;

use std::fmt;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;
use serde_json::value::RawValue;

pub use messages::*;

/// The protocol version this crate speaks.
pub const PROTOCOL_VERSION: Version = Version { major: 1, minor: 0 };

/// The port a hub listens on by default, on loopback.
pub const DEFAULT_PORT: u16 = 9240;

/// The URL an agent dials when nothing names another: the hub's default port, on loopback.
pub fn default_url() -> String {
    format!("ws://127.0.0.1:{DEFAULT_PORT}")
}

/// One example of every message, one per line, each a frame as [`route`] reads it; a response
/// follows the request it answers. Every line decodes and encodes back to the same bytes.
pub const GOLDEN_MESSAGES: &str = include_str!("../golden/messages.jsonl");

/// The method names.
pub mod methods {
    /// App to hub, request: the app's identity ([`crate::Hello`] → [`crate::Welcome`]).
    pub const HELLO: &str = "hello";
    /// App to hub, notification: a surface opened ([`crate::SurfaceInfo`]).
    pub const SURFACE_OPENED: &str = "surface.opened";
    /// App to hub, notification: a surface closed ([`crate::SurfaceRef`]).
    pub const SURFACE_CLOSED: &str = "surface.closed";
    /// App to hub, notification, while watched: the tree changed ([`crate::TreeChanged`]).
    pub const TREE_CHANGED: &str = "tree.changed";
    /// Hub to app: the tree, bounded ([`crate::TreeParams`] → [`crate::TreeResult`]).
    pub const TREE: &str = "tree";
    /// Hub to app: watch the tree's changes or stop ([`crate::WatchParams`] → [`crate::Done`]).
    pub const TREE_WATCH: &str = "tree.watch";
    /// Hub to app: one node in full ([`crate::NodeParams`] → [`crate::NodeResult`]).
    pub const NODE: &str = "node";
    /// Hub to app: the nodes that match ([`crate::FindParams`] → [`crate::FindResult`]).
    pub const FIND: &str = "find";
    /// Hub to app: the nodes under a point ([`crate::PickParams`] → [`crate::PickResult`]).
    pub const PICK: &str = "pick";
    /// Hub to app: press, hover, scroll or drag a node ([`crate::Act`] → [`crate::Done`]).
    pub const ACT: &str = "act";
    /// Hub to app: type, submit, clear or press keys ([`crate::Input`] → [`crate::Done`]).
    pub const INPUT: &str = "input";
    /// Hub to app: wait for a text, a node, a node's end or quiet ([`crate::WaitParams`] →
    /// [`crate::Done`]).
    pub const WAIT: &str = "wait";
    /// Hub to app: a picture of a surface or a node ([`crate::ScreenshotParams`] →
    /// [`crate::Image`]).
    pub const SCREENSHOT: &str = "screenshot";
    /// Hub to app: turn the layer's pick mode on or off ([`crate::PickMode`] → [`crate::Done`]).
    pub const LAYER_PICK_MODE: &str = "layer.pickMode";
    /// Hub to app: the node the layer outlines as selected ([`crate::Selection`] →
    /// [`crate::Done`]).
    pub const LAYER_SELECT: &str = "layer.select";
    /// App to hub, notification: the layer picked a node ([`crate::Picked`]).
    pub const LAYER_PICKED: &str = "layer.picked";
    /// App to hub, notification: the layer's toolbar asks for pick mode on or off
    /// ([`crate::PickMode`]).
    pub const LAYER_PICK_MODE_REQUESTED: &str = "layer.pickModeRequested";
    /// App to hub, notification: a note written in the layer's note box ([`crate::Note`]).
    pub const LAYER_NOTE: &str = "layer.note";
    /// Hub to app: the app's log lines ([`crate::LogsParams`] → [`crate::LogsResult`]).
    pub const LOGS: &str = "logs";
    /// Hub to app: the tools the app registered ([`crate::ExtensionList`]).
    pub const EXTENSION_LIST: &str = "extension.list";
    /// Hub to app: run one of them ([`crate::ExtensionCall`] → [`crate::ExtensionResult`]).
    pub const EXTENSION_CALL: &str = "extension.call";

    /// Every method this version knows.
    pub const ALL: &[&str] = &[
        HELLO,
        SURFACE_OPENED,
        SURFACE_CLOSED,
        TREE_CHANGED,
        TREE,
        TREE_WATCH,
        NODE,
        FIND,
        PICK,
        ACT,
        INPUT,
        WAIT,
        SCREENSHOT,
        LAYER_PICK_MODE,
        LAYER_SELECT,
        LAYER_PICKED,
        LAYER_PICK_MODE_REQUESTED,
        LAYER_NOTE,
        LOGS,
        EXTENSION_LIST,
        EXTENSION_CALL,
    ];

    /// Whether `method` is one this version knows.
    pub fn known(method: &str) -> bool {
        ALL.contains(&method)
    }
}

/// The error codes: JSON-RPC's own and this protocol's (`-32000` and below).
pub mod codes {
    /// The frame is not JSON, or not a JSON-RPC message.
    pub const PARSE_ERROR: i64 = -32700;
    /// The method is not one the receiver answers.
    pub const METHOD_NOT_FOUND: i64 = -32601;
    /// The parameters do not fit the method.
    pub const INVALID_PARAMS: i64 = -32602;
    /// The request failed (the message says why).
    pub const FAILED: i64 = -32000;
    /// The hub refused the `hello`'s credential.
    pub const CREDENTIAL: i64 = -32001;
    /// The two sides speak protocol versions of different majors.
    pub const VERSION: i64 = -32002;
    /// The node the request names is gone: take a new snapshot.
    pub const STALE_NODE: i64 = -32003;
    /// The request cannot run on this target (the web's agent has no native driver).
    pub const UNSUPPORTED_ON_TARGET: i64 = -32004;
    /// A wait or an action did not finish in time.
    pub const TIMEOUT: i64 = -32005;
}

/// A protocol version: `major.minor`, written as a string (`"1.0"`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Version {
    pub major: u16,
    pub minor: u16,
}

impl Version {
    /// Whether a peer speaking `theirs` can talk to one speaking `self`: their majors agree.
    pub fn check(self, theirs: Version) -> Result<(), VersionMismatch> {
        if self.major == theirs.major {
            Ok(())
        } else {
            Err(VersionMismatch { ours: self, theirs })
        }
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.major, self.minor)
    }
}

impl std::str::FromStr for Version {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let invalid = || format!("`{text}` is not a protocol version (`major.minor`)");
        let (major, minor) = text.split_once('.').ok_or_else(invalid)?;
        Ok(Version {
            major: major.parse().map_err(|_| invalid())?,
            minor: minor.parse().map_err(|_| invalid())?,
        })
    }
}

impl Serialize for Version {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Version {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = <std::borrow::Cow<'de, str>>::deserialize(deserializer)?;
        text.parse().map_err(serde::de::Error::custom)
    }
}

/// Two peers whose versions' majors differ.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VersionMismatch {
    /// The version the refusing side speaks.
    pub ours: Version,
    /// The version the other side sent.
    pub theirs: Version,
}

impl VersionMismatch {
    /// The error that answers the `hello`: code [`codes::VERSION`], both versions in its message
    /// and in its data (`{"ours":…,"theirs":…}`).
    pub fn to_error(self) -> RpcError {
        RpcError {
            code: codes::VERSION,
            message: self.to_string(),
            data: Some(serde_json::json!({
                "kind": "version",
                "ours": self.ours.to_string(),
                "theirs": self.theirs.to_string(),
            })),
        }
    }
}

impl fmt::Display for VersionMismatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "agent protocol {} cannot talk to agent protocol {}: their major versions differ",
            self.theirs, self.ours
        )
    }
}

impl std::error::Error for VersionMismatch {}

/// A request's id: a number or a string, as JSON-RPC allows.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Id {
    Number(u64),
    Text(String),
}

impl From<u64> for Id {
    fn from(id: u64) -> Self {
        Id::Number(id)
    }
}

impl From<&str> for Id {
    fn from(id: &str) -> Self {
        Id::Text(id.to_owned())
    }
}

impl fmt::Display for Id {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Id::Number(id) => write!(f, "{id}"),
            Id::Text(id) => f.write_str(id),
        }
    }
}

/// A JSON-RPC error. `data.kind` names this protocol's errors in words (`"stale_node"`,
/// `"unsupported_on_target"`, …), so a client need not know the codes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RpcError {
    pub code: i64,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl RpcError {
    /// An error with `code` and `message` and no data.
    pub fn new(code: i64, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            data: None,
        }
    }

    /// `data` with `kind` first: the same bytes whether serde_json keeps a map's order (feature
    /// `preserve_order`) or sorts it.
    fn with_kind(code: i64, kind: &str, message: impl Into<String>, data: Value) -> Self {
        let mut fields = serde_json::Map::new();
        fields.insert("kind".into(), Value::String(kind.into()));
        if let Value::Object(rest) = data {
            fields.extend(rest);
        }
        Self {
            code,
            message: message.into(),
            data: Some(Value::Object(fields)),
        }
    }

    /// A frame that is not a JSON-RPC message.
    pub fn parse_error(message: impl Into<String>) -> Self {
        Self::new(codes::PARSE_ERROR, message)
    }

    /// A method the receiver does not answer.
    pub fn method_not_found(method: &str) -> Self {
        Self::new(
            codes::METHOD_NOT_FOUND,
            format!("no method `{method}` here"),
        )
    }

    /// Parameters that do not fit the method.
    pub fn invalid_params(message: impl Into<String>) -> Self {
        Self::new(codes::INVALID_PARAMS, message)
    }

    /// A request that failed.
    pub fn failed(message: impl Into<String>) -> Self {
        Self::new(codes::FAILED, message)
    }

    /// The hub refused the `hello`'s credential.
    pub fn credential(message: impl Into<String>) -> Self {
        Self::with_kind(
            codes::CREDENTIAL,
            "credential",
            message,
            serde_json::json!({}),
        )
    }

    /// Node `node` is gone. `snapshot` is the node to take a new snapshot under (its nearest
    /// ancestor still there), `None` for the whole tree; the message says which.
    pub fn stale_node(node: u64, snapshot: Option<u64>) -> Self {
        let message = match snapshot {
            Some(root) => format!(
                "node {node} is gone: take a new snapshot of the subtree at node {root} (`tree` with `root: {root}`) and act on an id from it"
            ),
            None => format!(
                "node {node} is gone: take a new snapshot of the tree (`tree`) and act on an id from it"
            ),
        };
        Self::with_kind(
            codes::STALE_NODE,
            "stale_node",
            message,
            serde_json::json!({ "node": node, "snapshotRoot": snapshot }),
        )
    }

    /// `method` cannot run on `target` (`"web"`).
    pub fn unsupported_on_target(method: &str, target: &str) -> Self {
        Self::with_kind(
            codes::UNSUPPORTED_ON_TARGET,
            "unsupported_on_target",
            format!("`{method}` is unsupported on {target}: the hub supplies it there itself"),
            serde_json::json!({ "method": method, "target": target }),
        )
    }

    /// A wait or an action that did not finish within `timeout_ms`.
    pub fn timeout(message: impl Into<String>, timeout_ms: u64) -> Self {
        Self::with_kind(
            codes::TIMEOUT,
            "timeout",
            message,
            serde_json::json!({ "timeoutMs": timeout_ms }),
        )
    }

    /// The word in `data.kind`, for this protocol's own errors.
    pub fn kind(&self) -> Option<&str> {
        self.data.as_ref()?.get("kind")?.as_str()
    }
}

impl fmt::Display for RpcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({})", self.message, self.code)
    }
}

impl std::error::Error for RpcError {}

/// The fields of a message, in the order every frame writes them.
#[derive(Serialize)]
struct Outgoing<'a, P: Serialize, R: Serialize> {
    jsonrpc: &'static str,
    /// `Some(None)` is JSON-RPC's null id.
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<Option<&'a Id>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    method: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    params: Option<&'a P>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<&'a R>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<&'a RpcError>,
}

fn encode<P: Serialize, R: Serialize>(message: &Outgoing<'_, P, R>) -> String {
    // The types here serialize to JSON without fail: no map has non-string keys.
    serde_json::to_string(message).unwrap_or_default()
}

/// The frame of request `id`: `method` with `params`.
pub fn encode_request<P: Serialize>(id: &Id, method: &str, params: &P) -> String {
    encode::<P, ()>(&Outgoing {
        jsonrpc: "2.0",
        id: Some(Some(id)),
        method: Some(method),
        params: Some(params),
        result: None,
        error: None,
    })
}

/// The frame of a notification: `method` with `params`.
pub fn encode_notification<P: Serialize>(method: &str, params: &P) -> String {
    encode::<P, ()>(&Outgoing {
        jsonrpc: "2.0",
        id: None,
        method: Some(method),
        params: Some(params),
        result: None,
        error: None,
    })
}

/// The frame answering request `id` (`None`: one whose id could not be read, answered with a
/// null id as JSON-RPC says).
pub fn encode_response<R: Serialize>(id: Option<&Id>, outcome: Result<&R, &RpcError>) -> String {
    let (result, error) = match outcome {
        Ok(result) => (Some(result), None),
        Err(error) => (None, Some(error)),
    };
    encode(&Outgoing::<(), R> {
        jsonrpc: "2.0",
        id: Some(id),
        method: None,
        params: None,
        result,
        error,
    })
}

/// A frame as read, its parameters or result still undecoded.
#[derive(Deserialize)]
struct Incoming<'a> {
    jsonrpc: std::borrow::Cow<'a, str>,
    #[serde(default)]
    id: Option<Id>,
    #[serde(default, borrow)]
    method: Option<std::borrow::Cow<'a, str>>,
    #[serde(default, borrow)]
    params: Option<&'a RawValue>,
    #[serde(default, borrow)]
    result: Option<&'a RawValue>,
    #[serde(default)]
    error: Option<RpcError>,
}

/// A message's parameters or result, not yet decoded.
#[derive(Clone, Copy, Debug)]
pub struct Params<'a>(Option<&'a RawValue>);

impl<'a> Params<'a> {
    /// The parameters as `T`; absent ones decode as `{}`, so a type whose fields are all optional
    /// takes them. Parameters that do not fit are [`codes::INVALID_PARAMS`].
    pub fn decode<T: DeserializeOwned>(&self) -> Result<T, RpcError> {
        serde_json::from_str(self.raw())
            .map_err(|error| RpcError::invalid_params(error.to_string()))
    }

    /// The parameters' JSON as it came, `{}` when absent.
    pub fn raw(&self) -> &'a str {
        self.0.map_or("{}", RawValue::get)
    }
}

/// What a frame is, and what its receiver does with it.
#[derive(Debug)]
pub enum Route<'a> {
    /// A request of a known method: answer it with [`encode_response`].
    Request {
        id: Id,
        method: &'static str,
        params: Params<'a>,
    },
    /// A notification of a known method.
    Notification {
        method: &'static str,
        params: Params<'a>,
    },
    /// The answer to request `id`.
    Response {
        id: Id,
        outcome: Result<Params<'a>, RpcError>,
    },
    /// A frame that needs an error back (a parse error, an unknown method): send this frame and
    /// keep the connection.
    Reply(String),
    /// A frame to drop (an unknown notification, a response without an id): keep the connection.
    Ignore,
}

/// Reads one text frame. Nothing here closes a connection: a frame that cannot be read gets a
/// reply or is dropped.
pub fn route(text: &str) -> Route<'_> {
    let incoming: Incoming<'_> = match serde_json::from_str(text) {
        Ok(incoming) => incoming,
        Err(error) => {
            return Route::Reply(encode_response::<()>(
                None,
                Err(&RpcError::parse_error(error.to_string())),
            ));
        }
    };
    if incoming.jsonrpc != "2.0" {
        return Route::Reply(encode_response::<()>(
            incoming.id.as_ref(),
            Err(&RpcError::parse_error("not a JSON-RPC 2.0 message")),
        ));
    }
    let Some(method) = incoming.method else {
        let Some(id) = incoming.id else {
            return Route::Ignore;
        };
        let outcome = match incoming.error {
            Some(error) => Err(error),
            None => Ok(Params(incoming.result)),
        };
        return Route::Response { id, outcome };
    };
    let Some(&known) = methods::ALL.iter().find(|known| **known == method) else {
        return match incoming.id {
            Some(id) => Route::Reply(encode_response::<()>(
                Some(&id),
                Err(&RpcError::method_not_found(&method)),
            )),
            None => Route::Ignore,
        };
    };
    let params = Params(incoming.params);
    match incoming.id {
        Some(id) => Route::Request {
            id,
            method: known,
            params,
        },
        None => Route::Notification {
            method: known,
            params,
        },
    }
}
