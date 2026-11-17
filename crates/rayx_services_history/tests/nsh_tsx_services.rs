//! The history for TSX through NSH's engine, headless: `addHistory()` and
//! `addPersistentHistory()` register RayX's history and require what it observes and keeps its
//! journals in (they register neither), and the `HistoryService` class undoes and redoes what a
//! Rust target applied, hearing each step. (The operation service and the key-value store are
//! fakes here: `addFakeOperations()`, `addFakeStorage()`.)

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::{Arc, Mutex};

use nsh_core::prelude::*;
use rayx_app::AppBuilder as _;
use rayx_app::services::history::HistoryService;
use rayx_app::services::operations::{
    DomainOperation, OperationApplyResult, OperationOrigin, OperationService,
    OperationTargetHandler, OperationTargetId,
};
use rayx_app::services::storage::KeyValueStore;
use rayx_app::testing::{FakeKeyValueStore, FakeOperations};
use rayx_app_nsh::testing;
use rayx_app_nsh::ts::{RayXAppBuilder, ServiceProvider};
use rayx_services_history::HistoryStore;
use serde::{Deserialize, Serialize};

/// `builder.addFakeOperations()` and `builder.addFakeStorage()`: fakes of what history needs.
struct Fakes;

#[class_extension(RayXAppBuilder)]
impl Fakes {
    /// Registers a fake operation service.
    pub fn add_fake_operations(
        this: Instance<RayXAppBuilder>,
    ) -> Result<Instance<RayXAppBuilder>, String> {
        RayXAppBuilder::extend_with(this, |mut builder| {
            builder
                .services_mut()
                .add_singleton_as::<dyn OperationService>(
                    FakeOperations::new() as Arc<dyn OperationService>
                );
            builder
        })
    }

    /// Registers a fake key-value store.
    pub fn add_fake_storage(
        this: Instance<RayXAppBuilder>,
    ) -> Result<Instance<RayXAppBuilder>, String> {
        RayXAppBuilder::extend_with(this, |mut builder| {
            builder
                .services_mut()
                .add_singleton_as::<dyn KeyValueStore>(
                    FakeKeyValueStore::new() as Arc<dyn KeyValueStore>
                );
            builder
        })
    }
}

#[derive(Clone, Deserialize, Serialize)]
struct Set(u32);

impl DomainOperation for Set {
    const KIND: &'static str = "tsx.history.set";
    const VERSION: u32 = 1;
}

/// The value the app edits.
static VALUE: Mutex<u32> = Mutex::new(0);

struct Target;

impl OperationTargetHandler<Set> for Target {
    fn apply_atomically(
        &self,
        operation: &Set,
        _origin: &OperationOrigin,
    ) -> Result<OperationApplyResult<Set>, String> {
        let mut value = VALUE.lock().unwrap();
        let before = std::mem::replace(&mut *value, operation.0);
        Ok(OperationApplyResult::applied(
            Set(before),
            ["value"],
            u64::from(before),
            u64::from(operation.0),
        ))
    }
}

/// What the test does from Rust.
#[module]
struct Probe;

#[module]
impl Probe {
    /// The services of the history's that resolve, by name.
    pub(crate) fn registered(services: Instance<ServiceProvider>) -> String {
        services.with(|services| {
            let services = services.services();
            let mut names = Vec::new();
            if services.service::<dyn KeyValueStore>().is_some() {
                names.push("KeyValueStore");
            }
            if services.service::<dyn OperationService>().is_some() {
                names.push("OperationService");
            }
            if services.service::<dyn HistoryService>().is_some() {
                names.push("HistoryService");
            }
            if services.service::<dyn HistoryStore>().is_some() {
                names.push("HistoryStore");
            }
            names.join(",")
        })
    }

    /// Registers the target `doc` and sets it to `value` as the local user.
    pub(crate) fn edit(services: Instance<ServiceProvider>, value: u32) {
        let operations = services
            .with(|services| {
                services
                    .services()
                    .required_service::<dyn OperationService>()
            })
            .unwrap();
        let id = OperationTargetId::new("doc").unwrap();
        if !operations.has_target(&id) {
            futures::executor::block_on(operations.register_handler(id.clone(), Arc::new(Target)))
                .unwrap();
        }
        futures::executor::block_on(operations.submit(&id, Set(value), OperationOrigin::LocalUser))
            .unwrap();
    }

