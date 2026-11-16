//! A hot TSX app that starts the dev tools' agent against a test hub, on an engine thread of
//! its own.

#![allow(dead_code)]

use crossbeam_channel::Sender;
use nsh_core::{ApproxTextEngine, MountBatch, Size, element::Registry, prelude::px};
use nsh_engine::{App, Bundle, Engine, EngineConfig, MountSink};
use rayx_app::DevTools;

use super::hub::Hub;

const APP: &str = r#"import { mount, signal, Text, View } from "nsh";
import { DevTools, RayXAppBuilder } from "test/app";

export function Card(props: { title: string }) {
  return <View testID="card">
    <Text>{props.title}</Text>
  </View>;
}

const ticks = signal(0);

function App() {
  return <View testID="root">
    <Card title="Hello" />
    <View testID="save" onPress={() => {}}><Text>Save</Text></View>
    <Text>{"tick " + ticks()}</Text>
    <View testID="rayx-devtools-layer"><Text>the layer</Text></View>
  </View>;
}

mount(App);
setInterval(() => ticks.update((t) => t + 1), 30);
new RayXAppBuilder().start().then(() => DevTools.start({ url: "@URL@" }));
"#;

struct Sink(Sender<MountBatch>);

impl MountSink for Sink {
    fn submit(&self, batch: MountBatch) {
        let _ = self.0.send(batch);
    }
}

/// One app at a time: the hook counter is the process's.
pub fn serial() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Starts the hot app, dialling `hub`; the engine runs on a thread of its own.
pub fn start(hub: &Hub) -> (Engine, std::path::PathBuf) {
    let mut set = Registry::ui_set("test/app");
    rayx_app::ts::register(&mut set);
    set.class::<DevTools>();
    let dir = std::env::temp_dir().join(format!("rayx-devtools-tree-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("App.tsx"), APP.replace("@URL@", &hub.url())).unwrap();
    let ui_sets = vec![set];
    let described: Vec<_> = ui_sets.iter().map(Into::into).collect();
    let bundle = nsh_bundler::bundle_hot(&dir.join("App.tsx"), &described).unwrap();
    let (code, map) = bundle.script();
    let mut config = EngineConfig::new(App::Js(Bundle::source(code).with_source_map(map.unwrap())));
    config.viewport = Size::new(px(390.0), px(844.0));
    config.ui_sets = ui_sets;
    let (tx, rx) = crossbeam_channel::unbounded();
    std::thread::spawn(move || while rx.recv().is_ok() {});
    let engine = Engine::start(config, || ApproxTextEngine, Sink(tx)).unwrap();
    (engine, dir)
}
