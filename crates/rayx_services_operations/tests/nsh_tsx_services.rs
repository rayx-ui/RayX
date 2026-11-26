//! The operation service for TSX through NSH's engine, headless: the `OperationService` class
//! reaches RayX's service: it registers a Rust target's handler (an `OperationTarget`), submits
//! records in a transaction, commits it and hears the events.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::{Arc, Mutex};

use nsh_core::JsonValue;
use nsh_core::prelude::*;
use rayx_app::services::operations::{
    DomainOperation, OperationApplyResult, OperationOrigin, OperationRecord, OperationTargetHandle,
    OperationTargetHandler, OperationTargetId,
};
use rayx_app_nsh::testing;
use rayx_services_operations::OperationTarget;
use serde::{Deserialize, Serialize};

#[derive(Clone, Deserialize, Serialize)]
struct Set(u32);

impl DomainOperation for Set {
    const KIND: &'static str = "tsx.set";
    const VERSION: u32 = 1;
}

/// The value the TSX app changes.
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

/// What the test gives TS from Rust.
#[module]
struct Probe;

#[module]
impl Probe {
    /// The handler of the target `id`.
    pub(crate) fn target(id: String) -> Instance<OperationTarget> {
        let id = OperationTargetId::new(id).unwrap();
        Instance::new(OperationTarget::new(OperationTargetHandle::of::<Set, _>(
            id,
            Arc::new(Target),
        )))
    }

    /// The record setting the target `id` to `value`.
    pub(crate) fn record(id: String, value: u32) -> JsonValue {
        let id = OperationTargetId::new(id).unwrap();
        serde_json::to_value(OperationRecord::from_operation(id, &Set(value)).unwrap()).unwrap()
    }

    /// The value.
    pub(crate) fn value() -> u32 {
        *VALUE.lock().unwrap()
    }

    /// Submits a JSON record to `target_id` from a background OS thread, proving that a TS
    /// `OperationTarget.fromHandler` target dispatches safely back onto the engine thread.
    pub(crate) async fn submit_from_background_thread(
        services: Instance<rayx_app_nsh::ts::ServiceProvider>,
        target_id: String,
        value: u32,
    ) -> Result<u64, String> {
        let operations = services
            .with(|services| {
                services
                    .services()
                    .required_service::<dyn rayx_app::services::operations::OperationService>()
            })
            .map_err(|error| error.to_string())?;
        let id = OperationTargetId::new(target_id).map_err(|error| error.to_string())?;
        let record =
            OperationRecord::from_json(id, "js.set", 1, serde_json::json!({ "value": value }))
                .map_err(|error| error.to_string())?;
        let (tx, rx) = futures::channel::oneshot::channel();
        std::thread::spawn(move || {
            let outcome = futures::executor::block_on(operations.submit_record(
                record,
                OperationOrigin::Agent("bg-agent".into()),
                None,
            ))
            .map(|receipt| receipt.after_revision)
            .map_err(|error| error.to_string());
            let _ = tx.send(outcome);
        });
        rx.await
            .map_err(|_| "background thread dropped".to_owned())?
    }
}

const TSX: &str = r#"import { mount, signal, Text } from "nsh";
import { OperationService, Probe, RayXAppBuilder } from "test/app";

const shown = signal("waiting");
setTimeout(async () => {
  const lines = [];
  try {
    const services = await new RayXAppBuilder().addHostFakes().addOperations().start();
    const operations = services.getRequiredService(OperationService);
    const events = [];
    operations.onEvent((event) => events.push(event.Committed ? `committed ${event.Committed.after_revision}` : `failed ${event.Failed.error_code}`));
    await operations.registerTarget("doc", Probe.target("doc"));
    lines.push(operations.targets.join(","));
    const id = await operations.begin();
    const first = await operations.submitRecord(Probe.record("doc", 3), "LocalUser", id);
    await operations.submitRecord(Probe.record("doc", 5), "LocalUser", id);
    lines.push(`${first.status} ${Probe.value()} ${events.length}`);
    await operations.commit(id);
    const aborted = await operations.begin();
    await operations.submitRecord(Probe.record("doc", 9), "LocalUser", aborted);
    await operations.abort(aborted);
    lines.push(String(Probe.value()));
    lines.push(String(await operations.releaseTarget("doc")));
    lines.push(await operations.submitRecord(Probe.record("doc", 1), "System").then(() => "applied", (error) => `rejected ${error.message}`));
    lines.push(String(operations.contract));
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
fn the_operation_class_reaches_rayx_s_operation_service() {
    let set = testing::set(|set| {
        rayx_services_operations::register(set);
        set.module::<Probe>();
    });
    assert_eq!(
        testing::run(set, TSX),
        "done doc|Applied 5 0|5|true|rejected target_released|null\
         |committed 3,committed 5"
    );
}

const FROM_HANDLER_TSX: &str = r#"import { mount, signal, Text } from "nsh";
import { OperationService, OperationTarget, Probe, RayXAppBuilder } from "test/app";

const shown = signal("waiting");
setTimeout(async () => {
  const lines = [];
  try {
    const services = await new RayXAppBuilder().addHostFakes().addOperations().start();
    const operations = services.getRequiredService(OperationService);
    let current = 0;
    const seenOrigins = [];
    const target = OperationTarget.fromHandler((record, origin) => {
      const payload = record.payloadJson;
      if (payload.value < 0) throw new Error("negative not allowed");
      seenOrigins.push(typeof origin === "string" ? origin : Object.keys(origin)[0]);
      const before = current;
      current = payload.value;
      return {
        status: "Applied",
        inverse: {
          target_id: record.target_id,
          kind: record.kind,
          version: record.version,
          payloadJson: { value: before },
        },
        affected_ids: ["value"],
        before_revision: before,
        after_revision: current,
      };
    });
    await operations.registerTarget("jsdoc", target);

    const r1 = await operations.submitJson(
      "jsdoc",
      "js.set",
      1,
      { value: 12 },
      "LocalUser",
      { command_id: "cmd.set", label_key: "op.set", tags: ["ui"], extra: { n: 1 } },
    );
    lines.push(`${r1.status} ${current} ${r1.forward.metadata.label_key} ${operations.payloadJson(r1.forward).value} ${OperationTarget.payloadJson(r1.inverse).value}`);

    const rec = OperationTarget.recordFromJson("jsdoc", "js.set", 1, { value: 20 }, { label_key: "op.set2" });
    const r2 = await operations.submitRecord(rec, { Agent: "planner" });
    lines.push(`${current} ${r2.origin.Agent} ${r2.forward.metadata.label_key}`);

    const fail = await operations
      .submitJson("jsdoc", "js.set", 1, { value: -5 }, "LocalUser")
      .then(() => "ok", (err) => `err:${err.message}`);
    lines.push(`${fail} ${current}`);

    const bgAfter = await Probe.submitFromBackgroundThread(services, "jsdoc", 33);
    lines.push(`bg:${bgAfter} ${current} ${seenOrigins.join(",")}`);
  } catch (error) {
    lines.push(`threw ${error}`);
  }
  shown.set(`done ${lines.join("|")}`);
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn ts_handler_target_and_submit_json_work_on_and_off_engine_thread() {
    let set = testing::set(|set| {
        rayx_services_operations::register(set);
        set.module::<Probe>();
    });
    assert_eq!(
        testing::run(set, FROM_HANDLER_TSX),
        "done Applied 12 op.set 12 0|20 planner op.set2|err:apply_failed 20|bg:33 33 LocalUser,Agent,Agent"
    );
}
