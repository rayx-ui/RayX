//! The dev tools' agent connects to a hub: it dials with backoff, so the hub may start after
//! the app; it opens with `hello` (the app's identity, platform, protocol version, a build id per
//! engine start, the app's root and the credential); it answers on the connection, an unknown
//! method too, without closing it. It starts by itself only in a host's dev session (a plain
//! engine, as a test's, has none) and otherwise when asked; and the switch is NSH's hooks: a
//! crate built without them has no agent.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, Sender};
use nsh_core::{ApproxTextEngine, MountBatch, prelude::*};
use nsh_engine::{App, EngineConfig, LocalEngine, MountSink};
use rayx_app_nsh::{
    AppState, ConfigurationAppBuilderExt as _, DevToolsOptions, RayXAppBuilder,
    StartAppBuilderExt as _, app_state, start_devtools,
};
use rayx_devtools_protocol::{self as proto, Route};
use tungstenite::Message;

/// The hub's port the app's configuration names (`DevTools:Url`).
static PORT: Mutex<u16> = Mutex::new(0);
/// What the root asks for once the app is ready: `Some` starts the agent with these options.
static ASK: Mutex<Option<DevToolsOptions>> = Mutex::new(None);

fn build_app(builder: RayXAppBuilder) -> RayXAppBuilder {
    let url = format!("ws://127.0.0.1:{}", *PORT.lock().unwrap());
    builder.configure_app_configuration(move |configuration| {
        configuration.add_in_memory([
            ("DevTools:Url", url.clone()),
            ("Application:Name", "Notes".to_owned()),
            ("DevTools:AppId", "com.example.notes".to_owned()),
        ]);
    })
}

#[component]
fn Root() -> impl IntoElement {
    build_app(::rayx_app::testing::add_host_fakes(RayXAppBuilder::new())).start();
    effect(move || {
        if app_state() == AppState::Ready
            && let Some(options) = ASK.lock().unwrap().take()
        {
            start_devtools(options);
        }
    });
    view().child(text("ready"))
}

struct Sink(Sender<MountBatch>);

impl MountSink for Sink {
    fn submit(&self, batch: MountBatch) {
        let _ = self.0.send(batch);
    }
}

fn serial() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// A port nothing listens on yet.
fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

/// Runs an engine with the app until `done` says so or `limit` passes.
fn run_app(limit: Duration, done: impl Fn() -> bool) {
    let (tx, rx): (Sender<MountBatch>, Receiver<MountBatch>) = crossbeam_channel::unbounded();
    let mut engine = LocalEngine::start(
        EngineConfig::new(App::Rust(nsh_core::element::Root::of::<Root>())),
        ApproxTextEngine,
        Sink(tx),
    )
    .unwrap();
    let deadline = Instant::now() + limit;
    while Instant::now() < deadline && !done() {
        engine.pump();
        while rx.recv_timeout(Duration::from_millis(2)).is_ok() {}
    }
}

/// What a hub on `port` saw: its first frame and the answers to what it asked.
#[derive(Debug, Default)]
struct Seen {
    hello: Option<String>,
    unknown: Option<String>,
    tree: Option<String>,
}

/// A hub that starts listening on `port` after `after`, takes one connection, reads its first
/// frame, then asks an unknown method and the tree on the same connection.
fn hub(port: u16, after: Duration, accept_for: Duration) -> Arc<Mutex<Seen>> {
    let seen = Arc::new(Mutex::new(Seen::default()));
    let out = seen.clone();
    std::thread::spawn(move || {
        std::thread::sleep(after);
        let listener = TcpListener::bind(("127.0.0.1", port)).unwrap();
        listener.set_nonblocking(true).unwrap();
        let deadline = Instant::now() + accept_for;
        let stream: TcpStream = loop {
            match listener.accept() {
                Ok((stream, _)) => break stream,
                Err(_) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(_) => return,
            }
        };
        stream.set_nonblocking(false).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        let mut socket = tungstenite::accept(stream).unwrap();
        let mut read = || match socket.read() {
            Ok(Message::Text(text)) => Some(text.to_string()),
            _ => None,
        };
        let hello = read();
        out.lock().unwrap().hello = hello;
        drop(read);
        socket
            .send(Message::text(
                r#"{"jsonrpc":"2.0","id":7,"method":"frobnicate","params":{}}"#,
            ))
            .unwrap();
        let unknown = socket.read().ok().map(|m| m.to_string());
        out.lock().unwrap().unknown = unknown;
        socket
            .send(Message::text(
                r#"{"jsonrpc":"2.0","id":8,"method":"tree","params":{"depth":1}}"#,
            ))
            .unwrap();
        let tree = socket.read().ok().map(|m| m.to_string());
        out.lock().unwrap().tree = tree;
    });
    seen
}

