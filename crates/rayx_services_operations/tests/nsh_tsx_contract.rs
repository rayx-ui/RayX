//! An operation service written in TS, through NSH's engine, headless: registered under the
//! `OperationService` class (the contract's token), Rust resolves it as `dyn OperationService`,
//! reads its constant and state, hears its events and awaits its commands; a Rust target's
//! handler reaches it as a handle, which it takes with `OperationTarget.of(handle)` and calls.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::{Arc, Mutex};

use nsh_core::prelude::*;
use rayx_app::services::operations::{
    DomainOperation, OperationApplyResult, OperationEvent, OperationOrigin, OperationService,
    OperationTargetHandler, OperationTargetId,
};
use rayx_app_nsh::testing;
use rayx_app_nsh::ts::ServiceProvider;
use serde::{Deserialize, Serialize};

/// What the subscriber heard.
static HEARD: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// The value the TS service changes through the Rust handler.
static VALUE: Mutex<u32> = Mutex::new(0);

#[derive(Clone, Deserialize, Serialize)]
struct Set(u32);

impl DomainOperation for Set {
    const KIND: &'static str = "contract.set";
    const VERSION: u32 = 1;
}

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

fn operations(services: &Instance<ServiceProvider>) -> Arc<dyn OperationService> {
    services
        .with(|services| {
            services
                .services()
                .required_service::<dyn OperationService>()
        })
        .unwrap()
}

/// What the test does with the TS implementation from Rust.
#[module]
struct Probe;

#[module]
impl Probe {
    /// Its constant and state.
    pub(crate) fn read(services: Instance<ServiceProvider>) -> String {
        let operations = operations(&services);
        let targets: Vec<String> = operations
            .targets()
            .iter()
            .map(|id| id.as_str().to_owned())
            .collect();
        format!("{} [{}]", operations.instance_id(), targets.join(","))
    }

    /// Hears its events.
    pub(crate) fn follow(services: Instance<ServiceProvider>) {
        operations(&services)
            .subscribe(|event| {
                if let OperationEvent::Committed(receipt) = event {
                    HEARD.lock().unwrap().push(format!(
                        "{} {}",
                        receipt.target_id.as_str(),
                        receipt.after_revision
                    ));
                }
            })
            .detach();
    }

    /// What it heard.
    pub(crate) fn heard() -> String {
        HEARD.lock().unwrap().join(",")
    }

    /// Registers the Rust target `doc` through the contract, then sets it to `value`: the
    /// receipt's status and inverse, and the value, or the error.
    pub(crate) async fn edit(services: Instance<ServiceProvider>, value: u32) -> String {
        let operations = operations(&services);
        let id = OperationTargetId::new("doc").unwrap();
        if let Err(error) = operations
            .register_handler::<Set, _>(id.clone(), Arc::new(Target))
            .await
        {
            return format!("error {error}");
        }
        match operations
            .submit(&id, Set(value), OperationOrigin::LocalUser)
            .await
        {
            Ok(receipt) => format!(
                "{:?} {} {}",
                receipt.status,
                receipt.inverse.unwrap().decode::<Set>().unwrap().0,
                *VALUE.lock().unwrap()
            ),
            Err(error) => format!("error {error}"),
        }
    }

    /// Submits to a target the TS service does not have: the error.
    pub(crate) async fn missing(services: Instance<ServiceProvider>) -> String {
        let operations = operations(&services);
        let id = OperationTargetId::new("missing").unwrap();
        match operations
            .submit(&id, Set(1), OperationOrigin::System)
            .await
        {
            Ok(_) => "applied".to_owned(),
            Err(error) => format!("error {error}"),
        }
    }
}

const TSX: &str = r#"import { mount, signal, Text } from "nsh";
import {
  ContractEvents, ContractState, OperationService, OperationTarget, Probe, RayXAppBuilder,
} from "test/app";

class MyOperations {
  operations = new ContractState({ targets: [], recent_transactions: [], recent_failure_codes: [] });
  events = new ContractEvents();
  instanceId = 77;
  targets = new Map();
  next = 0;
  async registerTarget(targetId, handle) {
    this.targets.set(targetId, OperationTarget.of(handle));
    this.operations.publish({ targets: [...this.targets.keys()], recent_transactions: [], recent_failure_codes: [] });
  }
  async releaseTarget(targetId) { return this.targets.delete(targetId); }
  async begin() { return ++this.next; }
  async commit(transactionId) {}
  async abort(transactionId) {}
  async submitRecord(record, origin, transactionId) {
    const target = this.targets.get(record.target_id);
    if (!target) throw new Error(`no target ${record.target_id}`);
    const result = target.apply(record, origin);
    const receipt = { transaction_id: transactionId ?? ++this.next, target_id: record.target_id, forward: record, origin, ...result };
    this.events.publish({ Committed: receipt });
    return receipt;
  }
  async publishRemoteRecord(record, peerId, beforeRevision, afterRevision, affectedIds) {
    throw new Error("no peers");
  }
}

const shown = signal("waiting");
setTimeout(async () => {
  const lines = [];
  let mine;
  try {
    const services = await new RayXAppBuilder().addHostFakes()
      .addSingletonAs(OperationService, () => (mine = new MyOperations()))
      .start();
    lines.push(Probe.read(services));
    Probe.follow(services);
    lines.push(await Probe.edit(services, 4));
    lines.push(await Probe.missing(services));
    lines.push(Probe.read(services));
    lines.push(Probe.heard());
    // The class over it: RayX's class, the implementation behind.
    lines.push(services.getRequiredService(OperationService).targets.join(","));
    // A handle is taken once.
    lines.push((() => { try { OperationTarget.of({ handle: 1 }); return "taken"; } catch (e) { return "refused"; } })());
  } catch (error) {
    lines.push(`threw ${error}`);
  }
  shown.set(`done ${lines.join("|")}`);
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn rust_uses_an_operation_service_written_in_ts_through_the_handler_s_class_handle() {
    let set = testing::set(|set| {
        rayx_services_operations::register(set);
        set.module::<Probe>();
    });
    let shown = testing::run(set, TSX);
    let parts: Vec<&str> = shown.split('|').collect();
    assert_eq!(parts[0], "done 77 []", "{shown}");
    assert_eq!(parts[1], "Applied 0 4", "{shown}");
    assert!(parts[2].starts_with("error "), "{shown}");
    assert!(parts[2].contains("no target missing"), "{shown}");
    assert_eq!(parts[3], "77 [doc]", "{shown}");
    assert_eq!(parts[4], "doc 4", "{shown}");
    assert_eq!(parts[5], "doc", "{shown}");
    assert_eq!(parts[6], "refused", "{shown}");
    assert_eq!(parts.len(), 7, "{shown}");
}
