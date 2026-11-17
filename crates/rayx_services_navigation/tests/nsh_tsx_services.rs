//! The navigation for TSX through NSH's engine, headless, in a TSX app with no Rust root: the
//! `NavigationService` class reaches RayX's navigation service, and any `dyn NavigationService`
//! (a fake here) when the app registers its own.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::Arc;

use nsh_core::prelude::*;
use rayx_app::AppBuilder as _;
use rayx_app::services::navigation::NavigationService;
use rayx_app::testing::FakeNavigation;
use rayx_app_nsh::testing;
use rayx_app_nsh::ts::RayXAppBuilder;

const TSX: &str = r#"import { mount, signal, Text } from "nsh";
import { NavigationService, RayXAppBuilder } from "test/app";

const shown = signal("waiting");
setTimeout(async () => {
  const lines = [];
  try {
    const services = await new RayXAppBuilder().addHostFakes()
      .addNavigation((options) => { options.addRoute("/").addRoute("/projects/{id}"); })
      .start();

    const navigation = services.getRequiredService(NavigationService);
    const changes = [];
    navigation.onChange((event) => changes.push(event.to.path));
    lines.push(navigation.navigate("/projects/12").status);
    lines.push(navigation.current().route.path);
    lines.push(navigation.back().route.path);
    lines.push(navigation.forward().route.params.id);
    lines.push(String(navigation.contract));
    // Listeners are called on a later turn.
    await new Promise((resolve) => setTimeout(resolve, 0));
    lines.push(`changes ${changes.join(",")}`);
  } catch (error) {
    lines.push(`threw ${error}`);
  }
  shown.set(`done ${lines.join("|")}`);
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn the_navigation_class_reaches_rayx_s_navigation_service() {
    let set = testing::set(rayx_services_navigation::register);
    assert_eq!(
        testing::run(set, TSX),
        "done Success|/projects/12|/|12|null|changes /projects/12,/,/projects/12"
    );
}

/// `builder.addFakeNavigation()`: a fake as the app's `dyn NavigationService`.
struct FakeNavigationBuilder;

#[class_extension(RayXAppBuilder)]
impl FakeNavigationBuilder {
    /// Registers a fake navigation service.
    pub fn add_fake_navigation(
        this: Instance<RayXAppBuilder>,
    ) -> Result<Instance<RayXAppBuilder>, String> {
        RayXAppBuilder::extend_with(this, |mut builder| {
            builder
                .services_mut()
                .replace_service::<dyn NavigationService>(
                    FakeNavigation::new() as Arc<dyn NavigationService>
                );
            builder
        })
    }
}

const FAKE: &str = r#"import { mount, signal, Text } from "nsh";
import { NavigationService, RayXAppBuilder } from "test/app";

const shown = signal("waiting");
setTimeout(async () => {
  const lines = [];
  try {
    const services = await new RayXAppBuilder().addHostFakes().addFakeNavigation().start();
    const navigation = services.getRequiredService(NavigationService);
    lines.push(navigation.navigate("/anywhere").route.path);
    const panel = navigation.region("panel");
    panel.navigate("/a");
    panel.navigate("/b");
    lines.push(panel.pages().map((page) => page.path).join(">"));
    lines.push(panel.goTo(0).route.path);
    lines.push(`${panel.close()} ${panel.pages().length}`);
  } catch (error) {
    lines.push(`threw ${error}`);
  }
  shown.set(`done ${lines.join("|")}`);
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn the_navigation_classes_reach_any_navigation_service() {
    let set = testing::set(|set| {
        rayx_services_navigation::register(set);
        set.class_extension::<FakeNavigationBuilder>();
    });
    assert_eq!(testing::run(set, FAKE), "done /anywhere|/a>/b|/a|true 0");
}
