//! A contract whose TS token is a RayX class, headless: `#[contract(ts = "…")]` names its TS
//! interface; TS registers an implementation under the class and Rust resolves it as
//! `dyn Trait`, while TS resolves the class over it (reading published state synchronously);
//! a Rust implementation is reached through the same class; and a contract without a class keeps
//! its token function.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod headless;

use std::sync::Arc;

use futures::future::BoxFuture;
use nsh_core::prelude::*;
use rayx_app_nsh::ts::{
    ContractError, ContractRegistryExt as _, Exception, RayXAppBuilder, ServiceClass,
    ServiceProvider,
};
use rayx_app_nsh::{AppBuilder as _, StateCell};

/// Why loading failed.
#[derive(Debug)]
pub enum ThemeError {
    /// The call did not reach TS.
    Contract(ContractError),
    /// What TS threw.
    Js(String),
}

impl From<ContractError> for ThemeError {
    fn from(error: ContractError) -> Self {
        Self::Contract(error)
    }
}

impl From<Exception> for ThemeError {
    fn from(exception: Exception) -> Self {
        Self::Js(exception.message)
    }
}

/// The app's theme, in Rust or in TS.
#[rayx_app_nsh::contract(ts = "ThemeContract")]
pub trait Theme {
    /// Loads the theme `name`.
    async fn load(&self, name: String) -> Result<String, ThemeError>;
    /// The theme's name now.
    #[state]
    fn current(&self) -> &StateCell<String>;
}

/// The theme for TSX: `services.getRequiredService(ThemeService)`, over whichever theme is
/// registered.
#[class]
pub struct ThemeService {
    theme: Arc<dyn Theme>,
}

impl ServiceClass for ThemeService {
    type Service = dyn Theme;

    fn wrap(theme: Arc<dyn Theme>) -> Self {
        Self { theme }
    }
}

#[class]
impl ThemeService {
    /// The theme's name now, read synchronously.
    #[getter]
    pub fn current(&self) -> String {
        self.theme.current().get().to_string()
    }

    /// The contract this class is the token of (TS types only: `null` at run time).
    #[getter]
    #[ts("(): ThemeContract | null")]
    pub fn contract(&self) -> Option<bool> {
        None
    }
}

/// A theme written in Rust.
struct RustTheme {
    current: StateCell<String>,
}

impl Theme for RustTheme {
    fn load(&self, name: String) -> BoxFuture<'_, Result<String, ThemeError>> {
        Box::pin(async move { Ok(format!("rust loaded {name}")) })
    }

    fn current(&self) -> &StateCell<String> {
        &self.current
    }
}

/// A clock, written in TS: a contract without a class.
#[rayx_app_nsh::contract]
pub trait Clock {
    /// Ticks.
    fn tick(&self);
}

/// The test's Rust side.
#[module]
struct Probe;

#[module]
impl Probe {
    /// Registers the Rust theme.
    pub(crate) fn add_rust_theme(
        builder: Instance<RayXAppBuilder>,
    ) -> Result<Instance<RayXAppBuilder>, String> {
        builder
            .with_mut(|builder| {
                builder.extend(|mut builder| {
                    builder
                        .services_mut()
                        .add_singleton_as::<dyn Theme>(Arc::new(RustTheme {
                            current: StateCell::new("rust".to_owned()),
                        })
                            as Arc<dyn Theme>);
                    builder
                })
            })
            .map_err(|error| error.to_string())?;
        Ok(builder)
    }

    /// What Rust gets from the registered theme: its name and what `load(name)` gives.
    pub(crate) async fn from_rust(services: Instance<ServiceProvider>, name: String) -> String {
        let theme = services
            .with(|services| services.services().required_service::<dyn Theme>())
            .unwrap();
        let current = theme.current().get().to_string();
        match theme.load(name).await {
            Ok(loaded) => format!("{current} {loaded}"),
            Err(error) => format!("{error:?}"),
        }
    }
}

fn set() -> nsh_core::element::Registry {
    headless::set(|set| {
        set.module::<Probe>()
            .contract_class::<dyn Theme, ThemeService>()
            .contract::<dyn Clock>();
    })
}

const TS_THEME: &str = r#"import { mount, signal, Text } from "nsh";
import { ContractState, Probe, RayXAppBuilder, ThemeService } from "test/app";

class MyTheme {
  current = new ContractState("light");
  async load(name) { return `ts loaded ${name}`; }
}

