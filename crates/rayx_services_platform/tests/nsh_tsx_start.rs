//! The platform service in a TSX app, headless (no Rust root): `await builder.start()` with
//! `addPlatform()` gives the `PlatformInfoService` class NSH's environment once started, and the
//! service keeps itself current from NSH afterwards (a resize reaches an `onChange` listener).

#![allow(clippy::expect_used, clippy::unwrap_used)]

use nsh_core::geometry::Size;
use nsh_core::prelude::*;
use rayx_app_nsh::testing;

const PLATFORM: &str = r#"import { mount, signal, Text } from "nsh";
import { PlatformInfoService, RayXAppBuilder } from "test/app";

const shown = signal("waiting");
setTimeout(async () => {
  try {
    const services = await new RayXAppBuilder().addHostFakes().addPlatform().start();
    const platform = services.getRequiredService(PlatformInfoService);
    platform.onChange((snapshot) => {
      if (snapshot.viewport.width === 800) {
        shown.set(`resized ${snapshot.viewport.width}x${snapshot.viewport.height}`);
      }
    });
    // Listeners run on later turns.
    await new Promise((resolve) => setTimeout(resolve, 20));
    shown.set(`ready ${platform.current().viewport.width}`);
  } catch (error) {
    shown.set(`ready threw ${error}`);
  }
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

fn set() -> nsh_core::element::Registry {
    testing::set(rayx_services_platform::register)
}

#[test]
fn start_keeps_the_platform_service_current_from_nsh() {
    let app = testing::start(set(), PLATFORM, None);
    // NSH's headless surface is 390 points wide.
    assert_eq!(app.shown("ready"), "ready 390");
    app.engine()
        .control()
        .resize(Size::new(px(800.0), px(600.0)));
    assert_eq!(app.shown("resized"), "resized 800x600");
}
