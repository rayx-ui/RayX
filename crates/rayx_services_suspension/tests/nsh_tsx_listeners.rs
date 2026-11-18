//! Listeners of the `SuspensionService` TS class through NSH's engine, headless, over any
//! `dyn SuspensionService` (a fake here): a provider gives every consumer the same object, so
//! each `onLifecycle` and `askBeforeQuit` returns a function that stops that listener alone;
//! another consumer's keeps hearing, a stopped `askBeforeQuit` no longer keeps a quit request
//! from quitting, and disposing the object stops what it still holds.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::{Arc, OnceLock};

use nsh_core::prelude::*;
use rayx_app::AppBuilder as _;
use rayx_app::services::suspension::{
    SuspensionEvent, SuspensionEventKind, SuspensionEventReason, SuspensionService,
};
use rayx_app::testing::FakeSuspension;
use rayx_app_nsh::testing;
use rayx_app_nsh::ts::RayXAppBuilder;

/// The app's suspension service: a fake the test drives.
fn fake() -> &'static Arc<FakeSuspension> {
    static FAKE: OnceLock<Arc<FakeSuspension>> = OnceLock::new();
    FAKE.get_or_init(FakeSuspension::new)
}

/// `builder.addTestSuspension()`: the fake as the app's `dyn SuspensionService`.
struct TestSuspension;

#[class_extension(RayXAppBuilder)]
impl TestSuspension {
    pub fn add_test_suspension(
        this: Instance<RayXAppBuilder>,
    ) -> Result<Instance<RayXAppBuilder>, String> {
        RayXAppBuilder::extend_with(this, |mut builder| {
            builder
                .services_mut()
                .replace_service_factory_as::<dyn SuspensionService>(|_| {
                    Arc::clone(fake()) as Arc<dyn SuspensionService>
                });
            builder
        })
    }
}

/// What the test changes and reads of the fake from Rust.
#[module]
struct Probe;

#[module]
impl Probe {
    /// Publishes a wake from sleep.
    pub(crate) fn wake() {
        fake().emit(SuspensionEvent::new(
            SuspensionEventKind::Woke,
            SuspensionEventReason::SystemWoke,
        ));
    }

    /// Whether a quit request comes to the app instead of quitting it.
    pub(crate) fn asks_before_quit() -> bool {
        fake().suspension().get().asks_before_quit
    }

    /// How many times the app asked to quit.
    pub(crate) fn quits() -> f64 {
        fake().quits() as f64
    }
}

const TSX: &str = r#"import { mount, signal, Text } from "nsh";
import { Probe, RayXAppBuilder, SuspensionService } from "test/app";

const shown = signal("waiting");
const tick = () => new Promise((resolve) => setTimeout(resolve, 10));
setTimeout(async () => {
  const lines = [];
  try {
    const services = await new RayXAppBuilder().addTestSuspension().addHostFakes().start();

    const suspension = services.getRequiredService(SuspensionService);
    lines.push(String(suspension === services.getRequiredService(SuspensionService)));
    const wakes = { first: 0, second: 0 };
    const isWake = (event) => event.kind === "Woke";
    const stopWake = suspension.onLifecycle((event) => { if (isWake(event)) wakes.first++; });
    suspension.onLifecycle((event) => { if (isWake(event)) wakes.second++; });
    Probe.wake();
    await tick();
    stopWake();
    Probe.wake();
    await tick();
    lines.push(`wakes ${wakes.first} / ${wakes.second}`);

    // A stopped `askBeforeQuit` no longer keeps a quit request from quitting.
    const asks = [Probe.asksBeforeQuit()];
    const stopAsking = suspension.askBeforeQuit(() => {});
    const stopAlsoAsking = services.getRequiredService(SuspensionService).askBeforeQuit(() => {});
    await tick();
    asks.push(Probe.asksBeforeQuit());
    stopAsking();
    await tick();
    asks.push(Probe.asksBeforeQuit());
    stopAlsoAsking();
    await tick();
    asks.push(Probe.asksBeforeQuit());
    lines.push(`asks ${asks}`);

    // Disposing the object stops what it still holds.
    suspension.askBeforeQuit(() => {});
    await tick();
    const before = Probe.asksBeforeQuit();
    suspension.dispose();
    await tick();
    lines.push(`disposed ${before} ${Probe.asksBeforeQuit()}`);
    lines.push(String(services.getRequiredService(SuspensionService) !== suspension));

    services.getRequiredService(SuspensionService).quit();
    await tick();
    lines.push(`quits ${Probe.quits()}`);
  } catch (error) {
    lines.push(`threw ${error}`);
  }
  shown.set(`done ${lines.join("|")}`);
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

fn set() -> nsh_core::element::Registry {
    testing::set(|set| {
        rayx_services_suspension::register(set);
        set.class_extension::<TestSuspension>().module::<Probe>();
    })
}

#[test]
fn each_consumer_stops_its_own_listener_of_the_suspension_object() {
    assert_eq!(
        testing::run(set(), TSX),
        "done true|wakes 1 / 2|asks false,true,true,false|disposed true false|true|quits 1"
    );
}

#[test]
fn listener_members_are_declared_to_return_their_stop_function() {
    let ts = testing::declarations(&set());
    for declared in [
        "  onLifecycle(listener: (event: SuspensionEvent) => void): () => void;",
        "  askBeforeQuit(listener: () => void): () => void;",
    ] {
        assert!(ts.contains(declared), "no `{declared}` in\n{ts}");
    }
}
