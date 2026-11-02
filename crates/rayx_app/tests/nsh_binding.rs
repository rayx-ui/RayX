//! dXn.Reactive view models bound to NSH, headless: a Rust root shows a view model's signals,
//! list and command as NSH signals and writes an input back, and a TSX class exposes its values
//! as `Observable`s, with changes from the engine's thread and from other threads.

// Loading the compiled TSX app takes `Bundle::bytecode`.
#![allow(unsafe_code, clippy::expect_used, clippy::unwrap_used)]

use std::sync::OnceLock;
use std::time::Duration;

use crossbeam_channel::{Receiver, Sender};
use dxn_reactive::collections::{ObservableList, SourceList};
use dxn_reactive::{Command, Signal as VmSignal};
use nsh_core::element::Registry;
use nsh_core::{ApproxTextEngine, MountBatch, MountItem, prelude::*};
use nsh_engine::{App, Bundle, Engine, EngineConfig, LocalEngine, MountSink};
use rayx_app::{
    AppServices, Observable, RayXAppBuilder, ReactiveCommand, set_app, use_command, use_list,
    use_reactive, use_two_way,
};

struct ChannelSink(Sender<MountBatch>);

impl MountSink for ChannelSink {
    fn submit(&self, batch: MountBatch) {
        let _ = self.0.send(batch);
    }
}

/// A view model, shared by the test and the UI.
struct Vm {
    count: VmSignal<u32>,
    name: VmSignal<String>,
    enabled: VmSignal<bool>,
    items: SourceList<String>,
    list: ObservableList<String>,
    increment: Command,
}

fn vm() -> &'static Vm {
    static VM: OnceLock<Vm> = OnceLock::new();
    VM.get_or_init(|| {
        let count = VmSignal::new(0_u32);
        let enabled = VmSignal::new(true);
        let items = SourceList::new(vec!["a".to_string()]);
        let list = ObservableList::from_source(items.clone());
        let counter = count.clone();
        let increment = Command::with_can_execute(enabled.read_only(), move || {
            counter
                .update(|count| *count += 1)
                .map(|_| ())
                .map_err(Into::into)
        });
        Vm {
            count,
            name: VmSignal::new(String::new()),
            enabled,
            items,
            list,
            increment,
        }
    })
}

#[component]
fn Root() -> impl IntoElement {
    let vm = vm();
    let count = use_reactive(&vm.count.read_only()).unwrap();
    let name = use_two_way(&vm.name).unwrap();
    let items = use_list(&vm.list).unwrap();
    let increment = use_command(&vm.increment).unwrap();
    // The UI writes the input once it runs.
    spawn(async move { name.set("Ana".to_string()) });
    text(move || {
        format!(
            "count {} name {} items {} can {}",
            count.get(),
            name.get(),
            items.get().join(","),
            increment.can_execute.get()
        )
    })
}

fn pump_until(
    engine: &mut LocalEngine<ApproxTextEngine>,
    rx: &Receiver<MountBatch>,
    wanted: &str,
) -> Vec<String> {
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    let mut shown = Vec::new();
    while std::time::Instant::now() < deadline {
        engine.pump();
        while let Ok(batch) = rx.recv_timeout(Duration::from_millis(5)) {
            for item in &batch.items {
                if let MountItem::Text { text, .. } = item {
                    shown.push(text.text.clone());
                }
            }
        }
        if shown.iter().any(|text| text == wanted) {
            break;
        }
    }
    shown
}

#[test]
fn a_rust_component_shows_the_view_model_and_writes_back() {
    let (tx, rx) = crossbeam_channel::unbounded();
    let mut engine = LocalEngine::start(
        EngineConfig::new(App::Rust(nsh_core::element::Root::of::<Root>())),
        ApproxTextEngine,
        ChannelSink(tx),
    )
    .unwrap();
    let wanted = "count 0 name Ana items a can true";
    let shown = pump_until(&mut engine, &rx, wanted);
    assert!(shown.iter().any(|text| text == wanted), "shown: {shown:?}");
    // The input reached the view model.
    assert_eq!(vm().name.get().unwrap(), "Ana");

    // Changes outside the engine (it runs only while it is pumped), and from another thread.
    vm().increment.execute().unwrap();
    vm().items.push("b".to_string()).unwrap();
    std::thread::spawn(|| vm().enabled.set(false).unwrap())
        .join()
        .unwrap();
    let wanted = "count 1 name Ana items a,b can false";
    let shown = pump_until(&mut engine, &rx, wanted);
    assert!(shown.iter().any(|text| text == wanted), "shown: {shown:?}");
    assert!(vm().increment.execute().is_err(), "refused while disabled");
}

