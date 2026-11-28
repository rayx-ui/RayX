//! Context and command services for TSX through NSH's engine, headless: declarations,
//! `builder.addCommands()`, `ContextService`, `CommandService` (direct commands, reactive
//! accessors, contextual handlers, `IgnoreWhileRunning`, `shortcutFor`, `resolveActiveBindings`),
//! and custom TS contract implementations.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::Arc;

use nsh_core::prelude::*;
use rayx_app::AppBuilder as _;
use rayx_app::services::commands::{CommandService, ContextService};
use rayx_app::services::input_map::{
    InputMapDocument, InputMapEntry, InputMapService, InputMapSource, InputMapState,
};
use rayx_app::testing::FakeInputMap;
use rayx_app_nsh::testing;
use rayx_app_nsh::ts::{RayXAppBuilder, ServiceProvider};

/// `builder.addFakeInputMap()`: registers a fake `dyn InputMapService` with sample bindings.
struct FakeInputMapBuilder;

#[class_extension(RayXAppBuilder)]
impl FakeInputMapBuilder {
    /// Registers a fake input map with sample keyboard bindings for testing.
    pub fn add_fake_input_map(
        this: Instance<RayXAppBuilder>,
    ) -> Result<Instance<RayXAppBuilder>, String> {
        RayXAppBuilder::extend_with(this, |mut builder| {
            let fake = FakeInputMap::new(InputMapState {
                application: InputMapDocument::new(
                    InputMapSource::Application,
                    0,
                    vec![
                        InputMapEntry::keyboard("", "mod-z", "edit.undo", None),
                        InputMapEntry::keyboard("canvas", "backspace", "canvas.delete", None),
                        InputMapEntry::keyboard("editor", "mod-s", "file.save", None),
                    ],
                ),
                ..InputMapState::default()
            });
            builder
                .services_mut()
                .replace_service::<dyn InputMapService>(fake as Arc<dyn InputMapService>);
            builder
        })
    }
}

/// Rust probe for verifying TS implementations of `ContextServiceContract` and
/// `CommandServiceContract`.
#[module]
struct Probe;

#[module]
impl Probe {
    /// Reads active contexts from Rust `dyn ContextService`.
    pub(crate) fn active_contexts(services: Instance<ServiceProvider>) -> Vec<String> {
        services
            .with(|s| s.services().required_service::<dyn ContextService>())
            .unwrap()
            .active_contexts()
    }

    /// Executes `id` from Rust `dyn CommandService`.
    pub(crate) async fn execute(
        services: Instance<ServiceProvider>,
        id: String,
    ) -> Result<bool, String> {
        let cmds = services
            .with(|s| s.services().required_service::<dyn CommandService>())
            .unwrap();
        cmds.execute(id, None)
            .await
            .map_err(|error| error.to_string())
    }
}

fn set() -> nsh_core::element::Registry {
    testing::set(|set| {
        rayx_services_commands::register(set);
        set.class_extension::<FakeInputMapBuilder>()
            .module::<Probe>();
    })
}

fn class<'a>(ts: &'a str, name: &str) -> &'a str {
    let head = format!("export declare class {name} {{");
    ts.split(head.as_str())
        .nth(1)
        .and_then(|rest| rest.split("\n}\n").next())
        .unwrap_or_else(|| panic!("no class {name} in\n{ts}"))
}

