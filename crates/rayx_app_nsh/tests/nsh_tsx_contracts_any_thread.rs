//! A contract implemented in TS, called from any thread, headless: a command and a posted
//! member from a spawned thread reach the TS implementation through the surface's queue, with
//! results and thrown errors back; on the engine thread a call skips the queue; once the engine
//! stopped, a call resolves `Err(ContractError::EngineGone)` instead of hanging.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod headless;

use std::sync::{Arc, Mutex};

use futures::future::BoxFuture;

use nsh_core::prelude::*;
use rayx_app_nsh::ts::{ContractError, ContractRegistryExt as _, Exception, ServiceProvider};

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

/// The contract, kept past the engine's life.
static KEPT: Mutex<Option<Arc<dyn WeatherSource>>> = Mutex::new(None);

/// A call started in a synchronous call from JS.
static IN_CALL: Mutex<Option<BoxFuture<'static, Result<String, WeatherError>>>> = Mutex::new(None);

/// A call still running when the engine stops.
static IN_FLIGHT: Mutex<Option<BoxFuture<'static, Result<String, WeatherError>>>> =
    Mutex::new(None);

/// What ran on the Rust side, in order.
static LOG: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// What the transient implementations noted, in order (its test runs beside the others).
static FRESH: Mutex<Vec<String>> = Mutex::new(Vec::new());

fn source(services: &Instance<ServiceProvider>) -> rayx_app::ServiceProvider {
    services.with(|services| services.services().clone())
}

/// Runs `call` on a thread of its own and awaits what it gives, without blocking the engine.
async fn on_another_thread<T: Send + 'static>(call: impl FnOnce() -> T + Send + 'static) -> T {
    let (send, receive) = futures::channel::oneshot::channel();
    std::thread::spawn(move || {
        let _ = send.send(call());
    });
    receive.await.unwrap()
}

/// The test's Rust side.
#[module]
struct Probe;

#[module]
impl Probe {
    /// `forecast(city)` called from another thread.
    pub(crate) async fn forecast_elsewhere(
        services: Instance<ServiceProvider>,
        city: String,
    ) -> String {
        let provider = source(&services);
        on_another_thread(move || {
            let source = provider.required_service::<dyn WeatherSource>().unwrap();
            match futures::executor::block_on(source.forecast(city)) {
                Ok(forecast) => forecast,
                Err(error) => format!("{error:?}"),
            }
        })
        .await
    }

    /// Notes `lines` from another thread, in order.
    pub(crate) async fn note_elsewhere(services: Instance<ServiceProvider>, lines: Vec<String>) {
        let provider = source(&services);
        on_another_thread(move || {
            let source = provider.required_service::<dyn WeatherSource>().unwrap();
            for line in lines {
                source.note(line);
            }
        })
        .await;
    }

    /// Notes `line` on the engine thread, in this call: it runs before this call returns.
    pub(crate) fn note_here(services: Instance<ServiceProvider>, line: String) {
        let provider = source(&services);
        provider
            .required_service::<dyn WeatherSource>()
            .unwrap()
            .note(line);
        LOG.lock().unwrap().push("noted here".into());
    }

    /// `forecast(city)` called on the engine thread, in this task: no queue.
    pub(crate) async fn forecast_here(services: Instance<ServiceProvider>, city: String) -> String {
        let source = source(&services)
            .required_service::<dyn WeatherSource>()
            .unwrap();
        match source.forecast(city).await {
            Ok(forecast) => forecast,
            Err(error) => format!("{error:?}"),
        }
    }

    /// Starts `forecast("forever")` (which TS never answers) from another thread, keeping the
    /// call for after the engine stopped.
    pub(crate) fn start_forever(services: Instance<ServiceProvider>) {
        let provider = source(&services);
        std::thread::spawn(move || {
            let source = provider.required_service::<dyn WeatherSource>().unwrap();
            let mut call: BoxFuture<'static, Result<String, WeatherError>> =
                Box::pin(async move { source.forecast("forever".into()).await });
            // Polled once, so the call is made (queued for the engine) now.
            let waker = futures::task::noop_waker();
            assert!(
                call.as_mut()
                    .poll(&mut std::task::Context::from_waker(&waker))
                    .is_pending()
            );
            *IN_FLIGHT.lock().unwrap() = Some(call);
        })
        .join()
        .unwrap();
    }

    /// Several first calls on one fresh adapter, before TS made its object: commands and posted
    /// members, all through the one object, the posts in order.
    pub(crate) async fn first_calls(services: Instance<ServiceProvider>) -> String {
        let source = source(&services)
            .required_service::<dyn WeatherSource>()
            .unwrap();
        let first = source.forecast("Rab".into());
        let second = source.forecast("Pag".into());
        for line in ["a", "b", "c"] {
            source.note(line.into());
        }
        let (first, second) = futures::join!(first, second);
        format!("{} / {}", first.unwrap(), second.unwrap())
    }