/// A view model class for TSX: its values as `Observable`s, its command as a `ReactiveCommand`.
#[class]
pub struct CounterVm {
    count: VmSignal<u32>,
    increment: Command,
}

#[class]
impl CounterVm {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        let count = VmSignal::new(0_u32);
        let counter = count.clone();
        let increment = Command::new(move || {
            counter
                .update(|count| *count += 1)
                .map(|_| ())
                .map_err(Into::into)
        });
        Self { count, increment }
    }

    #[getter]
    pub fn count(&self) -> Result<Instance<Observable>, String> {
        Observable::of(&self.count.read_only()).map_err(|error| error.to_string())
    }

    #[getter]
    pub fn increment(&self) -> Instance<ReactiveCommand> {
        ReactiveCommand::of(&self.increment)
    }

    /// Sets the count on another thread, as a background task of the view model would.
    pub fn set_elsewhere(&self, value: u32) {
        let count = self.count.clone();
        std::thread::spawn(move || count.set(value).unwrap())
            .join()
            .unwrap();
    }
}

const TSX: &str = r#"import { mount, signal, Text } from "nsh";
import { AppServices, CounterVm } from "test/app";

const shown = signal("waiting");
const tick = () => new Promise((resolve) => setTimeout(resolve, 10));
setTimeout(async () => {
  await AppServices.load();
  const vm = new CounterVm();
  const count = vm.count;
  const seen = [];
  count.bind((value) => seen.push(value));
  const canExecute = vm.increment.canExecute.value;
  vm.increment.execute();
  vm.increment.execute();
  await tick();
  vm.setElsewhere(7);
  for (let i = 0; i < 50 && !seen.includes(7); i++) await tick();
  shown.set(`done ${seen.join(",")} value ${count.value} can ${canExecute}`);
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

fn shown(rx: &Receiver<MountBatch>, prefix: &str) -> String {
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    let mut last = String::new();
    while let Ok(batch) = rx.recv_deadline(deadline) {
        for item in &batch.items {
            if let MountItem::Text { text, .. } = item {
                last = text.text.clone();
            }
        }
        if last.starts_with(prefix) {
            break;
        }
    }
    last
}

#[test]
fn a_tsx_class_exposes_the_view_model_as_observables() {
    set_app(|builder: RayXAppBuilder| builder);
    let mut set = Registry::ui_set("test/app");
    set.class::<AppServices>()
        .class::<CounterVm>()
        .class::<Observable>()
        .class::<ReactiveCommand>();
    let dir = std::env::temp_dir().join(format!("rayx-app-binding-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let entry = dir.join("App.tsx");
    std::fs::write(&entry, TSX).unwrap();
    let ui_sets = vec![set];
    let bundle = nsh_bundler::bundle(&nsh_bundler::BundleOptions {
        entry,
        minify: false,
        split: false,
        ui_sets: ui_sets.iter().map(Into::into).collect(),
    })
    .unwrap();
    let described: Vec<_> = ui_sets.iter().map(Into::into).collect();
    let bytecode = nsh_js::compile_bytecode_with(&bundle.code, false, &described).unwrap();
    let (tx, rx) = crossbeam_channel::unbounded();
    // SAFETY: compiled just now by this runtime.
    let app = App::Js(unsafe { Bundle::bytecode(bytecode) });
    let config = EngineConfig {
        ui_sets,
        ..EngineConfig::new(app)
    };
    let _engine = Engine::start(config, || ApproxTextEngine, ChannelSink(tx)).unwrap();
    let last = shown(&rx, "done");
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(last, "done 0,1,2,7 value 7 can true");
}
