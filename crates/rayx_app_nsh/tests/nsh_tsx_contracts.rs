//! `#[contract]`: a Rust trait TS implements, through NSH's engine, headless. Rust resolves the
//! TS implementation as `Arc<dyn Trait>` and awaits its results and errors; a method returning
//! nothing runs on the spot in a call from JS and is posted in order otherwise; off the engine
//! thread a call fails with `ContractError::WrongThread`; Rust startup services depending on
//! the contract initialize after the TS implementation; the set declares the interface and the
//! token function.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod headless;

use std::cell::RefCell;
use std::sync::Arc;

use futures::FutureExt as _;
use futures::future::LocalBoxFuture;
use nsh_core::prelude::*;
use rayx_app_nsh::AppBuilder as _;
use rayx_app_nsh::di::{
    AsyncInitializable, StartupInitializable, StartupInitializationContext,
    StartupInitializationResult, StartupServiceError, StartupServiceType,
};
use rayx_app_nsh::ts::{
    ContractError, ContractRegistryExt as _, Exception, RayXAppBuilder, ServiceProvider,
};

/// Why a forecast failed.
#[derive(Debug)]
pub enum WeatherError {
    /// The call did not reach TS.
    Contract(ContractError),
    /// What TS threw.
    Js(String),
}

impl From<ContractError> for WeatherError {
    fn from(error: ContractError) -> Self {
        Self::Contract(error)
    }
}

impl From<Exception> for WeatherError {
    fn from(exception: Exception) -> Self {
        Self::Js(exception.message)
    }
}

/// Where forecasts come from: written in TS.
#[rayx_app_nsh::contract]
pub trait WeatherSource {
    /// The forecast for `city`.
    async fn forecast(&self, city: String) -> Result<String, WeatherError>;
    /// Notes `line`.
    fn note(&self, line: String);
}

thread_local! {
    /// What ran, on the engine's thread (each test has its own engine).
    static LOG: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

fn log(line: impl Into<String>) {
    LOG.with_borrow_mut(|log| log.push(line.into()));
}

/// A Rust startup service that depends on the contract and calls it while it initializes.
struct Reporter;

impl AsyncInitializable for Reporter {
    fn is_initialized(&self) -> bool {
        false
    }

    fn initialize(
        &self,
        context: StartupInitializationContext,
    ) -> LocalBoxFuture<'static, StartupInitializationResult> {
        let source = context.services().required_service::<dyn WeatherSource>();
        async move {
            let source = source.map_err(|error| StartupServiceError::new(error.to_string()))?;
            let forecast = source
                .forecast("Zagreb".into())
                .await
                .map_err(|error| StartupServiceError::new(format!("{error:?}")))?;
            log(format!("reporter {forecast}"));
            Ok(())
        }
        .boxed_local()
    }
}

impl StartupInitializable for Reporter {
    fn initialization_depends_on(&self) -> Vec<StartupServiceType> {
        vec![StartupServiceType::of::<dyn WeatherSource>()]
    }
}

thread_local! {
    /// The contract `Injected` got from its factory.
    static INJECTED: RefCell<Option<Arc<dyn WeatherSource>>> = const { RefCell::new(None) };
}

/// A Rust startup service that gets the contract from its factory (before `start()` made the
/// TS implementation), depends on it and calls it while it initializes.
struct Injected {
    source: Arc<dyn WeatherSource>,
}

impl AsyncInitializable for Injected {
    fn is_initialized(&self) -> bool {
        false
    }

    fn initialize(
        &self,
        _context: StartupInitializationContext,
    ) -> LocalBoxFuture<'static, StartupInitializationResult> {
        let source = Arc::clone(&self.source);
        async move {
            let forecast = source
                .forecast("Osijek".into())
                .await
                .map_err(|error| StartupServiceError::new(format!("{error:?}")))?;
            log(format!("injected {forecast}"));
            INJECTED.with_borrow_mut(|kept| *kept = Some(source));
            Ok(())
        }
        .boxed_local()
    }
}

impl StartupInitializable for Injected {
    fn initialization_depends_on(&self) -> Vec<StartupServiceType> {
        vec![StartupServiceType::of::<dyn WeatherSource>()]
    }
}

