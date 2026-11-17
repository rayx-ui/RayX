//! A TSX app's start with RayX's suspension service, headless (no Rust root): the service
//! follows NSH's app state from its startup on, in a TSX app too.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use rayx_app_nsh::testing;

const TSX: &str = r#"import { mount, signal, Text } from "nsh";
import { RayXAppBuilder, SuspensionService } from "test/app";

const shown = signal("waiting");
setTimeout(async () => {
  try {
    const services = await new RayXAppBuilder().addHostFakes().addSuspension().start();
    const suspension = services.getRequiredService(SuspensionService);
    // The service's following runs on later turns.
    await new Promise((resolve) => setTimeout(resolve, 20));
    const lifecycle = suspension.lifecycle();
    const synced = lifecycle.last_reason === null ? "not synced" : "synced";
    shown.set(`ready ${synced} ${lifecycle.lifecycle}`);
  } catch (error) {
    shown.set(`ready threw ${error}`);
  }
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn start_follows_nsh_s_app_state_in_a_tsx_app() {
    let set = testing::set(rayx_services_suspension::register);
    let app = testing::start(set, TSX, None);
    assert_eq!(app.shown("ready"), "ready synced Running");
}
