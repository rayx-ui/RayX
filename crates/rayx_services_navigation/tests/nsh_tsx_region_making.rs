//! A `NavigationRegion` over a navigation service written in TS, headless: the service makes the
//! region later, so every command waits for the one call making it (two commands issued at once
//! included), and a command after a failed call makes the region again.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use rayx_app_nsh::testing;

/// A TS navigation service whose `ensureRegion` answers after `setTimeout`, failing while
/// `broken`; `asked` records the calls.
const SERVICE: &str = r#"import { mount, signal, Text } from "nsh";
import { ContractEvents, ContractState, NavigationService, RayXAppBuilder } from "test/app";

const route = (path) => ({
  uri: path, base_uri: "app://", path, query: "", fragment: "", location: path,
  segments: path.split("/"), params: {}, query_params: {}, passed_parameters: null,
  component: null, pattern: path, prewarm: false,
});
const result = (region, path, direction) =>
  ({ region, direction, status: "Success", route: route(path) });

class MyNavigation {
  navigation = new ContractState({ root_region: "main", regions: {}, location: "/" });
  events = new ContractEvents();
  rootRegionName = "main";
  urlRegion = "main";
  defaultHistoryMode = "Push";
  asked = [];
  broken = false;
  async navigate(request) { return result(request.region, request.args.uri, "New"); }
  async back(region) { this.asked.push(`back ${region}`); return result(region, "/before", "Back"); }
  async forward(region) { this.asked.push(`forward ${region}`); return result(region, "/after", "Forward"); }
  async refresh(region) { return result(region, "/same", "Refresh"); }
  async goToEntry(region, index) { return result(region, "/first", "Back"); }
  async ensureRegion(region) {
    this.asked.push(`ensure ${region}`);
    await new Promise((resolve) => setTimeout(resolve, 10));
    if (this.broken) throw new Error("no region yet");
    this.asked.push(`made ${region}`);
  }
  async closeRegion(region) { return true; }
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
    const navigation = services.getRequiredService(NavigationService);
    await run(navigation, mine, lines);
  } catch (error) {
    lines.push(`threw ${error}`);
  }
  shown.set(`done ${lines.join("|")}`);
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

/// `SERVICE` with `body`, the async function `run(navigation, mine, lines)`.
fn script(body: &str) -> String {
    format!("{SERVICE}\nasync function run(navigation, mine, lines) {{\n{body}\n}}\n")
}

fn run(body: &str) -> String {
    testing::run(
        testing::set(rayx_services_navigation::register),
        &script(body),
    )
}

#[test]
fn every_command_issued_at_once_waits_for_the_region_to_be_made() {
    let shown = run(r#"
    const panel = navigation.region("panel");
    const [back, forward] = await Promise.all([panel.back(), panel.forward()]);
    lines.push(back.route.path, forward.route.path);
    await panel.back();
    lines.push(mine.asked.join(","));
"#);
    assert_eq!(
        shown,
        "done /before|/after|ensure panel,made panel,back panel,forward panel,back panel"
    );
}

#[test]
fn a_command_after_a_failed_make_makes_the_region_again() {
    let shown = run(r#"
    mine.broken = true;
    const panel = navigation.region("panel");
    try {
      await panel.back();
      lines.push("resolved");
    } catch (error) {
      lines.push(`rejected ${String(error).includes("no region yet")}`);
    }
    mine.broken = false;
    lines.push((await panel.forward()).route.path);
    lines.push((await panel.back()).route.path);
    lines.push(mine.asked.join(","));
"#);
    assert_eq!(
        shown,
        "done rejected true|/after|/before|ensure panel,ensure panel,made panel,forward panel,\
         back panel"
    );
}