    /// Posted members from another thread on one fresh adapter, before TS made its object.
    pub(crate) fn posts_elsewhere(services: Instance<ServiceProvider>) {
        let source = source(&services)
            .required_service::<dyn WeatherSource>()
            .unwrap();
        std::thread::spawn(move || {
            for line in ["x", "y", "z"] {
                source.note(line.into());
            }
        })
        .join()
        .unwrap();
    }

    /// How many jobs the surface's runner queued so far.
    pub(crate) fn queued(services: Instance<ServiceProvider>) -> String {
        source(&services)
            .required_service::<rayx_app_nsh::EngineRunner>()
            .unwrap()
            .queued()
            .to_string()
    }

    /// Starts `forecast(city)` in this synchronous call from JS, keeping the call: it runs as
    /// the call's op, not through the queue.
    pub(crate) fn start_in_call(services: Instance<ServiceProvider>, city: String) {
        let source = source(&services)
            .required_service::<dyn WeatherSource>()
            .unwrap();
        let mut call: BoxFuture<'static, Result<String, WeatherError>> =
            Box::pin(async move { source.forecast(city).await });
        let waker = futures::task::noop_waker();
        let _ = call
            .as_mut()
            .poll(&mut std::task::Context::from_waker(&waker));
        *IN_CALL.lock().unwrap() = Some(call);
    }

    /// What the call `start_in_call` started gives.
    pub(crate) async fn started_in_call() -> String {
        let call = IN_CALL.lock().unwrap().take();
        match call {
            Some(call) => match call.await {
                Ok(forecast) => forecast,
                Err(error) => format!("{error:?}"),
            },
            None => "not started".into(),
        }
    }

    /// Keeps the contract for after the engine stopped.
    pub(crate) fn keep(services: Instance<ServiceProvider>) {
        let provider = source(&services);
        *KEPT.lock().unwrap() = Some(provider.required_service::<dyn WeatherSource>().unwrap());
    }

    /// Logs `line` (from TS).
    pub(crate) fn log(line: String) {
        LOG.lock().unwrap().push(line);
    }

    /// What was logged, in order.
    pub(crate) fn logged() -> String {
        LOG.lock().unwrap().join(", ")
    }

    /// Logs `line` from a transient implementation.
    pub(crate) fn fresh_log(line: String) {
        FRESH.lock().unwrap().push(line);
    }

