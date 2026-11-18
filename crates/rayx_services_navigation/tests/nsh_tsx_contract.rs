//! A navigation service written in TS, through NSH's engine, headless: registered under the
//! `NavigationService` class (the contract's token), Rust resolves it as `dyn NavigationService`
//! and reads its constants and state, hears its events and awaits its commands.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::{Arc, Mutex};

use nsh_core::prelude::*;
use rayx_app::services::navigation::NavigationService;
use rayx_app_nsh::testing;
use rayx_app_nsh::ts::ServiceProvider;

/// What the subscriber heard.
static HEARD: Mutex<Vec<String>> = Mutex::new(Vec::new());

fn navigation(services: &Instance<ServiceProvider>) -> Arc<dyn NavigationService> {
    services
        .with(|services| {
            services
                .services()
                .required_service::<dyn NavigationService>()
        })
        .unwrap()
}

/// What the test reads of the TS implementation from Rust.
#[module]
struct Probe;

#[module]
impl Probe {
    /// Its constants and its state's location.
    pub(crate) fn read(services: Instance<ServiceProvider>) -> String {
        let navigation = navigation(&services);
        format!(
            "{} {} {:?} {}",
            navigation.root_region_name(),
            navigation.url_region(),
            navigation.default_history_mode(),
            navigation.location()
        )
    }

    /// Hears its route changes.
    pub(crate) fn follow(services: Instance<ServiceProvider>) {
        navigation(&services)
            .on_route_changed(|changed| {
                HEARD
                    .lock()
                    .unwrap()
                    .push(format!("{} {}", changed.region, changed.to.path));
            })
            .detach();
    }

    /// What it heard.
    pub(crate) fn heard() -> String {
        HEARD.lock().unwrap().join(",")
    }

    /// Navigates the root region to `uri` through the contract: the route's path, or the error.
    pub(crate) async fn navigate(services: Instance<ServiceProvider>, uri: String) -> String {
        let navigation = navigation(&services);
        match navigation.navigate_to(uri).await {
            Ok(result) => format!("{:?} {}", result.status, result.route.path),
            Err(error) => format!("error {error}"),
        }
    }
}

const TSX: &str = r#"import { mount, signal, Text } from "nsh";
import {
  ContractEvents, ContractState, NavigationService, Probe, RayXAppBuilder,
} from "test/app";

const route = (path) => ({
  uri: path, base_uri: "app://", path, query: "", fragment: "", location: path,
  segments: path.split("/"), params: {}, query_params: {}, passed_parameters: null,
  component: null, pattern: path, prewarm: false,
});

class MyNavigation {
  navigation = new ContractState({ root_region: "main", regions: {}, location: "/start" });
  events = new ContractEvents();
  rootRegionName = "main";
  urlRegion = "main";
  defaultHistoryMode = "Replace";
  asked = [];
  async navigate(request) {
    if (request.args.uri === "/nowhere") throw new Error("no such route");
    this.asked.push(`${request.region} ${request.args.uri} ${request.args.history_mode}`);
    this.events.publish({ kind: "RouteChanged", region: request.region, from: null, to: route(request.args.uri) });
    return { region: request.region, direction: "New", status: "Success", route: route(request.args.uri) };
  }
  async back(region) { throw new Error("no history"); }
  async forward(region) { throw new Error("no history"); }
  async refresh(region) { throw new Error("no history"); }
  async goToEntry(region, index) { throw new Error("no history"); }
  async ensureRegion(region) {}
  async closeRegion(region) { return false; }
  async removeRegion(region) { return false; }
  async applyLocation(location) { return []; }
}

const shown = signal("waiting");
setTimeout(async () => {
  const lines = [];
  let mine;
  try {
    const services = await new RayXAppBuilder().addHostFakes()
      .addSingletonAs(NavigationService, () => (mine = new MyNavigation()))
      .start();
    lines.push(Probe.read(services));
    Probe.follow(services);
    lines.push(await Probe.navigate(services, "/projects/1"));
    lines.push(await Probe.navigate(services, "/nowhere"));
    // The class over it: RayX's class, the implementation behind.
    lines.push(services.getRequiredService(NavigationService).current() === null ? "null" : "a route");
    lines.push(mine.asked.join(","));
    mine.navigation.publish({ root_region: "main", regions: {}, location: "/projects/1" });
    lines.push(Probe.read(services));
    lines.push(Probe.heard());
  } catch (error) {
    lines.push(`threw ${error}`);
  }
  shown.set(`done ${lines.join("|")}`);
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn rust_uses_a_navigation_service_written_in_ts() {
    let set = testing::set(|set| {
        rayx_services_navigation::register(set);
        set.module::<Probe>();
    });
    let shown = testing::run(set, TSX);
    let parts: Vec<&str> = shown.split('|').collect();
    assert_eq!(parts[0], "done main main Replace /start", "{shown}");
    assert_eq!(parts[1], "Success /projects/1", "{shown}");
    assert!(parts[2].starts_with("error "), "{shown}");
    assert!(parts[2].contains("no such route"), "{shown}");
    assert_eq!(parts[3], "null", "{shown}");
    assert_eq!(parts[4], "main /projects/1 Replace", "{shown}");
    assert_eq!(parts[5], "main main Replace /projects/1", "{shown}");
    assert_eq!(parts[6], "main /projects/1", "{shown}");
    assert_eq!(parts.len(), 7, "{shown}");
}
