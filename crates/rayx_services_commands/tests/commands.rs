//! Context and command services: active context stacks, direct commands, contextual handler
//! dispatch, `IgnoreWhileRunning` concurrency guards, cancellation safety, and input-map
//! keybinding resolution.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use futures::channel::oneshot;
use futures::executor::block_on;
use futures::task::{Context, Poll, noop_waker};
use rayx_app::services::commands::{CommandError, CommandEvent, CommandService, ContextService};
use rayx_app::services::input_map::{
    InputMapDocument, InputMapEntry, InputMapService, InputMapSource, InputMapState, InputWhen,
};
use rayx_app::testing::FakeInputMap;
use rayx_app::{AppBuilder as _, RayXAppBuilder};
use rayx_services_commands::{
    CommandDefinition, CommandsAppBuilderExt as _, ContextCommandHandlerDefinition, RayxCommands,
    RayxContext, input_sequence_to_nsh_keystroke, resolve_active_bindings, shortcut_for,
};

fn build_standalone() -> (Arc<RayxContext>, Arc<RayxCommands>) {
    let services = RayXAppBuilder::new().add_commands().build().unwrap();
    (
        services.required_service::<RayxContext>().unwrap(),
        services.required_service::<RayxCommands>().unwrap(),
    )
}

#[test]
fn context_stack_orders_innermost_first_deduplicates_and_supports_raii_guards() {
    let (ctx, _) = build_standalone();
    let dyn_ctx: Arc<dyn ContextService> = ctx.clone();

    assert!(dyn_ctx.active_contexts().is_empty());
    assert!(dyn_ctx.has_context(""));
    assert!(!dyn_ctx.has_context("editor"));

    let heard = Arc::new(Mutex::new(Vec::<Vec<String>>::new()));
    let sink = Arc::clone(&heard);
    let sub = dyn_ctx.subscribe(move |contexts| {
        sink.lock().unwrap().push(contexts.to_vec());
    });

    // Blank context is rejected.
    assert!(matches!(
        ctx.push_sync("   "),
        Err(CommandError::InvalidArgument(_))
    ));

    // Declarative base contexts.
    block_on(dyn_ctx.set_contexts(vec![
        "editor".into(),
        "".into(),
        "document".into(),
        "editor".into(),
    ]))
    .unwrap();
    assert_eq!(dyn_ctx.active_contexts(), vec!["editor", "document"]);
    assert!(dyn_ctx.has_context("editor"));
    assert!(dyn_ctx.has_context("document"));
    assert!(dyn_ctx.has_context(""));

    // Pushed contexts take precedence over base contexts (newest innermost).
    let g1 = ctx.enter("modal").unwrap();
    assert_eq!(
        dyn_ctx.active_contexts(),
        vec!["modal", "editor", "document"]
    );

    // Pushing a context already in base promotes it to innermost without duplication.
    let g2 = ctx.enter("document").unwrap();
    assert_eq!(
        dyn_ctx.active_contexts(),
        vec!["document", "modal", "editor"]
    );

    // Dropping out of order removes only its own entry.
    drop(g1);
    assert_eq!(dyn_ctx.active_contexts(), vec!["document", "editor"]);

    drop(g2);
    assert_eq!(dyn_ctx.active_contexts(), vec!["editor", "document"]);

    // Detached guard stays pushed until popped explicitly.
    let g3 = ctx.enter("palette").unwrap();
    let token = g3.token().unwrap();
    g3.detach();
    assert_eq!(
        dyn_ctx.active_contexts(),
        vec!["palette", "editor", "document"]
    );
    assert!(block_on(dyn_ctx.pop_context(token)).unwrap());
    assert!(!block_on(dyn_ctx.pop_context(token)).unwrap());
    assert_eq!(dyn_ctx.active_contexts(), vec!["editor", "document"]);

    drop(sub);
    ctx.set_contexts_sync(vec!["other".into()]);
    assert_eq!(heard.lock().unwrap().len(), 7);
}