/// A Rust startup service with nothing to initialize, whose factory keeps the contract.
struct Capturing;

impl AsyncInitializable for Capturing {
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

impl StartupInitializable for Capturing {}

/// The test's Rust side.
#[module]
struct Probe;

#[module]
impl Probe {
    /// Adds the Rust startup service that depends on the contract.
    pub(crate) fn add_reporter(
        builder: Instance<RayXAppBuilder>,
    ) -> Result<Instance<RayXAppBuilder>, String> {
        builder
            .with_mut(|builder| {
                builder.extend(|mut builder| {
                    builder
                        .services_mut()
                        .add_startup_service(Arc::new(Reporter));
                    builder
                })
            })
            .map_err(|error| error.to_string())?;
        Ok(builder)
    }

    /// Adds the Rust startup service whose factory resolves the contract.
    pub(crate) fn add_injected(
        builder: Instance<RayXAppBuilder>,
    ) -> Result<Instance<RayXAppBuilder>, String> {
        builder
            .with_mut(|builder| {
                builder.extend(|mut builder| {
                    builder
                        .services_mut()
                        .add_startup_scoped(|services| Injected {
                            source: services.required_service::<dyn WeatherSource>().unwrap(),
                        });
                    builder
                })
            })
            .map_err(|error| error.to_string())?;
        Ok(builder)
    }

    /// Adds a Rust startup service whose factory only keeps the contract it resolves (before
    /// `start()`, and before anything made the TS object), for `injectedForecast`.
    pub(crate) fn add_capturing(
        builder: Instance<RayXAppBuilder>,
    ) -> Result<Instance<RayXAppBuilder>, String> {
        builder
            .with_mut(|builder| {
                builder.extend(|mut builder| {
                    builder.services_mut().add_startup_scoped(|services| {
                        let source = services.required_service::<dyn WeatherSource>().unwrap();
                        INJECTED.with_borrow_mut(|kept| *kept = Some(source));
                        Capturing
                    });
                    builder
                })
            })
            .map_err(|error| error.to_string())?;
        Ok(builder)
    }

    /// What the contract `Injected` got from its factory gives for `city`, after start.
    pub(crate) async fn injected_forecast(city: String) -> String {
        let Some(source) = INJECTED.with_borrow(Clone::clone) else {
            return "not injected".into();
        };
        match source.forecast(city).await {
            Ok(forecast) => forecast,
            Err(error) => format!("{error:?}"),
        }
    }

    /// What Rust gets from the TS implementation's `forecast(city)`.
    pub(crate) async fn forecast(services: Instance<ServiceProvider>, city: String) -> String {
        let source =
            services.with(|services| services.services().required_service::<dyn WeatherSource>());
        match source {
            Ok(source) => match source.forecast(city).await {
                Ok(forecast) => forecast,
                Err(error) => format!("{error:?}"),
            },
            Err(error) => error.to_string(),
        }
    }

    /// Notes `line` through the contract, inside this call from JS (on the spot).
    pub(crate) fn note_now(
        services: Instance<ServiceProvider>,
        line: String,
    ) -> Result<(), String> {
        let source = services
            .with(|services| services.services().required_service::<dyn WeatherSource>())
            .map_err(|error| error.to_string())?;
        source.note(line);
        log("noted now");
        Ok(())
    }

    /// Notes `lines` through the contract from a task (posted, in order).
    pub(crate) async fn note_later(services: Instance<ServiceProvider>, lines: Vec<String>) {
        let source =
            services.with(|services| services.services().required_service::<dyn WeatherSource>());
        if let Ok(source) = source {
            for line in lines {
                source.note(line);
            }
            log("posted");
        }
    }

    /// What a call off the engine thread gives.
    pub(crate) fn off_thread(services: Instance<ServiceProvider>) -> String {
        let provider = services.with(|services| services.services().clone());
        std::thread::spawn(move || {
            let source = provider.required_service::<dyn WeatherSource>().unwrap();
            match source.forecast("Split".into()).now_or_never() {
                Some(Ok(forecast)) => forecast,
                Some(Err(error)) => format!("{error:?}"),
                None => "pending".to_owned(),
            }
        })
        .join()
        .unwrap()
    }

