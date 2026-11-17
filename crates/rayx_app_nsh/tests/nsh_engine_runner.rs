//! The surface's engine runner, headless: from another thread it runs an engine-only NSH call on
//! the engine thread and gives its result; on the engine thread it runs without the queue (at
//! once in a task, as the call's op in a synchronous call from JS); once the engine is gone it
//! resolves `Err(EngineGone)`.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod headless;

use std::sync::{Arc, Mutex};

use nsh_core::prelude::*;
use rayx_app_nsh::ts::ServiceProvider;
use rayx_app_nsh::{EngineGone, EngineRunner};

/// The runner, kept past the engine's life.
static KEPT: Mutex<Option<EngineRunner>> = Mutex::new(None);

/// Work the runner started that never ends, kept for after the engine stopped.
static IN_FLIGHT: Mutex<Option<futures::future::BoxFuture<'static, Result<u32, EngineGone>>>> =
    Mutex::new(None);

/// What ran in and after a synchronous call from JS, in order.
static ORDER: Mutex<Vec<&'static str>> = Mutex::new(Vec::new());

fn runner(services: &Instance<ServiceProvider>) -> EngineRunner {
    let runner = services.with(|services| services.services().required_service::<EngineRunner>());
    (*runner.unwrap()).clone()
}

/// The test's Rust side.
#[module]
struct Probe;

#[module]
impl Probe {
    /// From another thread: an engine-only NSH call (`global`) made through the runner.
    pub(crate) async fn elsewhere(services: Instance<ServiceProvider>) -> String {
        let runner = runner(&services);
        let (send, receive) = futures::channel::oneshot::channel();
        std::thread::spawn(move || {
            let here = runner.is_engine_thread();
            let ran = futures::executor::block_on(runner.run(|| async {
                // A signal of the surface's: reachable on the engine thread only.
                let engine = std::thread::current().id();
                let count = global::<Visits>().with_untracked(|visits| visits.0 + 1);
                global::<Visits>().set(Visits(count));
                (engine, count)
            }));
            let answer = match ran {
                Ok((engine, count)) => format!(
                    "here {here} visits {count} other thread {}",
                    engine != std::thread::current().id()
                ),
                Err(error) => error.to_string(),
            };
            let _ = send.send(answer);
        });
        receive.await.unwrap()
    }

    /// On the engine thread, in a task: `run` makes its future at once and `post` runs at once.
    pub(crate) async fn here(services: Instance<ServiceProvider>) -> String {
        let runner = runner(&services);
        let order = Arc::new(Mutex::new(Vec::new()));
        let made = Arc::clone(&order);
        drop(runner.run(move || {
            made.lock().unwrap().push("made");
            async {}
        }));
        let posted = Arc::clone(&order);
        runner
            .post(move || posted.lock().unwrap().push("posted"))
            .unwrap();
        order.lock().unwrap().push("returned");
        format!(
            "here {} {}",
            runner.is_engine_thread(),
            order.lock().unwrap().join(",")
        )
    }

    /// In a synchronous call from JS, where NSH starts no task: `run` waits for the engine's
    /// next turn.
    pub(crate) fn in_js_call(services: Instance<ServiceProvider>) {
        let runner = runner(&services);
        drop(runner.run(|| {
            ORDER.lock().unwrap().push("made");
            async {}
        }));
        ORDER.lock().unwrap().push("returned");
    }

    /// What ran in and after the synchronous call, in order.
    pub(crate) fn order() -> String {
        ORDER.lock().unwrap().join(",")
    }

    /// How many jobs the runner queued so far.
    pub(crate) fn queued(services: Instance<ServiceProvider>) -> String {
        runner(&services).queued().to_string()
    }

    /// Keeps the runner for after the engine stopped, and work of it that never ends.
    pub(crate) fn keep(services: Instance<ServiceProvider>) {
        let runner = runner(&services);
        let pending = runner.clone();
        let started = std::thread::spawn(move || pending.run(futures::future::pending::<u32>))
            .join()
            .unwrap();
        *IN_FLIGHT.lock().unwrap() = Some(started);
        *KEPT.lock().unwrap() = Some(runner);
    }
}

/// How often the runner's work ran on this surface.
#[derive(Clone, Copy, Default, PartialEq)]
struct Visits(u32);

fn set() -> nsh_core::element::Registry {
    headless::set(|set| {
        set.module::<Probe>();
    })
}

const TSX: &str = r#"import { mount, signal, Text } from "nsh";
import { Probe, RayXAppBuilder } from "test/app";

const shown = signal("waiting");
setTimeout(async () => {
  const lines = [];
  try {
    const services = await new RayXAppBuilder().addHostFakes().start();
    lines.push(await Probe.elsewhere(services));
    lines.push(await Probe.elsewhere(services));
    lines.push(`queued ${Probe.queued(services)}`);
    lines.push(await Probe.here(services));
    Probe.inJsCall(services);
    await new Promise((resolve) => setTimeout(resolve, 10));
    lines.push(Probe.order());
    // The engine thread's work, in a task and in a call from JS, skipped the queue.
    lines.push(`queued ${Probe.queued(services)}`);
    Probe.keep(services);
  } catch (error) {
    lines.push(`threw ${error}`);
  }
  shown.set(`done ${lines.join("|")}`);
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn the_runner_reaches_the_engine_from_any_thread_until_it_is_gone() {
    assert_eq!(
        headless::run(set(), TSX),
        "done here false visits 1 other thread true\
         |here false visits 2 other thread true\
         |queued 2\
         |here true made,posted,returned\
         |returned,made\
         |queued 2"
    );
    let runner = KEPT.lock().unwrap().take().expect("kept");
    let answer = std::thread::spawn({
        let runner = runner.clone();
        move || futures::executor::block_on(runner.run(|| async { 1 }))
    })
    .join()
    .unwrap();
    assert_eq!(answer, Err(EngineGone));
    assert_eq!(runner.post(|| {}), Err(EngineGone));
    // Work queued or running when the engine stopped resolves too.
    let in_flight = IN_FLIGHT.lock().unwrap().take().expect("started");
    assert_eq!(futures::executor::block_on(in_flight), Err(EngineGone));
}
