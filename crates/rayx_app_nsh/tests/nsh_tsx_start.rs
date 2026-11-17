//! `await builder.start()` through NSH's engine, headless: it resolves with the root
//! `ServiceProvider` once every startup service initialized, rejects with the startup error's
//! message (a failing service, a timeout, an option validated on start, a ready hook), resolves a
//! second start on the surface with the same root, and each surface builds its own provider.
//! (The platform and suspension crates' groups cover their services kept current from NSH.)

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod headless;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use futures::FutureExt as _;
use futures::future::{LocalBoxFuture, pending};
use nsh_core::prelude::*;
use rayx_app_nsh::di::{
    AsyncInitializable, StartupInitializable, StartupInitializationContext,
    StartupInitializationResult, StartupInitializationState,
};
use rayx_app_nsh::ts::{RayXAppBuilder, ServiceProvider};
use rayx_app_nsh::{AppService, ReadyAppBuilderExt as _, ReadyError, ServiceAppBuilderExt as _};

static NEXT: AtomicUsize = AtomicUsize::new(1);

/// A singleton: one per surface.
#[derive(AppService)]
struct Session {
    id: usize,
}

/// How the catalog's startup goes.
#[derive(Clone, Copy, PartialEq)]
enum Startup {
    /// It initializes after 5 ms.
    Ok,
    /// It fails.
    Fail,
    /// It never finishes: its own 50 ms timeout fails startup.
    Hang,
}

/// A startup service.
#[derive(Clone, AppService)]
#[app_service(startup)]
struct Catalog {
    startup: Startup,
    initialized: StartupInitializationState,
}

impl AsyncInitializable for Catalog {
    fn is_initialized(&self) -> bool {
        self.initialized.is_initialized()
    }

    fn initialize(
        &self,
        _context: StartupInitializationContext,
    ) -> LocalBoxFuture<'static, StartupInitializationResult> {
        let initialized = self.initialized.clone();
        match self.startup {
            Startup::Fail => async { Err("catalog unavailable".into()) }.boxed_local(),
            Startup::Hang => pending().boxed_local(),
            Startup::Ok => async move {
                timer(Duration::from_millis(5)).await;
                initialized.mark_initialized();
                Ok(())
            }
            .boxed_local(),
        }
    }

    fn initialization_timeout(&self) -> Option<Duration> {
        Some(Duration::from_millis(50))
    }
}

impl StartupInitializable for Catalog {}

/// Options validated on start.
#[derive(Debug, Default)]
struct PagesOptions {
    title: String,
}

/// The test's services, added to a TS builder.
#[module]
struct Probe;

#[module]
impl Probe {
    /// Adds the session, and the catalog starting as `startup` says (`"ok"`, `"fail"`,
    /// `"hang"`), invalid options (`"invalid"`) or a failing ready hook (`"hook"`).
    pub(crate) fn add_catalog(
        builder: Instance<RayXAppBuilder>,
        startup: String,
    ) -> Result<Instance<RayXAppBuilder>, String> {
        builder
            .with_mut(|builder| {
                builder.extend(|builder| {
                    let builder = builder.add_singleton(Session {
                        id: NEXT.fetch_add(1, Ordering::SeqCst),
                    });
                    let catalog = |startup| Catalog {
                        startup,
                        initialized: StartupInitializationState::default(),
                    };
                    match startup.as_str() {
                        "fail" => builder.add_startup_singleton(catalog(Startup::Fail)),
                        "hang" => builder.add_startup_singleton(catalog(Startup::Hang)),
                        "invalid" => builder.add_options::<PagesOptions>(|options| {
                            options
                                .validate(
                                    |options| !options.title.is_empty(),
                                    "the pages need a title",
                                )
                                .validate_on_start();
                        }),
                        "hook" => builder.on_ready(|_| async {
                            Err(ReadyError::new("the catalog is unreachable"))
                        }),
                        _ => builder.add_startup_singleton(catalog(Startup::Ok)),
                    }
                })
            })
            .map_err(|error| error.to_string())?;
        Ok(builder)
    }

    /// The surface's session id.
    pub(crate) fn session(services: Instance<ServiceProvider>) -> Result<u32, String> {
        services
            .with(|services| services.services().required_service::<Session>())
            .map(|session| session.id as u32)
            .map_err(|error| error.to_string())
    }

    /// Whether the catalog initialized.
    pub(crate) fn initialized(services: Instance<ServiceProvider>) -> Result<bool, String> {
        services
            .with(|services| services.services().required_service::<Catalog>())
            .map(|catalog| catalog.is_initialized())
            .map_err(|error| error.to_string())
    }
}

fn set() -> nsh_core::element::Registry {
    headless::set(|set| {
        set.module::<Probe>();
    })
}

const STARTS: &str = r#"import { mount, signal, Text } from "nsh";
import { Probe, RayXAppBuilder } from "test/app";

