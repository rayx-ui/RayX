//! The navigation classes' commands are promises, headless: over a navigation service written in
//! TS (which answers later) `NavigationService` and `NavigationRegion` resolve with its answers
//! and reject with what it throws; the reads stay synchronous.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use rayx_app_nsh::testing;

const TSX: &str = r#"import { mount, signal, Text } from "nsh";
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
  async navigate(request) {
    await new Promise((resolve) => setTimeout(resolve, 0));
    if (request.args.uri === "/nowhere") throw new Error("no such route");
    this.asked.push(`navigate ${request.region} ${request.args.uri} ${request.args.history_mode}`);
    return result(request.region, request.args.uri, "New");
  }
  async back(region) { this.asked.push(`back ${region}`); return result(region, "/before", "Back"); }
  async forward(region) { this.asked.push(`forward ${region}`); return result(region, "/after", "Forward"); }
  async refresh(region) { this.asked.push(`refresh ${region}`); return result(region, "/same", "Refresh"); }
  async goToEntry(region, index) { this.asked.push(`goTo ${region} ${index}`); return result(region, "/first", "Back"); }
  async ensureRegion(region) {
    if (region === "broken") throw new Error("no such region");
    this.asked.push(`ensure ${region}`);
  }
  async closeRegion(region) { this.asked.push(`close ${region}`); return true; }
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
    const navigated = navigation.navigate("/projects/1");
    lines.push(navigated instanceof Promise ? "a promise" : "no promise");
    lines.push((await navigated).route.path);
    lines.push((await navigation.back()).direction);
    lines.push((await navigation.forward()).route.path);
    lines.push((await navigation.refresh()).route.path);
    try {
      await navigation.navigate("/nowhere");
      lines.push("resolved");
    } catch (error) {
      lines.push(`rejected ${String(error).includes("no such route")}`);
    }
    lines.push(navigation.current() === null ? "current null" : "current a route");

    const panel = navigation.region("panel");
    lines.push((await panel.navigate("/a", true)).route.path);
    lines.push((await panel.back()).route.path);
    lines.push((await panel.forward()).route.path);
    lines.push((await panel.refresh()).route.path);
    lines.push((await panel.goTo(0)).route.path);
    lines.push(String(await panel.close()));
    lines.push(String(panel.pages().length));
    try {
      await navigation.region("broken").back();
      lines.push("resolved");
    } catch (error) {
      lines.push(`rejected ${String(error).includes("no such region")}`);
    }
    lines.push(mine.asked.join(","));
  } catch (error) {
    lines.push(`threw ${error}`);
  }
  shown.set(`done ${lines.join("|")}`);
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn the_navigation_classes_commands_are_promises_over_a_ts_implementation() {
    let set = testing::set(rayx_services_navigation::register);
    let shown = testing::run(set, TSX);
    let parts: Vec<&str> = shown.split('|').collect();
    assert_eq!(
        parts[..15],
        [
            "done a promise",
            "/projects/1",
            "Back",
            "/after",
            "/same",
            "rejected true",
            "current null",
            "/a",
            "/before",
            "/after",
            "/same",
            "/first",
            "true",
            "0",
            "rejected true",
        ],
        "{shown}"
    );
    assert_eq!(
        parts[15],
        "navigate main /projects/1 Push,back main,forward main,refresh main,ensure panel,\
         navigate panel /a Replace,back panel,forward panel,refresh panel,goTo panel 0,\
         close panel",
        "{shown}"
    );
    assert_eq!(parts.len(), 16, "{shown}");
}
