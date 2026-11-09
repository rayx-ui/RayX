//! Ready hooks: they run in the order they were added, with the app's root provider; one whose
//! predicate does not hold is skipped, and the first that fails stops the rest.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::{Arc, Mutex};

use futures::executor::block_on;
use rayx_core::prelude::*;

#[derive(AppService)]
struct Greeting(&'static str);

/// What the hooks did, in order.
type Log = Arc<Mutex<Vec<String>>>;

fn hooks(builder: RayXAppBuilder, log: &Log) -> RayXAppBuilder {
    let first = log.clone();
    let skipped = log.clone();
    let second = log.clone();
    builder
        .add_singleton(Greeting("hello"))
        .on_ready(move |context| {
            let log = first.clone();
            async move {
                let greeting = context.services().required_service::<Greeting>().unwrap();
                log.lock().unwrap().push(format!("first {}", greeting.0));
                Ok(())
            }
        })
        .on_ready_when(
            |services| !services.contains_service::<Greeting>(),
            move |_| {
                let log = skipped.clone();
                async move {
                    log.lock().unwrap().push("skipped".into());
                    Ok(())
                }
            },
        )
        .on_ready_when(
            |services| services.contains_service::<Greeting>(),
            move |_| {
                let log = second.clone();
                async move {
                    log.lock().unwrap().push("second".into());
                    Ok(())
                }
            },
        )
}

fn run(services: &ServiceProvider) -> ReadyResult {
    let hooks = services.required_service::<ReadyHooks>().unwrap();
    block_on(hooks.run(services))
}

#[test]
fn ready_hooks_run_in_order_when_their_predicates_hold() {
    let log = Log::default();
    let services = hooks(RayXAppBuilder::new(), &log).build().unwrap();
    assert_eq!(services.required_service::<ReadyHooks>().unwrap().len(), 3);
    run(&services).unwrap();
    assert_eq!(*log.lock().unwrap(), ["first hello", "second"]);
}

#[test]
fn a_failing_ready_hook_stops_the_rest() {
    let log = Log::default();
    let after = log.clone();
    let services = RayXAppBuilder::new()
        .on_ready(|_| async { Err(ReadyError::new("no network")) })
        .on_ready(move |_| {
            let log = after.clone();
            async move {
                log.lock().unwrap().push("after".into());
                Ok(())
            }
        })
        .build()
        .unwrap();
    assert_eq!(run(&services), Err(ReadyError::new("no network")));
    assert!(log.lock().unwrap().is_empty());
}

#[test]
fn an_app_without_ready_hooks_registers_none() {
    let services = RayXAppBuilder::new().build().unwrap();
    assert!(!services.contains_service::<ReadyHooks>());
}