    /// What ran, in order.
    pub(crate) fn logged() -> String {
        LOG.with_borrow(|log| log.join(", "))
    }

    /// Logs `line`.
    pub(crate) fn log(line: String) {
        log(line);
    }
}

fn set() -> nsh_core::element::Registry {
    headless::set(|set| {
        set.module::<Probe>().contract::<dyn WeatherSource>();
    })
}

const TSX: &str = r#"import { mount, signal, Text } from "nsh";
import { Probe, RayXAppBuilder, weatherSourceToken } from "test/app";

class OpenMeteo {
  constructor() { Probe.log("made"); }
  async forecast(city) {
    if (city === "nowhere") throw new Error(`no forecast for ${city}`);
    return `sunny in ${city}`;
  }
  note(line) { Probe.log(`note ${line}`); }
}

const shown = signal("waiting");
setTimeout(async () => {
  const lines = [];
  try {
    const services = await Probe.addReporter(
      new RayXAppBuilder().addSingletonAs(weatherSourceToken(), () => new OpenMeteo()),
    ).start();
    // Made by `start()`, before the Rust startup service that depends on it, which calls it.
    lines.push(Probe.logged());
    // Results and errors back.
    lines.push(await Probe.forecast(services, "Rijeka"));
    lines.push(await Probe.forecast(services, "nowhere"));
    // TS resolves the same object Rust calls.
    lines.push(String(services.getRequiredService(weatherSourceToken()) instanceof OpenMeteo));
    lines.push(String(weatherSourceToken() === weatherSourceToken()));
    // A method returning nothing: on the spot in a call from JS, posted in order otherwise.
    Probe.noteNow(services, "one");
    lines.push(Probe.logged().split(", ").slice(-2).join(", "));
    await Probe.noteLater(services, ["two", "three"]);
    await new Promise((resolve) => setTimeout(resolve, 0));
    lines.push(Probe.logged().split(", ").slice(-3).join(", "));
    // Off the engine thread.
    lines.push(Probe.offThread(services));
  } catch (error) {
    lines.push(`threw ${error}`);
  }
  shown.set(`done ${lines.join("|")}`);
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn rust_calls_a_ts_implementation_through_the_contract() {
    assert_eq!(
        headless::run(set(), TSX),
        "done made, reporter sunny in Zagreb\
         |sunny in Rijeka\
         |Js(\"no forecast for nowhere\")\
         |true|true\
         |note one, noted now\
         |posted, note two, note three\
         |Contract(WrongThread { contract: \"WeatherSource\" })"
    );
}

const STARTUP: &str = r#"import { mount, signal, Text } from "nsh";
import { Probe, RayXAppBuilder, weatherSourceToken } from "test/app";

class Slow {
  isInitialized() { return false; }
  async initialize() {
    await new Promise((resolve) => setTimeout(resolve, 20));
    Probe.log("ts initialized");
  }
  async forecast(city) { return `rain in ${city}`; }
  note() {}
}

const shown = signal("waiting");
Probe.addReporter(new RayXAppBuilder().addStartupSingletonAs(weatherSourceToken(), () => new Slow()))
  .start()
  .then(
    () => shown.set(`done ${Probe.logged()}`),
    (error) => shown.set(`done rejected ${error.message}`),
  );
mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn a_rust_startup_service_depending_on_the_contract_initializes_after_the_ts_one() {
    assert_eq!(
        headless::run(set(), STARTUP),
        "done ts initialized, reporter rain in Zagreb"
    );
}

#[test]
fn the_set_declares_the_contract_s_interface_and_token() {
    let set = set();
    let ts = headless::declarations(&set);
    for declared in [
        "/** Where forecasts come from: written in TS. */\nexport interface WeatherSource {",
        "  /** The forecast for `city`. */\n  forecast(city: string): Promise<string>;",
        "  note(line: string): void;",
        "export declare function weatherSourceToken(): ServiceToken<WeatherSource>;",
    ] {
        assert!(ts.contains(declared), "no `{declared}` in\n{ts}");
    }
    assert_eq!(set.undocumented(), Vec::<String>::new());
}

const INJECTED_TSX: &str = r#"import { mount, signal, Text } from "nsh";
import { Probe, RayXAppBuilder, weatherSourceToken } from "test/app";

class OpenMeteo {
  isInitialized() { return false; }
  initialize() { Probe.log("ts initialized"); }
  async forecast(city) { return `fog in ${city}`; }
  note() {}
}

const shown = signal("waiting");
setTimeout(async () => {
  const lines = [];
  try {
    const services = await Probe.addInjected(
      new RayXAppBuilder().@ADD@(weatherSourceToken(), () => new OpenMeteo()),
    ).start();
    lines.push(Probe.logged());
    // Later Rust calls reach JS: through the adapter the factory got, and a fresh resolve.
    lines.push(await Probe.injectedForecast("Pula"));
    lines.push(await Probe.forecast(services, "Zadar"));
  } catch (error) {
    lines.push(`threw ${error}`);
  }
  shown.set(`done ${lines.join("|")}`);
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn a_rust_startup_factory_resolving_the_contract_before_it_is_made_gets_a_working_adapter() {
    for (add, logged) in [
        ("addSingletonAs", "injected fog in Osijek"),
        (
            "addStartupSingletonAs",
            "ts initialized, injected fog in Osijek",
        ),
        (
            "addStartupScopedAs",
            "ts initialized, injected fog in Osijek",
        ),
    ] {
        assert_eq!(
            headless::run(set(), &INJECTED_TSX.replace("@ADD@", add)),
            format!("done {logged}|fog in Pula|fog in Zadar"),
            "{add}"
        );
    }
}

const NAMED_KEY: &str = r#"import { mount, signal, Text } from "nsh";
import { Probe, RayXAppBuilder, ServiceToken, weatherSourceToken } from "test/app";

const Named = new ServiceToken("Named");
let asked = 0;
class OpenMeteo {
  isInitialized() { return false; }
  initialize() { Probe.log("ts initialized"); }
  startupServiceType() { asked++; return Named; }
  async forecast(city) { return `wind in ${city}`; }
  note() {}
}

const shown = signal("waiting");
Probe.addReporter(new RayXAppBuilder().addStartupSingletonAs(weatherSourceToken(), () => new OpenMeteo()))
  .start()
  .then(
    () => shown.set(`done ${asked} ${Probe.logged()}`),
    (error) => shown.set(`done rejected ${error.message}`),
  );
mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn a_contract_s_startup_implementation_stays_keyed_by_its_contract() {
    assert_eq!(
        headless::run(set(), NAMED_KEY),
        "done 1 ts initialized, reporter wind in Zagreb"
    );
}

const CAPTURED: &str = r#"import { mount, signal, Text } from "nsh";
import { Probe, RayXAppBuilder, weatherSourceToken } from "test/app";

class OpenMeteo {
  async forecast(city) { return `hail in ${city}`; }
  note() {}
}

const shown = signal("waiting");
setTimeout(async () => {
  const lines = [];
  try {
    const services = await Probe.addCapturing(
      new RayXAppBuilder().addScopedAs(weatherSourceToken(), () => new OpenMeteo()),
    ).start();
    // Resolved by Rust from the root before the root scope's object was made: not made yet.
    lines.push(await Probe.injectedForecast("Pula"));
    // Made by a TS resolve in the root scope: the adapter Rust kept reaches it now.
    lines.push(String(services.getRequiredService(weatherSourceToken()) instanceof OpenMeteo));
    lines.push(await Probe.injectedForecast("Pula"));
  } catch (error) {
    lines.push(`threw ${error}`);
  }
  shown.set(`done ${lines.join("|")}`);
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn a_scoped_adapter_resolved_before_its_object_was_made_reaches_it_once_made() {
    let shown = headless::run(set(), CAPTURED);
    assert!(
        shown.starts_with("done Contract(Unavailable { contract: \"WeatherSource\""),
        "{shown}"
    );
    assert!(shown.contains("not made yet"), "{shown}");
    assert!(shown.ends_with("|true|hail in Pula"), "{shown}");
}
