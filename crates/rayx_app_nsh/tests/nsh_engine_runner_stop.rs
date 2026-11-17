//! The surface's engine runner in Rust code JS runs outside a call, headless: a stop function
//! JS calls can start no task of NSH's core, so `run` and `post` there queue their work for the
//! engine (as from another thread) instead of giving up with the engine still running.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod headless;

use std::sync::Mutex;

use nsh_core::prelude::*;
use rayx_app_nsh::EngineRunner;
use rayx_app_nsh::ts::ServiceProvider;

/// What ran, in order.
static LOG: Mutex<Vec<String>> = Mutex::new(Vec::new());

fn log(line: impl Into<String>) {
    LOG.lock().unwrap().push(line.into());
}

/// The test's Rust side.
#[module]
struct Probe;

#[module]
impl Probe {
    /// A stop function that runs and posts work through the runner, and says whether each was
    /// taken (`Err(EngineGone)` when not).
    pub(crate) fn stopper(services: Instance<ServiceProvider>) -> Stop {
        let runner = services
            .with(|services| services.services().required_service::<EngineRunner>())
            .unwrap();
        let runner = (*runner).clone();
        Stop::new(move || {
            let ran = runner.run(|| async { log("ran") });
            // Nothing awaits it: the work is taken (queued) already.
            let taken = futures::FutureExt::now_or_never(ran);
            log(format!(
                "run {}",
                if taken == Some(Err(rayx_app_nsh::EngineGone)) {
                    "gone"
                } else {
                    "taken"
                }
            ));
            let posted = runner.post(|| log("posted"));
            log(format!(
                "post {}",
                if posted.is_ok() { "taken" } else { "gone" }
            ));
        })
    }

    /// What ran, in order.
    pub(crate) fn logged() -> String {
        LOG.lock().unwrap().join(",")
    }
}

const TSX: &str = r#"import { mount, signal, Text } from "nsh";
import { Probe, RayXAppBuilder } from "test/app";

const shown = signal("waiting");
setTimeout(async () => {
  try {
    const services = await new RayXAppBuilder().addHostFakes().start();
    const stop = Probe.stopper(services);
    stop();
    for (let i = 0; i < 50 && !(Probe.logged().includes("posted") && Probe.logged().includes("ran")); i++) {
      await new Promise((resolve) => setTimeout(resolve, 5));
    }
    shown.set(`done ${Probe.logged()}`);
  } catch (error) {
    shown.set(`done threw ${error}`);
  }
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn a_stop_function_s_work_is_queued_for_the_engine() {
    let set = headless::set(|set| {
        set.module::<Probe>();
    });
    let shown = headless::run(set, TSX);
    // Both taken at once, both run later (the run's work as a task of its own, so in any order).
    assert!(shown.starts_with("done run taken,post taken,"), "{shown}");
    let mut ran: Vec<&str> = shown.split(',').skip(2).collect();
    ran.sort_unstable();
    assert_eq!(ran, ["posted", "ran"], "{shown}");
}
