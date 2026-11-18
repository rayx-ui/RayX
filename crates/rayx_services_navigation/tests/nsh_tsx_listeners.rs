//! Listeners of the `NavigationService` and `NavigationRegion` TS classes through NSH's engine,
//! headless: a provider gives every consumer the same object, so each `onChange` returns a
//! function that stops that listener alone; another consumer's keeps hearing.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use rayx_app_nsh::testing;

const TSX: &str = r#"import { mount, signal, Text } from "nsh";
import { NavigationService, RayXAppBuilder } from "test/app";

const shown = signal("waiting");
const tick = () => new Promise((resolve) => setTimeout(resolve, 10));
setTimeout(async () => {
  const lines = [];
  try {
    const services = await new RayXAppBuilder().addHostFakes()
      .addNavigation((options) => { options.addRoute("/").addRoute("/a").addRoute("/b"); })
      .start();

    // Two consumers, one object: each stops its own listener.
    const first = services.getRequiredService(NavigationService);
    const second = services.getRequiredService(NavigationService);
    lines.push(String(first === second));
    const routes = { first: [], second: [] };
    const stopFirst = first.onChange((event) => routes.first.push(event.to.path));
    second.onChange((event) => routes.second.push(event.to.path));
    await first.navigate("/a");
    await tick();
    lines.push(String(typeof stopFirst));
    stopFirst();
    stopFirst();
    await second.navigate("/b");
    await tick();
    lines.push(`routes ${routes.first} / ${routes.second}`);

    const panel = first.region("panel");
    const pages = { first: 0, second: 0 };
    const stopPanel = panel.onChange(() => pages.first++);
    panel.onChange(() => pages.second++);
    await panel.navigate("/a");
    await tick();
    stopPanel();
    await panel.navigate("/b");
    await tick();
    lines.push(`pages ${pages.first} / ${pages.second}`);
  } catch (error) {
    lines.push(`threw ${error}`);
  }
  shown.set(`done ${lines.join("|")}`);
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

fn set() -> nsh_core::element::Registry {
    testing::set(rayx_services_navigation::register)
}

#[test]
fn each_consumer_stops_its_own_listener_of_a_shared_service_object() {
    assert_eq!(
        testing::run(set(), TSX),
        "done true|function|routes /a / /a,/b|pages 1 / 2"
    );
}

#[test]
fn listener_members_are_declared_to_return_their_stop_function() {
    let ts = testing::declarations(&set());
    for declared in [
        "  onChange(listener: (event: unknown) => void): () => void;",
        "  onChange(listener: (pages: unknown) => void): () => void;",
    ] {
        assert!(ts.contains(declared), "no `{declared}` in\n{ts}");
    }
}
