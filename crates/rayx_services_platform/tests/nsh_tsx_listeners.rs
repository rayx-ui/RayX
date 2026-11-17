//! Listeners of the `PlatformInfoService` TS class through NSH's engine, headless, over any
//! `dyn PlatformInfoService` (a fake here): a provider gives every consumer the same object, so
//! each `onChange` returns a function that stops that listener alone; another consumer's keeps
//! hearing.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::{Arc, OnceLock};

use nsh_core::prelude::*;
use rayx_app::AppBuilder as _;
use rayx_app::services::platform::{PlatformInfoService, PlatformSnapshot};
use rayx_app::testing::FakePlatform;
use rayx_app_nsh::testing;
use rayx_app_nsh::ts::RayXAppBuilder;

/// The app's platform service: a fake the test drives.
fn fake() -> &'static Arc<FakePlatform> {
    static FAKE: OnceLock<Arc<FakePlatform>> = OnceLock::new();
    FAKE.get_or_init(|| FakePlatform::new(PlatformSnapshot::default()))
}

/// `builder.addTestPlatform()`: the fake as the app's `dyn PlatformInfoService`.
struct TestPlatform;

#[class_extension(RayXAppBuilder)]
impl TestPlatform {
    pub fn add_test_platform(
        this: Instance<RayXAppBuilder>,
    ) -> Result<Instance<RayXAppBuilder>, String> {
        RayXAppBuilder::extend_with(this, |mut builder| {
            builder
                .services_mut()
                .replace_service_factory_as::<dyn PlatformInfoService>(|_| {
                    Arc::clone(fake()) as Arc<dyn PlatformInfoService>
                });
            builder
        })
    }
}

/// What the test changes of the fake from Rust.
#[module]
struct Probe;

#[module]
impl Probe {
    /// Sets the platform's viewport width to `width`.
    pub(crate) fn resize(width: f64) {
        let mut snapshot = (*fake().platform().get()).clone();
        snapshot.viewport.width = width as f32;
        fake().publish(snapshot);
    }
}

const TSX: &str = r#"import { mount, signal, Text } from "nsh";
import { PlatformInfoService, Probe, RayXAppBuilder } from "test/app";

const shown = signal("waiting");
const tick = () => new Promise((resolve) => setTimeout(resolve, 10));
setTimeout(async () => {
  const lines = [];
  try {
    const services = await new RayXAppBuilder().addTestPlatform().addHostFakes().start();
    const platform = services.getRequiredService(PlatformInfoService);
    lines.push(String(platform === services.getRequiredService(PlatformInfoService)));
    const widths = { first: [], second: [] };
    const stopWidth = platform.onChange((snapshot) => widths.first.push(snapshot.viewport.width));
    platform.onChange((snapshot) => widths.second.push(snapshot.viewport.width));
    Probe.resize(500);
    await tick();
    stopWidth();
    Probe.resize(600);
    await tick();
    lines.push(`widths ${widths.first} / ${widths.second}`);
    lines.push(String(platform.current().viewport.width));
  } catch (error) {
    lines.push(`threw ${error}`);
  }
  shown.set(`done ${lines.join("|")}`);
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

fn set() -> nsh_core::element::Registry {
    testing::set(|set| {
        rayx_services_platform::register(set);
        set.class_extension::<TestPlatform>().module::<Probe>();
    })
}

#[test]
fn each_consumer_stops_its_own_listener_of_the_platform_object() {
    assert_eq!(
        testing::run(set(), TSX),
        "done true|widths 500 / 500,600|600"
    );
}

#[test]
fn the_listener_member_is_declared_to_return_its_stop_function() {
    let ts = testing::declarations(&set());
    let declared = "  onChange(listener: (snapshot: unknown) => void): () => void;";
    assert!(ts.contains(declared), "no `{declared}` in\n{ts}");
}