#[test]
fn direct_command_executes_checks_predicates_emits_events_and_unregisters() {
    let (ctx, cmds) = build_standalone();
    let dyn_cmds: Arc<dyn CommandService> = cmds.clone();

    let events = Arc::new(Mutex::new(Vec::<CommandEvent>::new()));
    let sink = Arc::clone(&events);
    let _ev_sub = dyn_cmds.subscribe_events(move |ev| sink.lock().unwrap().push(ev.clone()));

    let enabled = Arc::new(AtomicBool::new(true));
    let is_checked = Arc::new(AtomicBool::new(false));
    let received_arg = Arc::new(Mutex::new(None::<serde_json::Value>));

    let enabled_c = Arc::clone(&enabled);
    let checked_c = Arc::clone(&is_checked);
    let arg_c = Arc::clone(&received_arg);

    let reg = cmds
        .register(
            CommandDefinition::sync("view.grid", "Toggle Grid", move |args| {
                *arg_c.lock().unwrap() = args;
                let prev = checked_c.load(Ordering::SeqCst);
                checked_c.store(!prev, Ordering::SeqCst);
                Ok(())
            })
            .with_category("View")
            .with_kind("action")
            .with_contexts(vec!["workspace".into()])
            .with_can_execute(move || enabled_c.load(Ordering::SeqCst))
            .with_checked({
                let checked = Arc::clone(&is_checked);
                move || Some(checked.load(Ordering::SeqCst))
            }),
        )
        .unwrap();

    // Out of context initially -> cannot execute, returns Ok(false).
    assert!(dyn_cmds.has("view.grid"));
    assert!(!dyn_cmds.can_execute("view.grid"));
    assert!(!block_on(dyn_cmds.execute("view.grid".into(), None)).unwrap());

    // Enter workspace context -> can execute.
    let _ws = ctx.enter("workspace").unwrap();
    assert!(dyn_cmds.can_execute("view.grid"));
    assert_eq!(dyn_cmds.checked("view.grid"), Some(false));
    let meta = dyn_cmds.get("view.grid").unwrap();
    assert_eq!(meta.title, "Toggle Grid");
    assert_eq!(meta.category.as_deref(), Some("View"));
    assert_eq!(meta.kind.as_deref(), Some("action"));
    assert_eq!(meta.contexts, vec!["workspace"]);

    // Execute with JSON arguments.
    let ran =
        block_on(dyn_cmds.execute("view.grid".into(), Some(serde_json::json!({ "snap": 8 }))))
            .unwrap();
    assert!(ran);
    assert_eq!(
        *received_arg.lock().unwrap(),
        Some(serde_json::json!({ "snap": 8 }))
    );
    assert_eq!(dyn_cmds.checked("view.grid"), Some(true));

    // Disable via predicate -> Ok(false).
    enabled.store(false, Ordering::SeqCst);
    cmds.refresh();
    assert!(!dyn_cmds.can_execute("view.grid"));
    assert!(!block_on(dyn_cmds.execute("view.grid".into(), None)).unwrap());

    // Unknown command -> Err(UnknownCommand).
    assert_eq!(
        block_on(dyn_cmds.execute("missing.cmd".into(), None)),
        Err(CommandError::UnknownCommand("missing.cmd".into()))
    );

    // Unregister removes it and emits Unregistered.
    reg.unregister();
    assert!(!dyn_cmds.has("view.grid"));

    let recorded = events.lock().unwrap().clone();
    assert_eq!(
        recorded,
        vec![
            CommandEvent::Registered("view.grid".into()),
            CommandEvent::Executed {
                id: "view.grid".into(),
            },
            CommandEvent::Unregistered("view.grid".into()),
        ]
    );
}

