# Dev tools

In a dev build a RayX app carries a small agent that a dev tool connects to: the tool asks what is
on screen (the tree as the source wrote it, a node's box and classes, where a node was written), drives
the app (presses, typing, waits, screenshots) and shows its selection in an in-app layer. A release
build has none of it.

The agent and the tool speak the **agent protocol** of the crate `rayx_devtools_protocol`
(`rayx::devtools_protocol`): JSON-RPC 2.0 over WebSocket text frames. The tool's side is called the
**hub**; one hub serves many apps, and many tools (an editor, an AI agent through MCP, a command line)
reach the apps through it.

## The switch

`rayx_app` compiles the agent and the layer under `cfg(rayx_devtools)`. Its build script sets it when
NSH's dev tools hooks are compiled in and `RAYX_DEVTOOLS` is not `0`:

- NSH's hooks are `nsh-core`'s feature `inspect`, which every platform's `dev` feature turns on and
  release runners never do. `nsh-core` tells the crates that depend on it whether it is on
  (`DEP_NSH_CORE_INSPECT`, through its `links` key), whatever the cargo profile.
- So `rayx app run` (a dev build) has the dev tools; `rayx app package`, a plain `cargo test` or
  `cargo check` of the app's crate and NSH's `describe` program do not.
- `RAYX_DEVTOOLS=0` at build time leaves them out of a dev build too.

The `DevTools` class and the `DevLayer` component are registered in every build, so the `describe`
program and a runner see the same names; without the dev tools their bodies do nothing.
`rayx_app::devtools_enabled()` (TSX: `DevTools.isEnabled()`) says whether a build has them.

## The agent

- **When it starts.** Once the app is ready: `await builder.start()` (TSX) or
  `RayXAppBuilder::new()….start()` (a Rust root) starts it in a host's dev session. It stays off in a plain engine (a test's) and in a test
  session, whose host refuses the app's connections; there `await DevTools.start({ url })` (TSX) or
  `rayx_app::start_devtools(options)` (Rust) starts it. `DevTools:Enabled = false` in the
  configuration turns it off.
- **Where it dials.** `ws://127.0.0.1:9240`, the hub's default on loopback, or `DevTools:Url`, or the
  `url` `DevTools.start` names. It dials again after 250 ms, doubling to 5 s, so the hub may start
  before or after the app. Nothing listens inside the app.
- **`hello`.** Its first frame on every connection: the app's id and name (`DevTools:AppId`,
  `Application:Name`), the platform, the protocol version, a build id that changes with every engine
  start, the app's root (the directory of its `nsh.json`, from the desktop dev host's
  `NSH_DEV_APP_DIR`; a web page sends none, since no web host tells it, and a hub that serves a web
  session knows its root itself) and a credential when one is set (`DevTools:Credential`, or `DevTools.start`'s
  `credential`; opaque, empty by default).
- **Natively** the connection runs on a thread of its own; on the web it is the page's WebSocket,
  through NSH's transport (TSX's `builder.start()` hands it over; a Rust root on the web has none).

## What it answers

| Request | Answer |
| --- | --- |
| `tree` | The tree as authored: each line its id, kind, role, text, test id, source stamp (module, anchor, line, column) and component chain; a component instance stands in the tree as a pseudo-node of kind `instance`, whose id is above every node id. Bounded by `root`, `depth`, `interactiveOnly` and `maxChars` (then `truncated`). |
| `node` | One node in full: its line, its box (frame, padding, border, margin), its classes and inline style, its label and the events it listens to. |
| `find` | The nodes that match a test id, a kind, a role and a text (the innermost node reading it). |
| `pick` | The nodes under a point, innermost first. |
| `tree.watch` | While on, a `tree.changed` notification with the new revision whenever the tree changed. |
| `act`, `input`, `wait`, `screenshot` | Natively, through the dev session's driver (NSH's observe mode): press, long press, hover, scroll and drag a node; type, submit, clear and press keys; wait for a text, a test id, a node's end or quiet; a picture of the surface or of a node. Each answers once the screen settled. On the web they answer `unsupported_on_target`: the hub drives a page it runs itself. |
| `layer.pickMode`, `layer.select` | The layer's pick mode and the selected node, which the hub owns. |

Every tree answer carries the tree's **revision**, which grows with every change the agent saw. A node
id is exact while its node lives; a request that names a node that is gone fails with `stale_node`,
whose message names a subtree to look at again (the nearest ancestor of the node that is still there).
Everything else that fails is `failed`, with the reason.

**Pay on connect.** With no hub watching the tree, the agent keeps no tree and does no work per
commit: it calls NSH's hooks only to answer a request, and while `tree.watch` is on.

The agent's own layer is in no answer.

## The layer

`<DevLayer style={{ position: "absolute" }} />` in TSX, `.child(dev_layer())` in a Rust root, mounted
once at the app's root (`rayx new` writes it). Its host takes no place in the parent's layout:
`dev_layer()` makes it absolute, and in TSX the `style` does (a component cannot style its own host),
so a dev build and a release build lay the app out alike. The agent leaves the layer out of every
answer, from that host down. In a dev build it shows a small toolbar, reachable by touch: pick mode (asked of the hub;
also `mod+shift+d`), the note box (a note goes to the hub about the selection) and one step outward
from the selection. In pick mode a full-surface view over the app takes the pointer: hovering outlines
the innermost node under it, a press reports it to the hub (`layer.picked`), a press with a modifier
the next node outward. The hovered and the selected node are outlined with their margin and padding
bands. The layer holds no state of its own: pick mode and the selection are the hub's.

**Stacking.** The layer is an overlay: on some platforms a modal opened after it sits above it, and the
layer cannot outline the modal's content.

## The protocol

`hello` carries the protocol version, `major.minor`. A peer ignores what it does not know, so a minor
version only adds; only a different major version refuses the connection, with an error naming both.
An unknown method is answered `method not found`, an unknown notification dropped, and neither closes
the connection.

The golden messages, one of every message, are [devtools/messages.jsonl](devtools/messages.jsonl), the
same file the crate embeds (`GOLDEN_MESSAGES`), so a pinned tag carries them. Each line is a frame; a
response follows the request it answers. The protocol names nothing of a product built on RayX: a
marker is a marker, a note a note.

## Tests

| Group | Proves |
| --- | --- |
| `cargo test -p rayx_devtools_protocol --test devtools_protocol` | Golden messages round-trip byte for byte; unknown methods; version mismatch |
| `cargo test -p rayx_app --test devtools_connect` | The switch, the dial with backoff, `hello`, starting only in a dev session |
| `cargo test -p rayx_app --test devtools_tree` | Tree, node, find and pick on a hot TSX app; bounds; pay on connect |
| `cargo test -p rayx_app --test devtools_act` | Act, input, wait and screenshot through a fake driver on the worker thread |
| `cargo test -p rayx_app --test devtools_layer` | Pick mode, outlines, the toolbar and the note box on the headless engine |
| `cargo test -p rayx-cli --test new_devtools` | `rayx new` writes the class and the layer |