#[test]
fn declarations_include_context_and_command_classes_aliases_and_docs() {
    let set = testing::set(rayx_services_commands::register);
    let ts = testing::declarations(&set);

    let ctx_class = class(&ts, "ContextService");
    assert!(ctx_class.contains("private constructor();"), "{ctx_class}");
    for member in [
        "get contract(): ContextServiceContract | null;",
        "state(): ContextState;",
        "activeContexts(): (string)[];",
        "hasContext(context: string): boolean;",
        "enter(context: string): () => void;",
        "setContexts(contexts: (string)[]): void;",
        "onChange(",
    ] {
        assert!(
            ctx_class.contains(&format!("  {member}")),
            "ContextService missing `{member}`:\n{ctx_class}"
        );
    }

    let cmd_class = class(&ts, "CommandService");
    assert!(cmd_class.contains("private constructor();"), "{cmd_class}");
    for member in [
        "get contract(): CommandServiceContract | null;",
        "state(): CommandState;",
        "register(descriptor: CommandDescriptor): () => void;",
        "registerHandler(id: string, context: string, handler: ContextCommandHandler): () => void;",
        "execute(id: string, args?: unknown): Promise<boolean>;",
        "canExecute(id: string): boolean;",
        "isExecuting(id: string): boolean;",
        "title(id: string): string | null;",
        "checked(id: string): boolean | null;",
        "all(): CommandMetadata[];",
        "has(id: string): boolean;",
        "shortcutFor(id: string): string | null;",
        "resolveActiveBindings(): ActiveKeyBinding[];",
        "onChange(",
        "onEvent(",
    ] {
        assert!(
            cmd_class.contains(&format!("  {member}")),
            "CommandService missing `{member}`:\n{cmd_class}"
        );
    }

    for alias in [
        "ContextState",
        "CommandMetadata",
        "CommandState",
        "CommandEvent",
        "CommandDescriptor",
        "ContextCommandHandler",
        "ActiveKeyBinding",
    ] {
        assert!(
            ts.contains(&format!("type {alias} =")),
            "missing alias {alias} in\n{ts}"
        );
    }

    assert!(ts.contains("ContextServiceContract"), "{ts}");
    assert!(ts.contains("CommandServiceContract"), "{ts}");
    assert!(ts.contains("addCommands(): RayXAppBuilder;"), "{ts}");
    assert_eq!(set.undocumented(), Vec::<String>::new());
}

const TSX_COMMANDS: &str = r#"import { mount, signal, Text } from "nsh";
import { CommandService, ContextService, Probe, RayXAppBuilder } from "test/app";

