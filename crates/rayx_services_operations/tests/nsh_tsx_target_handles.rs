//! A target's handler that crossed to an operation service written in TS lives only as long as
//! the call when TS does not take it: a `registerTarget` that throws before
//! `OperationTarget.of(handle)` releases it with the call, and one TS took lives with TS's
//! target.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::{Arc, Mutex, Weak};

use nsh_core::prelude::*;
use rayx_app::services::operations::{
    DomainOperation, OperationApplyResult, OperationOrigin, OperationService,
    OperationTargetHandler, OperationTargetId,
};
use rayx_app_nsh::testing;
use rayx_app_nsh::ts::ServiceProvider;
use serde::{Deserialize, Serialize};

/// The handlers that crossed, weakly.
static CROSSED: Mutex<Vec<Weak<Target>>> = Mutex::new(Vec::new());

#[derive(Clone, Deserialize, Serialize)]
struct Set(u32);

impl DomainOperation for Set {
    const KIND: &'static str = "handles.set";
    const VERSION: u32 = 1;
}

struct Target;

impl OperationTargetHandler<Set> for Target {
    fn apply_atomically(
        &self,
        operation: &Set,
        _origin: &OperationOrigin,
    ) -> Result<OperationApplyResult<Set>, String> {
        Ok(OperationApplyResult::applied(
            Set(0),
            ["value"],
            0,
            u64::from(operation.0),
        ))
    }
}

/// What the test does with the TS implementation from Rust.
#[module]
struct HandleProbe;

#[module]
impl HandleProbe {
    /// Registers a handler as the target `target_id`: `ok`, or the error.
    pub(crate) async fn register(services: Instance<ServiceProvider>, target_id: String) -> String {
        let operations = services
            .with(|services| {
                services
                    .services()
                    .required_service::<dyn OperationService>()
            })
            .unwrap();
        let target = Arc::new(Target);
        CROSSED.lock().unwrap().push(Arc::downgrade(&target));
        let id = OperationTargetId::new(target_id).unwrap();
        match operations.register_handler::<Set, _>(id, target).await {
            Ok(()) => "ok".to_owned(),
            Err(_) => "refused".to_owned(),
        }
    }

    /// Which handlers are alive, in order.
    pub(crate) fn alive() -> String {
        CROSSED
            .lock()
            .unwrap()
            .iter()
            .map(|target| {
                if target.upgrade().is_some() {
                    "alive"
                } else {
                    "gone"
                }
            })
            .collect::<Vec<_>>()
            .join(",")
    }
}

const TSX: &str = r#"import { mount, signal, Text } from "nsh";
import {
  ContractEvents, ContractState, HandleProbe, OperationService, OperationTarget, RayXAppBuilder,
} from "test/app";

class MyOperations {
  operations = new ContractState({ targets: [], recent_transactions: [], recent_failure_codes: [] });
  events = new ContractEvents();
  instanceId = 5;
  targets = new Map();
  async registerTarget(targetId, handle) {
    if (targetId === "refused") throw new Error("refused before taking the handle");
    this.targets.set(targetId, OperationTarget.of(handle));
  }
  async releaseTarget(targetId) { return this.targets.delete(targetId); }
  async begin() { return 1; }
  async commit(transactionId) {}
  async abort(transactionId) {}
  async submitRecord(record, origin, transactionId) { throw new Error("unused"); }
  async publishRemoteRecord(record, peerId, beforeRevision, afterRevision, affectedIds) {
    throw new Error("unused");
  }
}

const shown = signal("waiting");
setTimeout(async () => {
  const lines = [];
  try {
    const services = await new RayXAppBuilder().addHostFakes()
      .addSingletonAs(OperationService, () => new MyOperations())
      .start();
    lines.push(await HandleProbe.register(services, "refused"));
    lines.push(await HandleProbe.register(services, "taken"));
    lines.push(HandleProbe.alive());
  } catch (error) {
    lines.push(`threw ${error}`);
  }
  shown.set(`done ${lines.join("|")}`);
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn a_handle_ts_did_not_take_is_released_with_the_call() {
    let set = testing::set(|set| {
        rayx_services_operations::register(set);
        set.module::<HandleProbe>();
    });
    let shown = testing::run(set, TSX);
    assert_eq!(shown, "done refused|ok|gone,alive", "{shown}");
}
