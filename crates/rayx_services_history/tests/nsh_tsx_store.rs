//! A history store written in TS, headless: `addHistoryWithStore(() => new MyStore())` makes the
//! TS class (it implements `HistoryStoreContract`) the app's history store: RayX's history loads
//! each scope's journal from it, saves the journal (JSON text, by revision) behind every change
//! and deletes a cleared scope's (`deleteScope`).

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::{Arc, Mutex};

use nsh_core::prelude::*;
use rayx_app::AppBuilder as _;
use rayx_app::services::history::HistoryService;
use rayx_app::services::operations::{
    DomainOperation, OperationApplyResult, OperationOrigin, OperationService,
    OperationTargetHandler, OperationTargetId,
};
use rayx_app::testing::FakeOperations;
use rayx_app_nsh::testing;
use rayx_app_nsh::ts::{RayXAppBuilder, ServiceProvider};
use rayx_services_history::{HistoryJournalStore, HistoryStore};
use serde::{Deserialize, Serialize};

/// `builder.addFakeOperations()`: what the history observes.
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
}

#[derive(Clone, Deserialize, Serialize)]
struct Set(u32);

impl DomainOperation for Set {
    const KIND: &'static str = "tsx.history.store.set";
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
    /// Whether the history, its store and the TS store resolve in Rust.
    pub(crate) fn registered(services: Instance<ServiceProvider>) -> String {
        services.with(|services| {
            let services = services.services();
            format!(
                "{} {} {}",
                services.service::<dyn HistoryService>().is_some(),
                services.service::<dyn HistoryStore>().is_some(),
                services.service::<dyn HistoryJournalStore>().is_some(),
            )
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

const TSX: &str = r#"import { mount, signal, Text } from "nsh";
import { HistoryService, Probe, RayXAppBuilder } from "test/app";

class MyStore {
  journals = new Map();
  calls = [];
  async load(scope) {
    this.calls.push(`load ${scope}`);
    return this.journals.get(scope)?.text ?? null;
  }
  async save(scope, journal, revision, expectedRevision) {
    const saved = this.journals.get(scope)?.revision ?? null;
    if (saved !== expectedRevision) return false;
    this.journals.set(scope, { text: journal, revision });
    this.calls.push(`save ${scope} ${revision}`);
    return true;
  }
  async deleteScope(scope) {
    this.journals.delete(scope);
    this.calls.push(`delete ${scope}`);
  }
}

const tick = () => new Promise((resolve) => setTimeout(resolve, 10));
const shown = signal("waiting");
setTimeout(async () => {
  const lines = [];
  let store;
  try {
    const services = await new RayXAppBuilder().addHostFakes().addFakeOperations()
      .addHistoryWithStore(() => (store = new MyStore()), (options) => options.callTimeoutMs(5000))
      .start();
    lines.push(Probe.registered(services));
    const history = services.getRequiredService(HistoryService);
    await history.bindTargetScope("doc", "document");
    Probe.edit(services, 3);
    Probe.edit(services, 5);
    await tick();
    const saved = store.journals.get("document");
    const journal = JSON.parse(saved.text);
    lines.push(`${saved.revision === journal.revision} ${journal.scope_id} ${journal.entries.length}`);
    lines.push(`${await history.undo("document")} ${Probe.value()}`);
    await tick();
    await history.clearScope("document");
    await tick();
    lines.push(String(store.journals.has("document")));
    lines.push(store.calls.map((call) => call.split(" ")[0]).filter((call, index, calls) => calls.indexOf(call) === index).join(","));
  } catch (error) {
    lines.push(`threw ${error}`);
  }
  shown.set(`done ${lines.join("|")}`);
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn a_history_store_written_in_ts_keeps_the_journals() {
    assert_eq!(
        testing::run(set(), TSX),
        "done true true true|true document 2|true 3|false|load,save,delete"
    );
}

/// The builder method's declaration: the factory of a `HistoryStoreContract`.
#[test]
fn add_history_with_store_takes_a_store_factory() {
    let ts = testing::declarations(&set());
    for declared in [
        "addHistoryWithStore(store: (services: ServiceProvider) => HistoryStoreContract",
        "export interface HistoryStoreContract {",
        "  load(scope: string): Promise<string | null>;",
    ] {
        assert!(ts.contains(declared), "no `{declared}` in\n{ts}");
    }
}
