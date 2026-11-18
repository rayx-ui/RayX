//! A selection service written in TS, through NSH's engine, headless: registered under the
//! `SelectionService` class (the contract's token), Rust resolves it as `dyn SelectionService`,
//! reads and follows its state and awaits its commands.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::{Arc, Mutex};

use nsh_core::prelude::*;
use rayx_app::services::selection::{SelectionAreaId, SelectionService, SelectionStatus};
use rayx_app_nsh::testing;
use rayx_app_nsh::ts::ServiceProvider;

/// What the subscriber heard.
static HEARD: Mutex<Vec<String>> = Mutex::new(Vec::new());

fn selection(services: &Instance<ServiceProvider>) -> Arc<dyn SelectionService> {
    services
        .with(|services| {
            services
                .services()
                .required_service::<dyn SelectionService>()
        })
        .unwrap()
}

/// What the test reads of the TS implementation from Rust.
#[module]
struct Probe;

#[module]
impl Probe {
    /// Its active area and status.
    pub(crate) fn read(services: Instance<ServiceProvider>) -> String {
        let selection = selection(&services);
        format!(
            "{:?} {}",
            selection.active_area().map(SelectionAreaId::get),
            selection.status().selected_character_count
        )
    }

    /// Hears its changes.
    pub(crate) fn follow(services: Instance<ServiceProvider>) {
        selection(&services)
            .subscribe(|area, status| {
                HEARD.lock().unwrap().push(format!(
                    "{:?}:{}",
                    area.map(SelectionAreaId::get),
                    status.selected_character_count
                ));
            })
            .detach();
    }

    /// What it heard.
    pub(crate) fn heard() -> String {
        HEARD.lock().unwrap().join(",")
    }

    /// Activates `area` through the contract with `characters` selected: `ok`, or the error.
    pub(crate) async fn activate(
        services: Instance<ServiceProvider>,
        area: u64,
        characters: u32,
    ) -> String {
        let status = SelectionStatus {
            participant_count: 1,
            selected_character_count: characters as usize,
            is_collapsed: false,
            is_selecting: false,
        };
        match selection(&services)
            .activate(SelectionAreaId::from_raw(area), status)
            .await
        {
            Ok(()) => "ok".to_owned(),
            Err(error) => format!("error {error}"),
        }
    }

    /// Clears it through the contract.
    pub(crate) async fn clear(services: Instance<ServiceProvider>) -> String {
        match selection(&services).clear().await {
            Ok(()) => "ok".to_owned(),
            Err(error) => format!("error {error}"),
        }
    }
}

const TSX: &str = r#"import { mount, signal, Text } from "nsh";
import { ContractState, Probe, RayXAppBuilder, SelectionService } from "test/app";

const empty = { participant_count: 0, selected_character_count: 0, is_collapsed: false, is_selecting: false };
const state = (area, status) => ({
  active_area: area, status, navigation_revision: 0, active_area_revision: 0, status_revision: 0,
});

class MySelection {
  selection = new ContractState(state(null, empty));
  asked = [];
  async publishStatus(activeArea, status) { this.asked.push(`publish ${activeArea}`); }
  async activate(activeArea, status) {
    if (activeArea === 0) throw new Error("no area 0");
    this.asked.push(`activate ${activeArea} ${status.selected_character_count}`);
    this.selection.publish(state(activeArea, status));
  }
  async deactivate(activeArea) { this.asked.push(`deactivate ${activeArea}`); }
  async clear() {
    this.asked.push("clear");
    this.selection.publish(state(null, empty));
  }
}

const shown = signal("waiting");
setTimeout(async () => {
  const lines = [];
  let mine;
  try {
    const services = await new RayXAppBuilder().addHostFakes()
      .addSingletonAs(SelectionService, () => (mine = new MySelection()))
      .start();
    lines.push(Probe.read(services));
    Probe.follow(services);
    lines.push(await Probe.activate(services, 2, 6));
    lines.push(await Probe.activate(services, 0, 1));
    lines.push(Probe.read(services));
    // The class over it: RayX's class, the implementation behind.
    lines.push(String(services.getRequiredService(SelectionService).activeArea()));
    lines.push(await Probe.clear(services));
    lines.push(mine.asked.join(","));
    lines.push(Probe.heard());
  } catch (error) {
    lines.push(`threw ${error}`);
  }
  shown.set(`done ${lines.join("|")}`);
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn rust_uses_a_selection_service_written_in_ts() {
    let set = testing::set(|set| {
        rayx_services_selection::register(set);
        set.module::<Probe>();
    });
    let shown = testing::run(set, TSX);
    let parts: Vec<&str> = shown.split('|').collect();
    assert_eq!(parts[0], "done None 0", "{shown}");
    assert_eq!(parts[1], "ok", "{shown}");
    assert!(parts[2].starts_with("error "), "{shown}");
    assert!(parts[2].contains("no area 0"), "{shown}");
    assert_eq!(parts[3], "Some(2) 6", "{shown}");
    assert_eq!(parts[4], "2", "{shown}");
    assert_eq!(parts[5], "ok", "{shown}");
    assert_eq!(parts[6], "activate 2 6,clear", "{shown}");
    assert_eq!(parts[7], "Some(2):6,None:0", "{shown}");
    assert_eq!(parts.len(), 8, "{shown}");
}