const shown = signal("waiting");
setTimeout(async () => {
  const lines = [];
  try {
    const services = await new RayXAppBuilder()
      .addSingletonAs(ThemeService, () => new MyTheme())
      .start();
    // Rust reaches the TS implementation as `dyn Theme`.
    lines.push(await Probe.fromRust(services, "solar"));
    // TS resolves the class over it, the same object each time, and reads its state at once.
    const theme = services.getRequiredService(ThemeService);
    lines.push(`${theme instanceof ThemeService} ${theme === services.getRequiredService(ThemeService)}`);
    lines.push(theme.current);
    lines.push(String(theme.contract));
  } catch (error) {
    lines.push(`threw ${error}`);
  }
  shown.set(`done ${lines.join("|")}`);
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn a_ts_implementation_under_the_class_is_rust_s_dyn_trait_and_ts_s_class() {
    assert_eq!(
        headless::run(set(), TS_THEME),
        "done light ts loaded solar|true true|light|null"
    );
}

const PUBLISHING: &str = r#"import { mount, signal, Text } from "nsh";
import { ContractState, RayXAppBuilder, ThemeService } from "test/app";

let made;
class MyTheme {
  current = new ContractState("light");
  constructor() { made = this; }
  isInitialized() { return true; }
  async initialize() {}
  async load(name) { return name; }
}

const shown = signal("waiting");
setTimeout(async () => {
  try {
    // A startup form under the class, too.
    const services = await new RayXAppBuilder()
      .addStartupSingletonAs(ThemeService, () => new MyTheme())
      .start();
    const theme = services.getRequiredService(ThemeService);
    const before = theme.current;
    made.current.publish("dark");
    shown.set(`done ${before} ${theme.current}`);
  } catch (error) {
    shown.set(`done threw ${error}`);
  }
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn the_class_reads_what_the_ts_implementation_publishes_synchronously() {
    assert_eq!(headless::run(set(), PUBLISHING), "done light dark");
}

const RUST_THEME: &str = r#"import { mount, signal, Text } from "nsh";
import { Probe, RayXAppBuilder, ThemeService } from "test/app";

const shown = signal("waiting");
setTimeout(async () => {
  const lines = [];
  try {
    const services = await Probe.addRustTheme(new RayXAppBuilder()).start();
    lines.push(await Probe.fromRust(services, "lunar"));
    lines.push(services.getRequiredService(ThemeService).current);
  } catch (error) {
    lines.push(`threw ${error}`);
  }
  shown.set(`done ${lines.join("|")}`);
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn a_rust_implementation_is_reached_through_the_same_class() {
    assert_eq!(
        headless::run(set(), RUST_THEME),
        "done rust rust loaded lunar|rust"
    );
}

#[test]
fn the_set_declares_the_named_interface_the_class_and_the_typed_registrations() {
    let set = set();
    let ts = headless::declarations(&set);
    for declared in [
        "/** The app's theme, in Rust or in TS. */\nexport interface ThemeContract {",
        "  readonly current: ContractState<string>;",
        "export declare class ThemeService {",
        "  get contract(): ThemeContract | null;",
        "export type ContractClass<T> = { readonly prototype: { readonly contract: T | null } };",
        "addSingletonAs<T>(token: ContractClass<T>, factory: (services: ServiceProvider) => T): this;",
        // A contract without a class keeps its token function.
        "export declare function clockToken(): ServiceToken<Clock>;",
    ] {
        assert!(ts.contains(declared), "no `{declared}` in\n{ts}");
    }
    // The class is the theme's token: it has no token function.
    assert!(!ts.contains("themeToken"), "{ts}");
    assert!(!ts.contains("themeContractToken"), "{ts}");
    assert_eq!(set.undocumented(), Vec::<String>::new());
}

const FORMS: &str = r#"import { mount, signal, Text } from "nsh";
import { ContractState, Probe, RayXAppBuilder, ThemeService } from "test/app";

class MyTheme {
  current = new ContractState("form");
  async load(name) { return `form loaded ${name}`; }
}

const shown = signal("waiting");
setTimeout(async () => {
  const lines = [];
  try {
    const services = await new RayXAppBuilder()
      .@ADD@(ThemeService, () => new MyTheme())
      .start();
    const scope = services.createScope();
    lines.push(await Probe.fromRust(scope, "x"));
    lines.push(scope.getRequiredService(ThemeService).current);
  } catch (error) {
    lines.push(`threw ${error}`);
  }
  shown.set(`done ${lines.join("|")}`);
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn every_lifetime_registers_an_implementation_under_the_class() {
    for form in ["addSingletonAs", "addScopedAs", "addTransientAs"] {
        assert_eq!(
            headless::run(set(), &FORMS.replace("@ADD@", form)),
            "done form form loaded x|form",
            "{form}"
        );
    }
}
