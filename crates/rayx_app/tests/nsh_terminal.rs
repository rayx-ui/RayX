//! The platform snapshot of an app in NSH's terminal backend (`Idiom::Terminal`, NSH 0.4): a
//! terminal keeps desktop behavior, so RayX reports a desktop's form factor.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::time::Duration;

use crossbeam_channel::{Receiver, Sender};
use nsh_core::platform::{Environment, Idiom, Os};
use nsh_core::{ApproxTextEngine, MountBatch, MountItem, prelude::*};
use nsh_engine::{App, EngineConfig, LocalEngine, MountSink};
use rayx_app::platform_snapshot;

struct ChannelSink(Sender<MountBatch>);

impl MountSink for ChannelSink {
    fn submit(&self, batch: MountBatch) {
        let _ = self.0.send(batch);
    }
}

/// Shows the form factor and whether the snapshot is a desktop's.
#[component]
fn Root() -> impl IntoElement {
    text(move || {
        let snapshot = platform_snapshot();
        format!(
            "form {:?} desktop {}",
            snapshot.form_factor, snapshot.desktop
        )
    })
}

fn shown_on(idiom: Idiom) -> Vec<String> {
    let (tx, rx): (Sender<MountBatch>, Receiver<MountBatch>) = crossbeam_channel::unbounded();
    let mut engine = LocalEngine::start(
        EngineConfig {
            environment: Environment {
                os: Os::Linux,
                idiom,
                ..Environment::default()
            },
            ..EngineConfig::new(App::Rust(nsh_core::element::Root::of::<Root>()))
        },
        ApproxTextEngine,
        ChannelSink(tx),
    )
    .unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    let mut shown = Vec::new();
    while std::time::Instant::now() < deadline
        && !shown.iter().any(|t: &String| t.starts_with("form"))
    {
        engine.pump();
        while let Ok(batch) = rx.recv_timeout(Duration::from_millis(5)) {
            for item in &batch.items {
                if let MountItem::Text { text, .. } = item {
                    shown.push(text.text.clone());
                }
            }
        }
    }
    shown
}

#[test]
fn a_terminal_is_a_desktop() {
    let shown = shown_on(Idiom::Terminal);
    assert!(
        shown.iter().any(|t| t == "form Desktop desktop true"),
        "shown: {shown:?}"
    );
}
