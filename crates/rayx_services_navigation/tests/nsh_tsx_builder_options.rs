//! `addNavigation` in TS through NSH's engine, headless: its options callback adds routes as
//! Rust's `NavigationOptions` does and they reach RayX's navigation service, a callback that
//! throws makes `addNavigation` throw with its message, the options object is valid during its
//! callback only, and the options class and the method are declared with Rust's.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use nsh_core::JsonValue;
use nsh_core::element::Registry;
use nsh_core::prelude::*;
use rayx_app_nsh::testing;
use rayx_app_nsh::ts::ServiceProvider;
use rayx_services_navigation::NavigationOptions;

/// What the test reads of the app's options from Rust.
#[module]
struct Probe;

#[module]
impl Probe {
    /// The navigation options the service was made with, as JSON.
    pub(crate) fn routes(services: Instance<ServiceProvider>) -> Result<JsonValue, String> {
        let options = services
            .with(|services| services.services().options::<NavigationOptions>())
            .map_err(|error| error.to_string())?;
        serde_json::to_value(&options.value().routes).map_err(|error| error.to_string())
    }
}

fn set() -> Registry {
    testing::set(|set| {
        rayx_services_navigation::register(set);
        set.module::<Probe>();
    })
}

const TSX: &str = r#"import { mount, signal, Text } from "nsh";
import { NavigationService, Probe, RayXAppBuilder } from "test/app";

const shown = signal("waiting");
setTimeout(async () => {
  const lines = [];
  try {
    const services = await new RayXAppBuilder().addHostFakes()
      .addNavigation((options) => {
        options.addRoute("/").addRoute("/projects/{id}", { component: "Project", prewarm: true });
      })
      .start();
    const routes = Probe.routes(services);
    lines.push(routes.map((route) => `${route.pattern}:${route.component}:${route.prewarm}`).join(","));
    lines.push(services.getRequiredService(NavigationService).navigate("/projects/12").status);
  } catch (error) {
    lines.push(`threw ${error}`);
  }
  shown.set(`done ${lines.join("|")}`);
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn routes_added_in_ts_reach_the_navigation_service() {
    assert_eq!(
        testing::run(set(), TSX),
        "done /:null:false,/projects/{id}:Project:true|Success"
    );
}

const THROWING: &str = r#"import { mount, signal, Text } from "nsh";
import { RayXAppBuilder } from "test/app";

const shown = signal("waiting");
setTimeout(async () => {
  const lines = [];
  const thrown = (f) => { try { f(); return "no throw"; } catch (e) { return e.message; } };
  const builder = new RayXAppBuilder().addHostFakes();
  lines.push(thrown(() => builder.addNavigation(() => { throw new Error("no routes"); })));
  // An options object is valid during its callback only.
  let kept;
  builder.addNavigation((options) => { kept = options; });
  lines.push(thrown(() => kept.addRoute("/late")).includes("disposed") ? "disposed" : "kept");
  shown.set(`done ${lines.join("|")}`);
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn a_throwing_callback_makes_add_navigation_throw_with_its_message() {
    assert_eq!(testing::run(set(), THROWING), "done no routes|disposed");
}

#[test]
fn the_navigation_options_class_is_declared_with_rust_s_methods() {
    let ts = testing::declarations(&set());
    for declared in [
        "export declare class NavigationOptions {",
        "  addRoute(pattern: string, options?: RouteOptions | null): NavigationOptions;",
        "addNavigation(configure?: ((options: NavigationOptions) => void) | null): RayXAppBuilder;",
    ] {
        assert!(ts.contains(declared), "no `{declared}` in\n{ts}");
    }
    // `addNavigation` carries its doc comment.
    let add = ts
        .find("addNavigation(configure?: ((options: NavigationOptions) => void) | null)")
        .unwrap();
    let before = ts[..add].trim_end();
    assert!(
        before.ends_with("*/"),
        "no doc comment on addNavigation in\n{ts}"
    );
    let doc = &before[before.rfind("/**").unwrap()..];
    assert!(doc.contains("navigation service"), "{doc}");
}