const shown = signal("waiting");
setTimeout(async () => {
  const lines = [];
  const thrown = (f) => { try { f(); return "no throw"; } catch (e) { return e.message; } };
  try {
    const builder = Probe.addCatalog(new RayXAppBuilder().addHostFakes(), "ok");
    // Two starts at once, of two builders: one startup, one root.
    const [services, also] = await Promise.all([
      builder.start(),
      Probe.addCatalog(new RayXAppBuilder().addHostFakes(), "ok").start(),
    ]);
    lines.push(`${Probe.initialized(services)} ${services === also}`);
    // Started: the same builder, or another, resolves with the same root.
    lines.push(String((await builder.start()) === services));
    lines.push(String((await new RayXAppBuilder().addHostFakes().start()) === services));
    lines.push(String(Probe.session(services) === Probe.session(also)));
    // The builder's other methods throw after `start()`.
    lines.push(thrown(() => builder.addOperations()));
  } catch (error) {
    lines.push(`threw ${error}`);
  }
  shown.set(`done ${lines.join("|")}`);
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn start_resolves_with_the_root_once_and_for_every_later_start() {
    assert_eq!(
        headless::run(set(), STARTS),
        "done true true|true|true|true|the RayXAppBuilder has started already: make a new one"
    );
}

const SAME_TURN: &str = r#"import { mount, signal, Text } from "nsh";
import { I18nService, RayXAppBuilder } from "test/app";

const shown = signal("waiting");
setTimeout(async () => {
  const lines = [];
  const thrown = (f) => { try { f(); return "no throw"; } catch (e) { return e.message; } };
  try {
    const builder = new RayXAppBuilder().addHostFakes();
    const ready = builder.start();
    // The same turn: the builder was taken by the call, so nothing more reaches the startup.
    lines.push(thrown(() => builder.addI18n((options) => { options.defaultLanguage("hr"); })));
    const services = await ready;
    lines.push(String(services.getService(I18nService)));
  } catch (error) {
    lines.push(`threw ${error}`);
  }
  shown.set(`done ${lines.join("|")}`);
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn start_takes_the_builder_in_the_call_so_the_same_turn_s_calls_throw() {
    assert_eq!(
        headless::run(set(), SAME_TURN),
        "done the RayXAppBuilder has started already: make a new one|null"
    );
}

const FAILS: &str = r#"import { mount, signal, Text } from "nsh";
import { Probe, RayXAppBuilder } from "test/app";

const shown = signal("waiting");
Probe.addCatalog(new RayXAppBuilder().addHostFakes(), "@STARTUP@").start().then(
  () => shown.set("done started"),
  (error) => shown.set(`done rejected ${error.message}`),
);
mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn start_rejects_with_the_startup_error_s_message() {
    for (startup, expected) in [
        ("fail", "catalog unavailable"),
        ("hang", "did not initialize within 50 ms"),
        ("invalid", "the pages need a title"),
        ("hook", "the catalog is unreachable"),
    ] {
        let shown = headless::run(set(), &FAILS.replace("@STARTUP@", startup));
        assert!(shown.starts_with("done rejected "), "{startup}: {shown}");
        assert!(shown.contains(expected), "{startup}: {shown}");
    }
    let shown = headless::run(set(), &FAILS.replace("@STARTUP@", "ok"));
    assert_eq!(shown, "done started");
}

const MISSING_SETTINGS: &str = r#"import { mount, signal, Text } from "nsh";
import { RayXAppBuilder } from "test/app";

const shown = signal("waiting");
new RayXAppBuilder().addHostFakes().addAppSettings("app").start().then(
  () => shown.set("done started"),
  (error) => shown.set(`done rejected ${error.message}`),
);
mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn start_rejects_when_the_configuration_does_not_load() {
    let shown = headless::run(set(), MISSING_SETTINGS);
    assert!(shown.starts_with("done rejected "), "{shown}");
    assert!(shown.contains("appsettings.json"), "{shown}");
}

const SESSION: &str = r#"import { mount, signal, Text } from "nsh";
import { Probe, RayXAppBuilder } from "test/app";

const shown = signal("waiting");
Probe.addCatalog(new RayXAppBuilder().addHostFakes(), "ok").start().then(
  (services) => shown.set(`done ${Probe.session(services)}`),
  (error) => shown.set(`done rejected ${error.message}`),
);
mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn each_surface_builds_its_own_provider() {
    let first = headless::start(set(), SESSION, None);
    let second = headless::start(set(), SESSION, None);
    let (first, second) = (first.shown("done"), second.shown("done"));
    assert!(
        first.starts_with("done ") && !first.contains("rejected"),
        "{first}"
    );
    assert!(
        second.starts_with("done ") && !second.contains("rejected"),
        "{second}"
    );
    assert_ne!(first, second, "each surface has its own session");
}
