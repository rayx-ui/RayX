//! The platform snapshot of an app on Tizen, Samsung's TV OS, run natively (`Os::Tizen`, NSH 0.5):
//! RayX reports the Tizen family and a TV's form factor, never a desktop.

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

/// Shows the family, the form factor and whether the snapshot is a desktop's.
#[component]
fn Root() -> impl IntoElement {
    text(move || {
        let snapshot = platform_snapshot();
        format!(
            "family {:?} form {:?} desktop {}",
            snapshot.family, snapshot.form_factor, snapshot.desktop
        )
    })
}

fn shown_on(os: Os, idiom: Idiom) -> Vec<String> {
    let (tx, rx): (Sender<MountBatch>, Receiver<MountBatch>) = crossbeam_channel::unbounded();
    let mut engine = LocalEngine::start(
        EngineConfig {
            environment: Environment {
                os,
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
        && !shown.iter().any(|t: &String| t.starts_with("family"))
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
fn a_tizen_tv_is_the_tizen_family_on_a_tv() {
    let shown = shown_on(Os::Tizen, Idiom::Tv);
    assert!(
        shown
            .iter()
            .any(|t| t == "family Tizen form Tv desktop false"),
        "shown: {shown:?}"
    );
}