    /// The value.
    pub(crate) fn value() -> u32 {
        *VALUE.lock().unwrap()
    }
}

fn set() -> nsh_core::element::Registry {
    testing::set(|set| {
        rayx_services_history::register(set);
        set.module::<Probe>().class_extension::<Fakes>();
    })
}

/// Each app on an engine of its own: what a chain registers, or why it does not start.
const IMPLIED: &str = r#"import { mount, signal, Text } from "nsh";
import { Probe, RayXAppBuilder } from "test/app";

const shown = signal("waiting");
setTimeout(async () => {
  const lines = [];
  try {
    const services = await new RayXAppBuilder().addHostFakes()
      @CHAIN@
      .start();
    lines.push(Probe.registered(services));
  } catch (error) {
    lines.push(`threw ${error}`);
  }
  shown.set(`done ${lines.join("|")}`);
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn the_builder_methods_register_the_history_and_require_what_it_needs() {
    for (chain, registered) in [
        (
            ".addFakeOperations().addHistory()",
            "OperationService,HistoryService",
        ),
        (
            ".addFakeStorage().addFakeOperations().addPersistentHistory()",
            "KeyValueStore,OperationService,HistoryService,HistoryStore",
        ),
    ] {
        let shown = testing::run(set(), &IMPLIED.replace("@CHAIN@", chain));
        assert_eq!(shown, format!("done {registered}"), "{chain}");
    }
    for chain in [".addHistory()", ".addFakeStorage().addPersistentHistory()"] {
        let shown = testing::run(set(), &IMPLIED.replace("@CHAIN@", chain));
        assert!(shown.starts_with("done threw"), "{chain}: {shown}");
        assert!(shown.contains("OperationService"), "{chain}: {shown}");
        assert!(shown.contains("addOperations"), "{chain}: {shown}");
    }
    let shown = testing::run(
        set(),
        &IMPLIED.replace("@CHAIN@", ".addFakeOperations().addPersistentHistory()"),
    );
    assert!(shown.starts_with("done threw"), "{shown}");
    assert!(shown.contains("KeyValueStore"), "{shown}");
    assert!(shown.contains("addStorage"), "{shown}");
}

const TSX: &str = r#"import { mount, signal, Text } from "nsh";
import { HistoryService, Probe, RayXAppBuilder } from "test/app";

const shown = signal("waiting");
setTimeout(async () => {
  const lines = [];
  try {
    const services = await new RayXAppBuilder().addHostFakes().addFakeOperations().addHistory().start();
    const history = services.getRequiredService(HistoryService);
    const events = [];
    history.onChange((event) => events.push(Object.keys(event)[0]));
    await history.bindTargetScope("doc", "document");
    Probe.edit(services, 3);
    Probe.edit(services, 5);
    lines.push(`${history.canUndo("document")} ${history.canRedo("document")} ${history.depths("document").join(",")}`);
    lines.push(`${await history.undo("document")} ${Probe.value()}`);
    lines.push(`${history.depths("document").join(",")} ${(await history.retainedRecords("document")).length}`);
    lines.push(`${await history.redo("document")} ${Probe.value()}`);
    await history.setScopeEntryLimit("document", 1);
    lines.push(history.depths("document").join(","));
    await history.clearScope("document");
    lines.push(`${history.canUndo("document")} ${await history.undo("document")}`);
    lines.push(await history.undo("").then(() => "undone", (error) => `rejected ${error.message}`));
    lines.push(String(history.contract));
    // Listeners are called on a later turn.
    await new Promise((resolve) => setTimeout(resolve, 0));
    lines.push(events.join(","));
  } catch (error) {
    lines.push(`threw ${error}`);
  }
  shown.set(`done ${lines.join("|")}`);
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn the_history_class_reaches_rayx_s_history() {
    assert_eq!(
        testing::run(set(), TSX),
        "done true false 2,0|true 3|1,1 4|true 5|1,0|false false\
         |rejected invalid_history_scope|null\
         |Recorded,Recorded,UndoCompleted,RedoCompleted,AvailabilityChanged,ScopeCleared"
    );
}
