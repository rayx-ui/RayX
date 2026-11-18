//! The selection for TSX through NSH's engine, headless, in a TSX app with no Rust root:
//! `builder.addSelection()` registers RayX's selection service over the app's navigation (a
//! fake here), the `SelectionService` class reaches it, its commands resolve and `onChange`
//! hears its changes; without a navigation the app does not start, naming what to add.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::Arc;

use nsh_core::prelude::*;
use rayx_app::AppBuilder as _;
use rayx_app::services::navigation::NavigationService;
use rayx_app::testing::FakeNavigation;
use rayx_app_nsh::testing;
use rayx_app_nsh::ts::RayXAppBuilder;

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

/// What the test drives from Rust.
#[module]
struct Probe;

#[module]
impl Probe {
    /// Navigates the app's navigation to `uri`.
    pub(crate) async fn navigate(
        services: Instance<rayx_app_nsh::ts::ServiceProvider>,
        uri: String,
    ) {
        let navigation = services
            .with(|services| {
                services
                    .services()
                    .required_service::<dyn NavigationService>()
            })
            .unwrap();
        navigation.navigate_to(uri).await.unwrap();
    }
}

fn set() -> nsh_core::element::Registry {
    testing::set(|set| {
        rayx_services_selection::register(set);
        set.class_extension::<FakeNavigationBuilder>()
            .module::<Probe>();
    })
}

const TSX: &str = r#"import { mount, signal, Text } from "nsh";
import { Probe, RayXAppBuilder, SelectionService } from "test/app";

const status = (characters) => ({
  participant_count: 1, selected_character_count: characters, is_collapsed: false, is_selecting: false,
});

const shown = signal("waiting");
setTimeout(async () => {
  const lines = [];
  try {
    const services = await new RayXAppBuilder().addHostFakes()
      .addSelection()
      .addFakeNavigation()
      .start();
    const selection = services.getRequiredService(SelectionService);
    const changes = [];
    selection.onChange((state) => changes.push(`${state.active_area}:${state.status.selected_character_count}`));
    lines.push(String(selection.contract));
    lines.push(String(selection.activeArea()));
    await selection.activate(3, status(4));
    await selection.publishStatus(9, status(7));
    lines.push(`${selection.activeArea()} ${selection.status().selected_character_count}`);
    await selection.publishStatus(3, status(5));
    lines.push(JSON.stringify(selection.state()));
    await Probe.navigate(services, "/b");
    lines.push(`${selection.activeArea()} ${selection.state().navigation_revision}`);
    await selection.activate(4, status(1));
    await selection.deactivate(3);
    await selection.deactivate(4);
    await selection.clear();
    try {
      await selection.activate(5, { nothing: true });
      lines.push("took a bad status");
    } catch (error) {
      lines.push(`rejected ${error}`);
    }
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
fn the_selection_class_reaches_rayx_s_selection_service() {
    let shown = testing::run(set(), TSX);
    let parts: Vec<&str> = shown.split('|').collect();
    assert_eq!(parts[0], "done null", "{shown}");
    assert_eq!(parts[1], "null", "{shown}");
    assert_eq!(parts[2], "3 4", "{shown}");
    assert_eq!(
        parts[3],
        r#"{"active_area":3,"status":{"participant_count":1,"selected_character_count":5,"is_collapsed":false,"is_selecting":false},"navigation_revision":0,"active_area_revision":1,"status_revision":2}"#,
        "{shown}"
    );
    assert_eq!(parts[4], "null 1", "{shown}");
    assert!(parts[5].starts_with("rejected "), "{shown}");
    assert!(parts[5].contains("SelectionStatus"), "{shown}");
    assert_eq!(parts[6], "changes 3:4,3:5,null:0,4:1,null:0", "{shown}");
    assert_eq!(parts.len(), 7, "{shown}");
}

const WITHOUT_NAVIGATION: &str = r#"import { mount, signal, Text } from "nsh";
import { RayXAppBuilder } from "test/app";

const shown = signal("waiting");
setTimeout(async () => {
  try {
    await new RayXAppBuilder().addHostFakes().addSelection().start();
    shown.set("done started");
  } catch (error) {
    shown.set(`done threw ${error}`);
  }
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn the_app_does_not_start_without_a_navigation_and_names_what_to_add() {
    let shown = testing::run(set(), WITHOUT_NAVIGATION);
    assert!(shown.starts_with("done threw"), "{shown}");
    assert!(shown.contains("NavigationService"), "{shown}");
    assert!(shown.contains("addNavigation"), "{shown}");
}
