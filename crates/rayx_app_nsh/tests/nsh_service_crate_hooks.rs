//! What a service crate builds on, with a stand-in service crate in the test, headless through
//! `rayx_app_nsh::testing` (feature `testing`): its TS builder method, a class extension of the
//! host's `RayXAppBuilder` registered by its set (in JS and in the declarations only with that
//! set), acting on the builder through the host's public functions (its Rust registration, its
//! options class recorded and replayed, a throwing callback, a started builder), and its service
//! class resolving through the provider.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::Arc;

use nsh_core::prelude::*;
use rayx_app_nsh::AppBuilder as _;
use rayx_app_nsh::testing;
use rayx_app_nsh::ts::{RayXAppBuilder, ServiceClass, ServiceClassRegistryExt as _, record};

/// The stand-in crate's service: forecasts for its cities.
pub struct Weather {
    cities: Vec<String>,
}

/// Its options, as Rust's would be.
#[derive(Clone, Debug, Default)]
struct WeatherSettings {
    cities: Vec<String>,
}

/// The weather service's options for TSX (`addWeatherService`): each method records a step.
#[class]
pub struct WeatherOptions {
    steps: Vec<String>,
}

impl WeatherOptions {
    fn new() -> Self {
        Self { steps: Vec::new() }
    }

    fn take(&mut self) -> Vec<String> {
        std::mem::take(&mut self.steps)
    }
}

#[class]
impl WeatherOptions {
    /// Adds a city.
    pub fn city(this: Instance<Self>, name: String) -> Instance<Self> {
        this.with_mut(|options| options.steps.push(name));
        this
    }
}

/// The weather service for TSX: `services.getRequiredService(WeatherService)`.
#[class]
pub struct WeatherService {
    weather: Arc<Weather>,
}

impl ServiceClass for WeatherService {
    type Service = Weather;

    fn wrap(weather: Arc<Weather>) -> Self {
        Self { weather }
    }
}

#[class]
impl WeatherService {
    /// The cities it forecasts.
    pub fn cities(&self) -> Vec<String> {
        self.weather.cities.clone()
    }
}

/// The methods the stand-in crate adds to the app builder.
pub struct WeatherBuilder;

#[class_extension(RayXAppBuilder)]
impl WeatherBuilder {
    /// Registers the weather service, configured by `configure`.
    pub fn add_weather_service(
        this: Instance<RayXAppBuilder>,
        #[args(options)] configure: Option<Callback<Instance<WeatherOptions>>>,
    ) -> Result<Instance<RayXAppBuilder>, String> {
        let steps = record(WeatherOptions::new(), configure, WeatherOptions::take)?;
        RayXAppBuilder::extend_with(this, move |mut builder| {
            let mut settings = WeatherSettings::default();
            settings.cities.extend(steps);
            builder.services_mut().add_singleton(Weather {
                cities: settings.cities,
            });
            builder
        })
    }
}

/// The stand-in crate's set: its classes, resolvers and builder method.
fn register(registry: &mut nsh_core::element::Registry) {
    registry
        .class::<WeatherOptions>()
        .service_class::<WeatherService>()
        .class_extension::<WeatherBuilder>();
}

/// The test's Rust side.
#[module]
struct Probe;

#[module]
impl Probe {
    /// The cities Rust resolves.
    pub(crate) fn cities(services: Instance<rayx_app_nsh::ts::ServiceProvider>) -> String {
        services
            .with(|services| services.services().required_service::<Weather>())
            .map(|weather| weather.cities.join(","))
            .unwrap_or_else(|error| error.to_string())
    }
}

const TSX: &str = r#"import { mount, signal, Text } from "nsh";
import { Probe, RayXAppBuilder, WeatherService } from "test/app";

const shown = signal("waiting");
const thrown = (f) => { try { f(); return "no throw"; } catch (e) { return e.message; } };
setTimeout(async () => {
  const lines = [];
  try {
    lines.push(typeof RayXAppBuilder.prototype.addWeatherService);
    // With options, chained with RayX's own methods.
    const builder = new RayXAppBuilder().addHostFakes()
      .addStorage()
      .addWeatherService((options) => { options.city("Zagreb").city("Split"); });
    // A callback that throws: the method throws its message.
    lines.push(thrown(() => new RayXAppBuilder().addHostFakes().addWeatherService(() => { throw new Error("no cities"); })));
    const services = await builder.start();
    lines.push(Probe.cities(services));
    // Its class resolves through the provider, the same object each time.
    const weather = services.getRequiredService(WeatherService);
    lines.push(`${weather instanceof WeatherService} ${weather === services.getRequiredService(WeatherService)} ${weather.cities().join(",")}`);
    // After `start()`, it throws.
    lines.push(thrown(() => builder.addWeatherService()).includes("started") ? "started" : "open");
  } catch (error) {
    lines.push(`threw ${error}`);
  }
  shown.set(`done ${lines.join("|")}`);
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn a_service_crate_s_builder_method_acts_on_the_builder() {
    let set = testing::set(|set| {
        set.module::<Probe>();
        register(set);
    });
    assert_eq!(
        testing::run(set, TSX),
        "done function|no cities|Zagreb,Split|true true Zagreb,Split|started"
    );
}

#[test]
fn without_options_the_service_takes_its_defaults() {
    let set = testing::set(|set| {
        set.module::<Probe>();
        register(set);
    });
    let shown = testing::run(
        set,
        r#"import { mount, signal, Text } from "nsh";
import { Probe, RayXAppBuilder } from "test/app";
const shown = signal("waiting");
new RayXAppBuilder().addHostFakes()
  .addWeatherService()
  .start()
  .then((services) => shown.set(`done [${Probe.cities(services)}]`));
mount(() => <Text>{shown()}</Text>);
"#,
    );
    assert_eq!(shown, "done []");
}

#[test]
fn the_method_exists_only_with_the_crate_s_set() {
    let with = testing::set(|set| {
        set.module::<Probe>();
        register(set);
    });
    let ts = testing::declarations(&with);
    assert!(
        ts.contains(
            "  /** Registers the weather service, configured by `configure`. */\n  \
             addWeatherService(configure?: ((options: WeatherOptions) => void) | null): \
             RayXAppBuilder;"
        ),
        "{ts}"
    );
    let without = testing::set(|set| {
        set.module::<Probe>();
    });
    assert!(!testing::declarations(&without).contains("addWeatherService("));
    let shown = testing::run(
        without,
        r#"import { mount, signal, Text } from "nsh";
import { RayXAppBuilder } from "test/app";
const shown = signal(`done ${typeof RayXAppBuilder.prototype.addWeatherService}`);
mount(() => <Text>{shown()}</Text>);
"#,
    );
    assert_eq!(shown, "done undefined");
}