const shown = signal("waiting");
setTimeout(async () => {
  const lines = [];
  try {
    const services = await new RayXAppBuilder()
      .addHostFakes()
      .addFakeInputMap()
      .addCommands()
      .start();

    const ctx = services.getRequiredService(ContextService);
    const cmds = services.getRequiredService(CommandService);

    const events = [];
    cmds.onEvent((ev) => events.push(Object.keys(ev)[0]));

    // Context enter/leave & setContexts
    ctx.setContexts(["editor"]);
    const leaveCanvas = ctx.enter("canvas");
    lines.push(ctx.activeContexts().join(","));
    lines.push(String(ctx.hasContext("canvas")) + ":" + String(ctx.hasContext("")));

    // Reactive command with signal-backed title & canExecute & checked
    const enabled = signal(true);
    const count = signal(0);
    const unregDirect = cmds.register({
      id: "file.save",
      title: () => `Save (${count()})`,
      category: "File",
      canExecute: () => enabled(),
      checked: () => count() > 0,
      run: async (arg) => {
        count.set(count() + (typeof arg === "number" ? arg : 1));
      },
    });
    lines.push(cmds.title("file.save") + "|" + cmds.canExecute("file.save") + "|" + cmds.checked("file.save"));
    const ranSave = await cmds.execute("file.save", 2);
    lines.push(ranSave + ":" + cmds.title("file.save") + ":" + cmds.checked("file.save"));

    // Disable reactively
    enabled.set(false);
    lines.push(String(cmds.canExecute("file.save")) + ":" + String(await cmds.execute("file.save")));

    // Contextual handlers (edit.undo)
    const log = [];
    cmds.registerHandler("edit.undo", "", {
      title: "Undo Global",
      run: () => { log.push("global"); },
    });
    cmds.registerHandler("edit.undo", "canvas", {
      title: "Undo Canvas",
      run: () => { log.push("canvas"); },
    });
    await cmds.execute("edit.undo");
    // Also callable from Rust through dyn CommandService!
    await Probe.execute(services, "edit.undo");
    leaveCanvas();
    await cmds.execute("edit.undo");
    lines.push(log.join(","));

    // IgnoreWhileRunning on async command
    let release;
    let markStarted;
    const started = new Promise((resolve) => { markStarted = resolve; });
    cmds.register({
      id: "slow.op",
      title: "Slow",
      run: () => new Promise((resolve) => { release = resolve; markStarted(); }),
    });
    const first = cmds.execute("slow.op");
    await started;
    const executingDuring = cmds.isExecuting("slow.op");
    let overlapRejected = false;
    try {
      await cmds.execute("slow.op");
    } catch (err) {
      overlapRejected = String(err).includes("already executing");
    }
    release();
    await first;
    lines.push(executingDuring + ":" + overlapRejected + ":" + cmds.isExecuting("slow.op"));

    // Shortcuts & active bindings from InputMapService
    const undoShortcut = cmds.shortcutFor("edit.undo");
    const saveShortcut = cmds.shortcutFor("file.save");
    const bindings = cmds.resolveActiveBindings().map((b) => `${b.keystroke}=${b.action}`).sort().join(",");
    lines.push(`${undoShortcut};${saveShortcut};${bindings}`);

    unregDirect();
    lines.push(String(cmds.has("file.save")));
    await new Promise((resolve) => setTimeout(resolve, 0));
    lines.push("events:" + events.length);
  } catch (error) {
    lines.push(`threw ${error}`);
  }
  shown.set(`done ${lines.join("||")}`);
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn tsx_context_and_command_services_work_end_to_end() {
    let shown = testing::run(set(), TSX_COMMANDS);
    let parts: Vec<&str> = shown.split("||").collect();
    assert_eq!(parts[0], "done canvas,editor", "{shown}");
    assert_eq!(parts[1], "true:true", "{shown}");
    assert_eq!(parts[2], "Save (0)|true|false", "{shown}");
    assert_eq!(parts[3], "true:Save (2):true", "{shown}");
    assert_eq!(parts[4], "false:false", "{shown}");
    assert_eq!(parts[5], "canvas,canvas,global", "{shown}");
    assert_eq!(parts[6], "true:true:false", "{shown}");
    assert_eq!(
        parts[7], "mod+z;mod+s;mod+s=file.save,mod+z=edit.undo",
        "{shown}"
    );
    assert_eq!(parts[8], "false", "{shown}");
    assert!(parts[9].starts_with("events:"), "{shown}");
    assert_eq!(parts.len(), 10, "{shown}");
}

const TSX_CUSTOM_CONTRACT: &str = r#"import { mount, signal, Text } from "nsh";
import {
  CommandService,
  ContextService,
  ContractEvents,
  ContractState,
  Probe,
  RayXAppBuilder,
} from "test/app";

class CustomContext {
  context = new ContractState({ contexts: ["custom"], revision: 1 });
  async pushContext(ctx) {
    this.context.publish({ contexts: [ctx, ...this.context.value.contexts], revision: this.context.value.revision + 1 });
    return 1;
  }
  async popContext(_token) { return true; }
  async setContexts(ctxs) {
    this.context.publish({ contexts: ctxs, revision: this.context.value.revision + 1 });
  }
}

class CustomCommands {
  commands = new ContractState({
    commands: [{ id: "demo.run", title: "Demo", contexts: [], can_execute: true, is_executing: false }],
    revision: 1,
  });
  events = new ContractEvents();
  ran = [];
  async execute(id, args) {
    this.ran.push(id);
    this.events.publish({ Executed: { id } });
    return true;
  }
}

const shown = signal("waiting");
setTimeout(async () => {
  try {
    let customCmds;
    const services = await new RayXAppBuilder()
      .addHostFakes()
      .addSingletonAs(ContextService, () => new CustomContext())
      .addSingletonAs(CommandService, () => (customCmds = new CustomCommands()))
      .start();
    const ctxs = Probe.activeContexts(services);
    const ran = await Probe.execute(services, "demo.run");
    shown.set(`done ${ctxs.join(",")}|${ran}|${customCmds.ran.join(",")}`);
  } catch (error) {
    shown.set(`done threw ${error}`);
  }
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn rust_calls_custom_ts_context_and_command_contracts() {
    let shown = testing::run(set(), TSX_CUSTOM_CONTRACT);
    assert_eq!(shown, "done custom|true|demo.run", "{shown}");
}
