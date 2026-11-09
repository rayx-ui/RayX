//! Ready hooks on NSH: they run on the engine's thread once the startup services initialized,
//! before the app counts as ready, so they reach NSH's functions; one that fails fails startup.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::{
    Mutex, MutexGuard,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;

use crossbeam_channel::{Receiver, Sender};
use nsh_core::{ApproxTextEngine, MountBatch, MountItem, prelude::*};
use nsh_engine::{App, EngineConfig, LocalEngine, MountSink};
use rayx_app::{AppState, RayXAppBuilder, ReadyAppBuilderExt as _, ReadyError, app_state, set_app};

/// What the hook saw: NSH's locale, and whether the app was ready already.
static SEEN: Mutex<Option<(String, bool)>> = Mutex::new(None);
static FAIL: AtomicBool = AtomicBool::new(false);

fn build_app(builder: RayXAppBuilder) -> RayXAppBuilder {
    builder.on_ready(|_| async {
        if FAIL.load(Ordering::SeqCst) {
            return Err(ReadyError::new("the catalog is unreachable"));
        }
        // NSH's functions work: the hook runs on the engine's thread.
        let ready = app_state() == AppState::Ready;
        *SEEN.lock().unwrap() = Some((untracked(locale).to_string(), ready));
        Ok(())
    })
}

#[component]
fn Root() -> impl IntoElement {
    rayx_app::start();
    dynamic(move || match app_state() {
        AppState::Ready => text("ready").into_element(),
        AppState::Failed(message) => text(format!("failed: {message}")).into_element(),
        _ => text("starting").into_element(),
    })
}

struct ChannelSink(Sender<MountBatch>);

impl MountSink for ChannelSink {
    fn submit(&self, batch: MountBatch) {
        let _ = self.0.send(batch);
    }
}

fn serial() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Starts the app and pumps until a text starting with `prefix` shows; returns it.
fn shown(prefix: &str) -> String {
    set_app(build_app);
    let (tx, rx): (Sender<MountBatch>, Receiver<MountBatch>) = crossbeam_channel::unbounded();
    let mut engine = LocalEngine::start(
        EngineConfig::new(App::Rust(nsh_core::element::Root::of::<Root>())),
        ApproxTextEngine,
        ChannelSink(tx),
    )
    .unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while std::time::Instant::now() < deadline {
        engine.pump();
        while let Ok(batch) = rx.recv_timeout(Duration::from_millis(5)) {
            for item in &batch.items {
                if let MountItem::Text { text, .. } = item
                    && text.text.starts_with(prefix)
                {
                    return text.text.to_string();
                }
            }
        }
    }
    panic!("nothing starting with `{prefix}` showed");
}

#[test]
fn ready_hooks_run_on_the_engine_thread_before_the_app_is_ready() {
    let _serial = serial();
    FAIL.store(false, Ordering::SeqCst);
    assert_eq!(shown("ready"), "ready");
    assert_eq!(
        SEEN.lock().unwrap().clone(),
        Some(("en-US".to_owned(), false))
    );
}

#[test]
fn a_failing_ready_hook_fails_startup() {
    let _serial = serial();
    FAIL.store(true, Ordering::SeqCst);
    assert_eq!(
        shown("failed"),
        "failed: a ready hook failed: the catalog is unreachable"
    );
}
