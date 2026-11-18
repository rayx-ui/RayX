//! A Rust startup service whose factory reads a TS implementation of a contract, headless:
//! `start()` makes the TS implementation (every form: `addSingletonAs` and the startup forms)
//! before any Rust startup factory runs, so the factory reads its published state and follows
//! its events (as RayX's history does with the app's operations).

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod headless;

use std::sync::Mutex;

use futures::FutureExt as _;
use futures::future::LocalBoxFuture;
use nsh_core::prelude::*;
use rayx_app_nsh::di::{
    AsyncInitializable, StartupInitializable, StartupInitializationContext,
    StartupInitializationResult,
};
use rayx_app_nsh::ts::{ContractRegistryExt as _, RayXAppBuilder};
use rayx_app_nsh::{AppBuilder as _, ChangeSubscription, EventStream, StateCell};

/// The app's clock: written in TS.
#[rayx_app_nsh::contract]
pub trait Clock {
    /// The time now.
    #[state]
    fn time(&self) -> &StateCell<String>;
    /// Each tick, by number.
    #[events]
    fn ticks(&self) -> &EventStream<u32>;
}

/// What the Rust startup service saw, in order.
static SEEN: Mutex<Vec<String>> = Mutex::new(Vec::new());

fn seen(line: String) {
    SEEN.lock().unwrap().push(line);
}

/// A Rust startup service that, made by its factory, read the clock and follows its ticks.
struct Watch(#[allow(dead_code)] ChangeSubscription);

impl AsyncInitializable for Watch {
    fn is_initialized(&self) -> bool {
        true
    }

    fn initialize(
        &self,
        _context: StartupInitializationContext,
    ) -> LocalBoxFuture<'static, StartupInitializationResult> {
        futures::future::ready(Ok(())).boxed_local()
    }
}

impl StartupInitializable for Watch {}

/// The test's Rust side.
#[module]
struct Probe;

#[module]
impl Probe {
    /// Adds the Rust startup service whose factory reads the clock.
    pub(crate) fn add_watch(
        builder: Instance<RayXAppBuilder>,
    ) -> Result<Instance<RayXAppBuilder>, String> {
        builder
            .with_mut(|builder| {
                builder.extend(|mut builder| {
                    builder.services_mut().add_startup_scoped(|services| {
                        let clock = services.required_service::<dyn Clock>().unwrap();
                        seen(format!("made at {}", clock.time().get()));
                        Watch(clock.ticks().subscribe(|tick| seen(format!("tick {tick}"))))
                    });
                    builder
                })
            })
            .map_err(|error| error.to_string())?;
        Ok(builder)
    }

    /// What it saw, in order.
    pub(crate) fn seen() -> String {
        SEEN.lock().unwrap().join(",")
    }
}

const TSX: &str = r#"import { mount, signal, Text } from "nsh";
import { ContractEvents, ContractState, Probe, RayXAppBuilder, clockToken } from "test/app";

class Clock {
  time = new ContractState("noon");
  ticks = new ContractEvents();
}

class StartingClock extends Clock {
  isInitialized() { return true; }
  async initialize() {}
}

const shown = signal("waiting");
setTimeout(async () => {
  let clock;
  try {
    const make = () => (clock = new @CLASS@());
    // The Rust service is registered first: its factory still runs after the TS object is made.
    await Probe.addWatch(new RayXAppBuilder().addHostFakes()).@FORM@(clockToken(), make).start();
    clock.ticks.publish(1);
    shown.set(`done ${Probe.seen()}`);
  } catch (error) {
    shown.set(`done threw ${error}`);
  }
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

/// What the Rust startup service saw with the TS implementation registered with `form`.
fn seen_with(form: &str) -> String {
    SEEN.lock().unwrap().clear();
    let set = headless::set(|set| {
        set.module::<Probe>().contract::<dyn Clock>();
    });
    let class = if form == "addSingletonAs" {
        "Clock"
    } else {
        "StartingClock"
    };
    headless::run(set, &TSX.replace("@FORM@", form).replace("@CLASS@", class))
}

#[test]
fn a_rust_startup_factory_reads_a_ts_implementation_start_made_first() {
    // One at a time: what the Rust service saw is kept in one static.
    for form in [
        "addSingletonAs",
        "addStartupSingletonAs",
        "addStartupScopedAs",
    ] {
        assert_eq!(seen_with(form), "done made at noon,tick 1", "{form}");
    }
}