#[test]
#[allow(unsafe_code)]
fn the_agent_dials_with_backoff_says_hello_and_answers() {
    let _serial = serial();
    assert!(
        rayx_app_nsh::devtools_enabled(),
        "this crate's tests build with NSH's hooks"
    );
    let port = free_port();
    *PORT.lock().unwrap() = port;
    // SAFETY: the tests of this file run one at a time (`serial`), and nothing else here reads
    // the environment meanwhile.
    unsafe { std::env::set_var("NSH_DEV_APP_DIR", "/work/notes") };
    *ASK.lock().unwrap() = Some(DevToolsOptions {
        url: None,
        credential: Some("opaque-7d".to_owned()),
    });
    // The hub comes up after the agent's first tries.
    let seen = hub(port, Duration::from_millis(700), Duration::from_secs(10));
    let started = Instant::now();
    run_app(Duration::from_secs(20), || {
        seen.lock().unwrap().tree.is_some()
    });
    let seen = seen.lock().unwrap();

    let hello = seen.hello.as_deref().expect("the agent connected");
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "it dialled again within its backoff"
    );
    let Route::Request { method, params, .. } = proto::route(hello) else {
        panic!("hello is a request: {hello}");
    };
    assert_eq!(method, proto::methods::HELLO);
    let hello: proto::Hello = params.decode().unwrap();
    assert_eq!(hello.protocol_version, proto::PROTOCOL_VERSION);
    assert_eq!(hello.app_id, "com.example.notes");
    assert_eq!(hello.app_name, "Notes");
    assert_eq!(hello.platform, std::env::consts::OS);
    assert!(!hello.build_id.is_empty());
    assert_eq!(hello.root.as_deref(), Some("/work/notes"));
    assert_eq!(hello.credential, "opaque-7d");

    // An unknown method is answered; the connection stays and answers the tree next.
    let unknown = seen.unknown.as_deref().unwrap();
    assert!(
        unknown.contains(r#""id":7"#) && unknown.contains("-32601"),
        "{unknown}"
    );
    let tree = seen.tree.as_deref().expect("the tree was answered");
    let Route::Response { outcome, .. } = proto::route(tree) else {
        panic!("a response: {tree}");
    };
    let tree: proto::TreeResult = outcome.unwrap().decode().unwrap();
    assert!(tree.revision >= 1 && !tree.nodes.is_empty(), "{tree:?}");
}

#[test]
fn it_starts_by_itself_only_in_a_dev_session() {
    let _serial = serial();
    let port = free_port();
    *PORT.lock().unwrap() = port;
    *ASK.lock().unwrap() = None;
    // A plain engine: the app is ready, nothing asks, no host handed a session's driver over.
    let seen = hub(port, Duration::ZERO, Duration::from_secs(2));
    run_app(Duration::from_secs(3), || false);
    assert!(
        seen.lock().unwrap().hello.is_none(),
        "the agent dialled outside a dev session"
    );

    // A host's dev session hands its driver over: the agent starts once the app is ready.
    struct Driver;
    impl nsh_core::inspect::DevDriver for Driver {
        fn step(&self, _: serde_json::Value) -> serde_json::Value {
            serde_json::json!({ "outcome": "done", "value": null })
        }
    }
    nsh_core::inspect::hand_driver(Arc::new(Driver));
    let port = free_port();
    *PORT.lock().unwrap() = port;
    let seen = hub(port, Duration::ZERO, Duration::from_secs(10));
    run_app(Duration::from_secs(15), || {
        seen.lock().unwrap().hello.is_some()
    });
    assert!(
        seen.lock().unwrap().hello.is_some(),
        "the agent stayed off in a dev session"
    );
}

#[test]
fn the_backoff_waits_from_a_quarter_second_doubling_to_five() {
    let waits: Vec<u64> = (0..8)
        .map(|attempt| rayx_app_nsh::devtools_backoff(attempt).as_millis() as u64)
        .collect();
    assert_eq!(waits, [250, 500, 1000, 2000, 4000, 5000, 5000, 5000]);
    assert_eq!(
        rayx_app_nsh::devtools_backoff(u32::MAX),
        Duration::from_secs(5)
    );
}

/// Checks `tests/devtools_switch/` with `features` and `env`; whether it compiled.
fn switch_compiles(features: &[&str], env: &[(&str, &str)]) -> Result<(), String> {
    let crate_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let root = crate_dir.ancestors().nth(2).unwrap();
    let dir = root.join("target/devtools-switch");
    std::fs::create_dir_all(dir.join("src")).unwrap();
    let workspace = std::fs::read_to_string(root.join("Cargo.toml")).unwrap();
    let patch = &workspace[workspace.find("[patch.crates-io]").unwrap()..];
    let manifest = std::fs::read_to_string(crate_dir.join("tests/devtools_switch/Cargo.toml.in"))
        .unwrap()
        // Forward slashes: a Windows path's backslashes are escapes in a TOML string.
        .replace(
            "@RAYX_APP@",
            &crate_dir.to_str().unwrap().replace('\\', "/"),
        )
        .replace("@PATCH@", patch);
    std::fs::write(dir.join("Cargo.toml"), manifest).unwrap();
    std::fs::copy(
        crate_dir.join("tests/devtools_switch/src/lib.rs"),
        dir.join("src/lib.rs"),
    )
    .unwrap();
    std::fs::copy(root.join("Cargo.lock"), dir.join("Cargo.lock")).unwrap();
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let mut command = std::process::Command::new(cargo);
    command
        .current_dir(&dir)
        .arg("check")
        .args(features)
        .env("CARGO_TARGET_DIR", dir.join("target"))
        .env_remove("RAYX_DEVTOOLS")
        .env_remove("EXPECT_DEVTOOLS");
    for (key, value) in env {
        command.env(key, value);
    }
    let out = command.output().unwrap();
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).into_owned())
    }
}

#[test]
fn the_switch_is_nsh_s_hooks_and_rayx_devtools() {
    // Without NSH's hooks (a plain `cargo check` of an app's crate, `describe`): no agent.
    switch_compiles(&[], &[]).unwrap();
    // With them (a dev build): the agent.
    switch_compiles(&["--features", "hooks"], &[("EXPECT_DEVTOOLS", "1")]).unwrap();
    // With them and `RAYX_DEVTOOLS=0`: none.
    switch_compiles(&["--features", "hooks"], &[("RAYX_DEVTOOLS", "0")]).unwrap();
    // And the assertion bites: expecting the agent without the hooks fails.
    assert!(switch_compiles(&[], &[("EXPECT_DEVTOOLS", "1")]).is_err());
}
