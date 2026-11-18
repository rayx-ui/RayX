//! Startup factories made first (`add_startup_resolved_async_first`): every one is made, its
//! future awaited to the end, before any other startup factory runs, so a factory that reads
//! what one made finds it (what a TS implementation of a contract is registered with); when one
//! fails, no other factory runs.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::{Arc, Mutex};
use std::task::Poll;

use futures::FutureExt as _;
use futures::future::{LocalBoxFuture, poll_fn};
use rayx_app::prelude::*;

type Log = Arc<Mutex<Vec<String>>>;

/// A startup service with nothing to initialize.
struct Ready;

impl AsyncInitializable for Ready {
    fn is_initialized(&self) -> bool {
        true
    }

    fn initialize(
        &self,
        _context: StartupInitializationContext,
    ) -> LocalBoxFuture<'static, StartupInitializationResult> {
        futures::future::ready(Ok(())).boxed_local()
    }
}

impl StartupInitializable for Ready {}

/// The startup keys the test registers under.
struct Early;
struct Later;
struct Other;

/// A factory future that pends `turns` times (as a TS object made in JS's next turn does), logs
/// `label` and makes [`Ready`], or fails with `failing`.
fn making(
    log: &Log,
    label: &'static str,
    turns: usize,
    failing: Option<&'static str>,
    service: StartupServiceType,
) -> StartupServiceFuture {
    let log = Arc::clone(log);
    let mut left = turns;
    async move {
        poll_fn(|context| {
            if left == 0 {
                Poll::Ready(())
            } else {
                left -= 1;
                context.waker().wake_by_ref();
                Poll::Pending
            }
        })
        .await;
        log.lock().unwrap().push(format!("made {label}"));
        match failing {
            Some(message) => Err(StartupInitializationFailure {
                service,
                message: message.to_owned(),
                timed_out: false,
            }),
            None => Ok(Arc::new(Ready) as StartupService),
        }
    }
    .boxed_local()
}

/// Runs the provider's startup.
fn start(provider: &ServiceProvider) -> Result<(), String> {
    futures::executor::block_on(
        provider
            .startup_registry()
            .initialize(StartupInitializationContext::new(provider.clone())),
    )
    .map_err(|error| error.to_string())
}

/// A provider with a factory added first (`early`, made after three turns, failing with
/// `failing`) and two added plainly before and after it, each logging when it runs.
fn provider(log: &Log, failing: Option<&'static str>) -> ServiceProvider {
    let mut builder = RayXAppBuilder::new();
    let services = builder.services_mut();
    let later = Arc::clone(log);
    services.add_startup_resolved_async(StartupServiceType::of::<Later>(), move |_| {
        later.lock().unwrap().push("later runs".into());
        making(&later, "later", 0, None, StartupServiceType::of::<Later>())
    });
    let early = Arc::clone(log);
    services.add_startup_resolved_async_first(StartupServiceType::of::<Early>(), move |_| {
        early.lock().unwrap().push("early runs".into());
        making(
            &early,
            "early",
            3,
            failing,
            StartupServiceType::of::<Early>(),
        )
    });
    let other = Arc::clone(log);
    services.add_startup_resolved_async(StartupServiceType::of::<Other>(), move |_| {
        other.lock().unwrap().push("other runs".into());
        making(&other, "other", 1, None, StartupServiceType::of::<Other>())
    });
    builder.build().unwrap()
}

#[test]
fn a_factory_added_first_is_made_before_any_other_runs() {
    let log = Log::default();
    assert_eq!(start(&provider(&log, None)), Ok(()));
    let log = log.lock().unwrap().clone();
    assert_eq!(log[..2], ["early runs", "made early"], "{log:?}");
    // The others run (and are made) together after it.
    let mut rest = log[2..].to_vec();
    rest.sort();
    assert_eq!(
        rest,
        ["later runs", "made later", "made other", "other runs"],
        "{log:?}"
    );
}

#[test]
fn when_a_factory_added_first_fails_no_other_runs() {
    let log = Log::default();
    let started = start(&provider(&log, Some("no early object")));
    assert!(
        started
            .as_ref()
            .is_err_and(|error| error.contains("no early object")),
        "{started:?}"
    );
    assert_eq!(*log.lock().unwrap(), ["early runs", "made early"]);
}
