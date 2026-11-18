//! The dev tools' in-app agent: in a dev build the app dials out to a tool's hub and answers its
//! questions about the screen ([`rayx_devtools_protocol`], `docs/devtools.md`).
//!
//! - **The switch.** Everything here that does work is compiled only under `cfg(rayx_devtools)`,
//!   which `build.rs` sets when NSH's dev tools hooks are compiled in (a dev build) and
//!   `RAYX_DEVTOOLS` is not `0`. The [`DevTools`] class is registered in every build, so the
//!   `describe` program and a runner see the same names; in a release build its body does
//!   nothing.
//! - **When it starts.** Once the app is ready (a Rust root's
//!   [`start`](crate::StartAppBuilderExt::start), TSX's `builder.start()`), never from the app's
//!   `#[native_module]` function, which also runs inside `describe`. It stays off in a test
//!   session (the host refuses the app's connections there) unless the test starts it with
//!   [`DevTools::start`] or [`start_devtools`], and when the configuration says
//!   `DevTools:Enabled = false`.
//! - **Where it dials.** [`rayx_devtools_protocol::default_url`] (the hub's loopback port), or
//!   `DevTools:Url` in the configuration, or what `DevTools.start({ url })` names. It retries
//!   with backoff from 250 ms to 5 s, so the hub may start before or after the app.
//! - **Pay on connect.** With no hub watching the tree, the agent keeps no tree and does no work
//!   per commit.

#[cfg(all(rayx_devtools, not(target_arch = "wasm32")))]
mod act;
#[cfg(rayx_devtools)]
mod agent;
mod layer;
#[cfg(rayx_devtools)]
mod link;
#[cfg(rayx_devtools)]
mod tree;

pub use layer::{DevLayer, dev_layer};

use nsh_core::JsonValue;
use nsh_core::prelude::*;
use serde::Deserialize;

/// Where the agent dials and what it says it is, beyond the defaults.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DevToolsOptions {
    /// The hub's URL; `None` for `DevTools:Url`, else the hub's default.
    pub url: Option<String>,
    /// The opaque string `hello` carries; `None` for `DevTools:Credential`, else none.
    pub credential: Option<String>,
}

/// Whether this build has the dev tools: a dev build, with `RAYX_DEVTOOLS` not `0`.
pub const fn enabled() -> bool {
    cfg!(rayx_devtools)
}

/// Whether this build has the dev tools, for the app's code: a dev build, with `RAYX_DEVTOOLS`
/// not `0`.
pub const fn devtools_enabled() -> bool {
    enabled()
}

/// How many times the agent called NSH's tree hooks in this process (`dev_tree`, `dev_node`,
/// `dev_pick`, `dev_source`, `dev_instances`): with no hub watching, a commit adds none. Always
/// `0` in a build without the dev tools.
#[doc(hidden)]
pub fn devtools_hook_calls() -> u64 {
    #[cfg(rayx_devtools)]
    return agent::hook_calls();
    #[cfg(not(rayx_devtools))]
    0
}

/// How long the agent waits before dialling again after `attempt` failed tries: 250 ms,
/// doubling, at most 5 s.
#[doc(hidden)]
pub fn devtools_backoff(attempt: u32) -> std::time::Duration {
    #[cfg(rayx_devtools)]
    return link::backoff(attempt);
    #[cfg(not(rayx_devtools))]
    std::time::Duration::from_millis(250)
        .saturating_mul(2u32.saturating_pow(attempt.min(16)))
        .min(std::time::Duration::from_secs(5))
}

/// Starts the agent on this surface now, also in a test session, with `options` over the
/// configuration's; once started, later calls change nothing. Returns whether this build has
/// the dev tools. Call it on the engine's thread once the app is ready.
pub fn start_devtools(options: DevToolsOptions) -> bool {
    #[cfg(rayx_devtools)]
    agent::start(options, agent::Why::Asked, None);
    #[cfg(not(rayx_devtools))]
    drop(options);
    enabled()
}

/// The dev tools for TSX: `DevTools.enabled` says whether this build has them, and
/// `await DevTools.start({ url, credential })` starts the agent now (in a test, where it stays off
/// otherwise). Registered in every build: in a release build it does nothing.
#[class]
pub struct DevTools;

#[class]
impl DevTools {
    /// Whether this build has the dev tools.
    pub fn is_enabled() -> bool {
        enabled()
    }

    /// Starts the agent with `options` (`{ url?, credential? }`); resolves with whether this
    /// build has the dev tools. A class's `async` member, so it runs as the core's task, where
    /// the surface's state is reachable.
    pub async fn start(sys: System, options: JsonValue) -> Result<bool, String> {
        let options = if options.is_null() {
            DevToolsOptions::default()
        } else {
            serde_json::from_value(options).map_err(|error| error.to_string())?
        };
        #[cfg(rayx_devtools)]
        agent::start(options, agent::Why::Asked, sys.websocket.clone());
        #[cfg(not(rayx_devtools))]
        drop((sys, options));
        Ok(enabled())
    }
}

/// The app is ready on this surface: start the agent unless this is a test session or the
/// configuration turns it off. `sys` is the VM's system when the caller has it (TSX's
/// `builder.start()`): its host refuses the app's connections in a test session, and its
/// WebSocket transport is the web's link; a Rust root has none.
pub(crate) fn app_ready(sys: Option<&System>) {
    #[cfg(rayx_devtools)]
    if !sys.is_some_and(refused) {
        agent::start(
            DevToolsOptions::default(),
            agent::Why::Ready,
            sys.and_then(|sys| sys.websocket.clone()),
        );
    }
    #[cfg(not(rayx_devtools))]
    let _ = sys;
}

/// Whether the host refuses the app's connections to the hub: a test session's does, unless
/// the test asks for the network.
#[cfg(rayx_devtools)]
fn refused(sys: &System) -> bool {
    sys.check_connection(&rayx_devtools_protocol::default_url())
        .is_err()
}

/// Registers the agent's observer of the dev session, once a process: from the app's UI set
/// ([`ts::register`](crate::ts::register)), before the host starts, else when a builder is made
/// or the app starts.
pub(crate) fn register() {
    #[cfg(rayx_devtools)]
    {
        static ONCE: std::sync::Once = std::sync::Once::new();
        ONCE.call_once(agent::register);
    }
}