    /// What the transient implementations logged, in order.
    pub(crate) fn fresh_logged() -> String {
        FRESH.lock().unwrap().join(",")
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
  async forecast(city) {
    if (city === "nowhere") throw new Error(`no forecast for ${city}`);
    if (city === "forever") { Probe.log("forever asked"); return new Promise(() => {}); }
    return `sunny in ${city}`;
  }
  note(line) { Probe.log(`note ${line}`); }
}

const shown = signal("waiting");
setTimeout(async () => {
  const lines = [];
  try {
    const services = await new RayXAppBuilder().addHostFakes()
      .addSingletonAs(weatherSourceToken(), () => new OpenMeteo())
      .start();
    // A command from another thread: its result, and what TS threw.
    lines.push(await Probe.forecastElsewhere(services, "Split"));
    lines.push(await Probe.forecastElsewhere(services, "nowhere"));
    // Posted members from another thread, in order.
    await Probe.noteElsewhere(services, ["one", "two"]);
    for (let i = 0; i < 50 && !Probe.logged().includes("note two"); i++) {
      await new Promise((resolve) => setTimeout(resolve, 5));
    }
    // On the engine thread, inside this call: on the spot, not through the queue.
    Probe.noteHere(services, "three");
    lines.push(Probe.logged());
    // A command on the engine thread, from a task and from a synchronous call from JS: neither
    // goes through the queue (the calls and posts from another thread above did).
    const queued = Probe.queued(services);
    lines.push(await Probe.forecastHere(services, "Hvar"));
    Probe.startInCall(services, "Vis");
    lines.push(await Probe.startedInCall());
    lines.push(`queued ${queued} ${Probe.queued(services)}`);
    // A call TS never answers, still running when the engine stops.
    Probe.startForever(services);
    for (let i = 0; i < 50 && !Probe.logged().includes("forever asked"); i++) {
      await new Promise((resolve) => setTimeout(resolve, 5));
    }
    Probe.keep(services);
  } catch (error) {
    lines.push(`threw ${error}`);
  }
  shown.set(`done ${lines.join("|")}`);
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn a_ts_implementation_is_called_from_any_thread_until_the_engine_stops() {
    assert_eq!(
        headless::run(set(), TSX),
        "done sunny in Split\
         |Js(\"no forecast for nowhere\")\
         |note one, note two, note three, noted here\
         |sunny in Hvar\
         |sunny in Vis\
         |queued 4 4"
    );
    assert!(
        LOG.lock()
            .unwrap()
            .iter()
            .any(|line| line == "forever asked")
    );
    // `run` stopped the engine: a call now resolves with the engine gone, from any thread.
    let source = KEPT.lock().unwrap().take().expect("kept");
    let answer = std::thread::spawn({
        let source = Arc::clone(&source);
        move || futures::executor::block_on(source.forecast("Zadar".into()))
    })
    .join()
    .unwrap();
    assert!(
        matches!(
            answer,
            Err(WeatherError::Contract(ContractError::EngineGone {
                contract: "WeatherSource"
            }))
        ),
        "{answer:?}"
    );
    // The call in flight when the engine stopped resolves too, with an error (NSH's own, that
    // JS stopped, or the engine gone), instead of hanging.
    let in_flight = IN_FLIGHT.lock().unwrap().take().expect("started");
    let answer = futures::executor::block_on(in_flight);
    assert!(
        matches!(
            &answer,
            Err(WeatherError::Contract(ContractError::EngineGone { .. }))
        ) || matches!(&answer, Err(WeatherError::Js(message)) if message.contains("JS stopped")),
        "{answer:?}"
    );
    // A posted member is dropped (with a line saying so), not queued forever.
    source.note("gone".into());
    assert!(!LOG.lock().unwrap().iter().any(|line| line.contains("gone")));
    std::mem::forget(source);
}

const SCOPED: &str = r#"import { mount, signal, Text } from "nsh";
import { Probe, RayXAppBuilder, weatherSourceToken } from "test/app";

class Local {
  async forecast(city) { return `local sun in ${city}`; }
  note() {}
}

const shown = signal("waiting");
setTimeout(async () => {
  const lines = [];
  try {
    const services = await new RayXAppBuilder().addHostFakes()
      .addScopedAs(weatherSourceToken(), () => new Local())
      .start();
    const scope = services.createScope();
    // TS services are made in a call from JS: this one, in the scope.
    scope.getRequiredService(weatherSourceToken());
    // Resolved and called from another thread: the call reaches it through the queue.
    lines.push(await Probe.forecastElsewhere(scope, "Pula"));
  } catch (error) {
    lines.push(`threw ${error}`);
  }
  shown.set(`done ${lines.join("|")}`);
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn a_scoped_ts_implementation_resolved_on_another_thread_is_called_there() {
    assert_eq!(headless::run(set(), SCOPED), "done local sun in Pula");
}

const TRANSIENT: &str = r#"import { mount, signal, Text } from "nsh";
import { Probe, RayXAppBuilder, weatherSourceToken } from "test/app";

let made = 0;
class Fresh {
  constructor() { made += 1; this.n = made; }
  async forecast(city) { return `fresh ${this.n} in ${city}`; }
  note(line) { Probe.freshLog(`fresh note ${line}`); }
}

const shown = signal("waiting");
setTimeout(async () => {
  const lines = [];
  try {
    const services = await new RayXAppBuilder().addHostFakes()
      .addTransientAs(weatherSourceToken(), () => new Fresh())
      .start();
    // Each resolve is a new adapter; TS makes its object when it is first called, on the engine
    // thread from a task, and from another thread.
    lines.push(await Probe.forecastHere(services, "Krk"));
    lines.push(await Probe.forecastElsewhere(services, "Cres"));
    lines.push(String(made));
    // Several first calls on one adapter: TS makes one object, and the posts keep their order.
    lines.push(await Probe.firstCalls(services));
    for (let i = 0; i < 50 && !Probe.freshLogged().includes("fresh note c"); i++) {
      await new Promise((resolve) => setTimeout(resolve, 5));
    }
    lines.push(String(made));
    lines.push(Probe.freshLogged());
    // Posts from another thread while the object is made: all of them, in order.
    Probe.postsElsewhere(services);
    for (let i = 0; i < 50 && !Probe.freshLogged().includes("fresh note z"); i++) {
      await new Promise((resolve) => setTimeout(resolve, 5));
    }
    lines.push(`${made} ${Probe.freshLogged()}`);
  } catch (error) {
    lines.push(`threw ${error}`);
  }
  shown.set(`done ${lines.join("|")}`);
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn a_transient_ts_implementation_is_made_on_the_engine_when_first_called() {
    assert_eq!(
        headless::run(set(), TRANSIENT),
        "done fresh 1 in Krk|fresh 2 in Cres|2\
         |fresh 3 in Rab / fresh 3 in Pag|3\
         |fresh note a,fresh note b,fresh note c\
         |4 fresh note a,fresh note b,fresh note c,fresh note x,fresh note y,fresh note z"
    );
}
