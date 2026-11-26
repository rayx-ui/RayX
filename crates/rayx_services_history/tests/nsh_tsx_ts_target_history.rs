//! End-to-end TSX test combining `OperationTarget.fromHandler`, `OperationService.submitJson`,
//! `OperationMetadata`, `OperationOrigin::Agent`, and `HistoryService` undo/redo with
//! `undoLabel`/`redoLabel` and `HistoryOptions.recordAgentOperations`.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use rayx_app_nsh::testing;

fn set() -> nsh_core::element::Registry {
    testing::set(|set| {
        rayx_services_operations::register(set);
        rayx_services_history::register(set);
    })
}

const END_TO_END_TSX: &str = r#"import { mount, signal, Text } from "nsh";
import { HistoryService, OperationService, OperationTarget, RayXAppBuilder } from "test/app";

const shown = signal("waiting");
setTimeout(async () => {
  const lines = [];
  try {
    const services = await new RayXAppBuilder()
      .addHostFakes()
      .addOperations()
      .addHistory()
      .start();
    const operations = services.getRequiredService(OperationService);
    const history = services.getRequiredService(HistoryService);

    let value = 0;
    const replayOrigins = [];
    const target = OperationTarget.fromHandler((record, origin) => {
      if (origin === "HistoryUndo" || origin === "HistoryRedo") {
        replayOrigins.push(origin);
      }
      const next = record.payloadJson.value;
      const before = value;
      value = next;
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
        after_revision: next,
      };
    });

    await operations.registerTarget("doc", target);
    await history.bindTargetScope("doc", "editor");
    lines.push(`${history.undoLabel("editor")},${history.redoLabel("editor")}`);

    // LocalUser edit with metadata label_key.
    await operations.submitJson(
      "doc",
      "doc.set",
      1,
      { value: 10 },
      "LocalUser",
      { command_id: "doc.set", label_key: "history.setTen", tags: ["user"] },
    );
    lines.push(`${value} ${history.depths("editor").join(",")} ${history.undoLabel("editor")},${history.redoLabel("editor")}`);

    // Agent multi-op transaction recorded as a single undoable entry with first receipt's label_key.
    const tx = await operations.begin();
    await operations.submitJson(
      "doc",
      "doc.set",
      1,
      { value: 15 },
      { Agent: "assistant-1" },
      { label_key: "history.agentBatch", tags: ["agent"] },
      tx,
    );
    await operations.submitJson(
      "doc",
      "doc.set",
      1,
      { value: 25 },
      { Agent: "assistant-1" },
      { label_key: "history.secondStep" },
      tx,
    );
    await operations.commit(tx);
    lines.push(`${value} ${history.depths("editor").join(",")} ${history.undoLabel("editor")},${history.redoLabel("editor")}`);

    // Undo agent batch in one step.
    await history.undo("editor");
    lines.push(`${value} ${history.depths("editor").join(",")} ${history.undoLabel("editor")},${history.redoLabel("editor")}`);

    // Undo user edit.
    await history.undo("editor");
    lines.push(`${value} ${history.depths("editor").join(",")} ${history.undoLabel("editor")},${history.redoLabel("editor")}`);

    // Redo both steps.
    await history.redo("editor");
    await history.redo("editor");
    lines.push(`${value} ${history.depths("editor").join(",")} ${history.undoLabel("editor")},${history.redoLabel("editor")} ${replayOrigins.join(",")}`);
  } catch (error) {
    lines.push(`threw ${error}`);
  }
  shown.set(`done ${lines.join("|")}`);
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn ts_handler_target_applies_undoes_and_redoes_end_to_end_through_history() {
    assert_eq!(
        testing::run(set(), END_TO_END_TSX),
        "done null,null\
         |10 1,0 history.setTen,null\
         |25 2,0 history.agentBatch,null\
         |10 1,1 history.setTen,history.agentBatch\
         |0 0,2 null,history.setTen\
         |25 2,0 history.agentBatch,null HistoryUndo,HistoryUndo,HistoryUndo,HistoryRedo,HistoryRedo,HistoryRedo"
    );
}

const AGENT_DISABLED_TSX: &str = r#"import { mount, signal, Text } from "nsh";
import { HistoryService, OperationService, OperationTarget, RayXAppBuilder } from "test/app";

const shown = signal("waiting");
setTimeout(async () => {
  const lines = [];
  try {
    const services = await new RayXAppBuilder()
      .addHostFakes()
      .addOperations()
      .addHistory((options) => options.recordAgentOperations(false))
      .start();
    const operations = services.getRequiredService(OperationService);
    const history = services.getRequiredService(HistoryService);

    let value = 0;
    const target = OperationTarget.fromHandler((record) => {
      const before = value;
      value = record.payloadJson.value;
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
        after_revision: value,
      };
    });

    await operations.registerTarget("doc", target);
    await history.bindTargetScope("doc", "editor");

    await operations.submitJson("doc", "doc.set", 1, { value: 7 }, { Agent: "assistant-1" }, { label_key: "history.agentIgnored" });
    lines.push(`${value} ${history.canUndo("editor")} ${history.undoLabel("editor")}`);

    await operations.submitJson("doc", "doc.set", 1, { value: 9 }, "LocalUser");
    lines.push(`${value} ${history.canUndo("editor")} ${history.undoLabel("editor")}`);
  } catch (error) {
    lines.push(`threw ${error}`);
  }
  shown.set(`done ${lines.join("|")}`);
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn agent_operations_are_ignored_in_ts_when_record_agent_operations_is_false() {
    assert_eq!(
        testing::run(set(), AGENT_DISABLED_TSX),
        "done 7 false null|9 true doc.set"
    );
}