#[test]
fn contextual_handlers_dispatch_innermost_first_with_global_fallback() {
    let (ctx, cmds) = build_standalone();
    let dyn_cmds: Arc<dyn CommandService> = cmds.clone();

    let log = Arc::new(Mutex::new(Vec::<&'static str>::new()));

    let log_global = Arc::clone(&log);
    let _h_global = cmds
        .register_handler(
            "edit.undo",
            "",
            ContextCommandHandlerDefinition::sync(move |_| {
                log_global.lock().unwrap().push("global");
                Ok(())
            })
            .with_title("Undo"),
        )
        .unwrap();

    let log_canvas = Arc::clone(&log);
    let _h_canvas = cmds
        .register_handler(
            "edit.undo",
            "canvas",
            ContextCommandHandlerDefinition::sync(move |_| {
                log_canvas.lock().unwrap().push("canvas");
                Ok(())
            })
            .with_title("Undo Canvas"),
        )
        .unwrap();

    let log_text = Arc::clone(&log);
    let _h_text = cmds
        .register_handler(
            "edit.undo",
            "text-editor",
            ContextCommandHandlerDefinition::sync(move |_| {
                log_text.lock().unwrap().push("text-editor");
                Ok(())
            })
            .with_title("Undo Typing"),
        )
        .unwrap();

    // Global fallback when no context active.
    assert_eq!(dyn_cmds.title("edit.undo").as_deref(), Some("Undo"));
    assert!(block_on(dyn_cmds.execute("edit.undo".into(), None)).unwrap());

    // Enter canvas -> canvas handler wins.
    let _g_canvas = ctx.enter("canvas").unwrap();
    assert_eq!(dyn_cmds.title("edit.undo").as_deref(), Some("Undo Canvas"));
    assert!(block_on(dyn_cmds.execute("edit.undo".into(), None)).unwrap());

    // Enter text-editor inside canvas -> text-editor handler wins.
    let g_text = ctx.enter("text-editor").unwrap();
    assert_eq!(dyn_cmds.title("edit.undo").as_deref(), Some("Undo Typing"));
    assert!(block_on(dyn_cmds.execute("edit.undo".into(), None)).unwrap());

    // Leave text-editor -> back to canvas.
    drop(g_text);
    assert_eq!(dyn_cmds.title("edit.undo").as_deref(), Some("Undo Canvas"));
    assert!(block_on(dyn_cmds.execute("edit.undo".into(), None)).unwrap());

    assert_eq!(
        *log.lock().unwrap(),
        vec!["global", "canvas", "text-editor", "canvas"]
    );
}

#[test]
fn stale_registration_drop_does_not_remove_newer_override() {
    let (_, cmds) = build_standalone();
    let dyn_cmds: Arc<dyn CommandService> = cmds.clone();
    let calls = Arc::new(AtomicUsize::new(0));

    let reg_old = cmds
        .register(CommandDefinition::sync("app.ping", "Old", |_| Ok(())))
        .unwrap();

    let calls_new = Arc::clone(&calls);
    let reg_new = cmds
        .register(CommandDefinition::sync("app.ping", "New", move |_| {
            calls_new.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }))
        .unwrap();

    assert_eq!(dyn_cmds.title("app.ping").as_deref(), Some("New"));
    drop(reg_old);
    assert!(dyn_cmds.has("app.ping"));
    assert_eq!(dyn_cmds.title("app.ping").as_deref(), Some("New"));
    assert!(block_on(dyn_cmds.execute("app.ping".into(), None)).unwrap());
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    drop(reg_new);
    assert!(!dyn_cmds.has("app.ping"));
}

#[test]
fn ignore_while_running_rejects_overlap_and_recovers_on_cancel_or_error() {
    let (_, cmds) = build_standalone();
    let dyn_cmds: Arc<dyn CommandService> = cmds.clone();

    let gate = Arc::new(Mutex::new(None::<oneshot::Sender<Result<(), String>>>));
    let gate_clone = Arc::clone(&gate);

    let _reg = cmds
        .register(CommandDefinition::new("file.save", "Save", move |_| {
            let (tx, rx) = oneshot::channel();
            *gate_clone.lock().unwrap() = Some(tx);
            Box::pin(async move { rx.await.unwrap_or_else(|_| Err("dropped".into())) })
        }))
        .unwrap();

    // Start first execution and poll it once so it enters `executing`.
    let waker = noop_waker();
    let mut cx = Context::from_waker(&waker);
    let mut first = dyn_cmds.execute("file.save".into(), None);
    assert!(matches!(first.as_mut().poll(&mut cx), Poll::Pending));
    assert!(dyn_cmds.is_executing("file.save"));

    // Overlapping second execution fails with AlreadyExecuting.
    let second = block_on(dyn_cmds.execute("file.save".into(), None));
    assert_eq!(
        second,
        Err(CommandError::AlreadyExecuting("file.save".into()))
    );

    // Dropping the in-flight future clears `is_executing` immediately.
    drop(first);
    assert!(!dyn_cmds.is_executing("file.save"));

    // Next execution can run and report handler errors cleanly.
    let mut third = dyn_cmds.execute("file.save".into(), None);
    assert!(matches!(third.as_mut().poll(&mut cx), Poll::Pending));
    let tx = gate.lock().unwrap().take().unwrap();
    tx.send(Err("disk full".into())).unwrap();
    let result = block_on(third);
    assert_eq!(
        result,
        Err(CommandError::ExecutionFailed {
            id: "file.save".into(),
            message: "disk full".into(),
        })
    );
    assert!(!dyn_cmds.is_executing("file.save"));
}

#[test]
fn keystroke_conversion_and_active_binding_resolution_follow_layers_and_contexts() {
    assert_eq!(
        input_sequence_to_nsh_keystroke("mod-z").as_deref(),
        Some("mod+z")
    );
    assert_eq!(
        input_sequence_to_nsh_keystroke("cmd-shift-P").as_deref(),
        Some("shift+cmd+p")
    );
    assert_eq!(
        input_sequence_to_nsh_keystroke("shift-cmd-p").as_deref(),
        Some("shift+cmd+p")
    );
    assert_eq!(
        input_sequence_to_nsh_keystroke("ctrl--").as_deref(),
        Some("ctrl+-")
    );
    assert_eq!(
        input_sequence_to_nsh_keystroke("mod-+").as_deref(),
        Some("mod+plus")
    );
    assert_eq!(
        input_sequence_to_nsh_keystroke("mod+shift+z").as_deref(),
        Some("mod+shift+z")
    );
    assert_eq!(
        input_sequence_to_nsh_keystroke("esc").as_deref(),
        Some("escape")
    );
    assert_eq!(input_sequence_to_nsh_keystroke("ctrl-k ctrl-c"), None);
    assert_eq!(input_sequence_to_nsh_keystroke("fn-f1"), None);

    let mut unbind_backspace = InputMapEntry::keyboard("", "backspace", "edit.delete", None);
    unbind_backspace.unbind = true;

    let state = InputMapState {
        framework: InputMapDocument::new(
            InputMapSource::Framework,
            0,
            vec![
                InputMapEntry::keyboard("", "mod-z", "edit.undo", None),
                InputMapEntry::keyboard("", "backspace", "edit.delete", None),
            ],
        ),
        application: InputMapDocument::new(
            InputMapSource::Application,
            0,
            vec![
                InputMapEntry::keyboard("canvas", "backspace", "canvas.delete", None)
                    .with_when(InputWhen::NotEditing)
                    .with_repeat(true),
                InputMapEntry::keyboard("canvas", "mod-shift-z", "edit.redo", None)
                    .with_pass_through(true),
            ],
        ),
        user: InputMapDocument::new(
            InputMapSource::User,
            1,
            vec![
                // User unbinds global backspace.
                unbind_backspace,
                // User rebinds undo in canvas to mod-alt-z.
                InputMapEntry::keyboard("canvas", "mod-alt-z", "edit.undo", None),
            ],
        ),
        revision: 1,
    };

    // With no active context: global undo is mod+z, global backspace was unbound by user.
    let global_bindings = resolve_active_bindings(&[], &state);
    assert_eq!(global_bindings.len(), 1);
    assert_eq!(global_bindings[0].keystroke, "mod+z");
    assert_eq!(global_bindings[0].action, "edit.undo");
    assert_eq!(
        shortcut_for("edit.undo", &[], &state).as_deref(),
        Some("mod+z")
    );
    assert_eq!(
        shortcut_for("canvas.delete", &[], &state).as_deref(),
        Some("backspace")
    );

    // With canvas active: canvas bindings win, and shortcut_for("edit.undo") prefers canvas's mod+alt+z.
    let active = vec!["canvas".to_owned()];
    let canvas_bindings = resolve_active_bindings(&active, &state);
    assert_eq!(
        shortcut_for("edit.undo", &active, &state).as_deref(),
        Some("mod+alt+z")
    );
    assert!(canvas_bindings.iter().any(|b| {
        b.keystroke == "backspace"
            && b.action == "canvas.delete"
            && b.when.as_deref() == Some("notEditing")
            && b.repeat
    }));
    assert!(
        canvas_bindings
            .iter()
            .any(|b| { b.keystroke == "mod+shift+z" && b.action == "edit.redo" && b.pass_through })
    );
}

#[test]
fn add_commands_observes_input_map_registered_before_or_after() {
    let fake_input = FakeInputMap::new(InputMapState {
        application: InputMapDocument::new(
            InputMapSource::Application,
            0,
            vec![InputMapEntry::keyboard("", "mod-s", "file.save", None)],
        ),
        ..InputMapState::default()
    });

    // InputMap registered AFTER add_commands().
    let mut builder = RayXAppBuilder::new().add_commands();
    builder
        .services_mut()
        .replace_service::<dyn InputMapService>(fake_input as Arc<dyn InputMapService>);
    let services = builder.build().unwrap();

    let cmds = services.required_service::<RayxCommands>().unwrap();
    assert_eq!(cmds.shortcut_for("file.save").as_deref(), Some("mod+s"));
    assert_eq!(cmds.resolve_active_bindings().len(), 1);
}
