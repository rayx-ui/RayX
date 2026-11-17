//! What an app's registrations require, checked by `build()` and `build_async()` before anything
//! is made: required and declared traits that are missing, several at once in one error, a
//! singleton depending on a scoped service; and an app with everything registered building as
//! before.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use futures::FutureExt as _;
use futures::future::LocalBoxFuture;
use rayx_app::prelude::*;

/// A service the app's host requires.
trait Platform: Send + Sync {
    fn name(&self) -> String;
}

/// A service that depends on `Platform`.
trait Fonts: Send + Sync {
    fn family(&self) -> String;
}

/// A scoped service.
trait Session: Send + Sync {
    fn id(&self) -> u32;
}

struct Desktop;

impl Platform for Desktop {
    fn name(&self) -> String {
        "desktop".into()
    }
}

struct Inter(Arc<dyn Platform>);

impl Fonts for Inter {
    fn family(&self) -> String {
        format!("Inter on {}", self.0.name())
    }
}

struct Visit(u32);

impl Session for Visit {
    fn id(&self) -> u32 {
        self.0
    }
}

const PLATFORM_HINT: &str = "add_platform() from rayx_services_platform, or register your own";
const FONTS_HINT: &str = "add_fonts() from rayx_services_fonts";

/// Registers the fonts, which resolve the platform, and declares that.
fn add_fonts(builder: &mut RayXAppBuilder) {
    builder
        .services
        .add_singleton_as::<dyn Fonts>(Arc::new(Inter(Arc::new(Desktop))) as Arc<dyn Fonts>);
    builder
        .services
        .require_for::<dyn Fonts, dyn Platform>(PLATFORM_HINT);
}

fn names(error: &AppBuildError) -> String {
    error.to_string()
}

#[test]
fn a_missing_required_trait_fails_the_build_with_its_hint() {
    let mut builder = RayXAppBuilder::new();
    builder.services.require::<dyn Platform>(PLATFORM_HINT);
    let error = builder.build().unwrap_err();
    let AppBuildError::Services(services) = &error else {
        panic!("{error}");
    };
    assert_eq!(
        services.problems,
        [ServiceValidationProblem::Missing {
            needed: "dyn di_validation::Platform".into(),
            by: None,
            hint: PLATFORM_HINT.into(),
        }]
    );
    assert_eq!(
        names(&error),
        format!(
            "the app's services are not complete:\n- the app needs `dyn di_validation::Platform`, \
             which is not registered: {PLATFORM_HINT}"
        )
    );
}

#[test]
fn a_missing_declared_dependency_names_who_needs_it() {
    let mut builder = RayXAppBuilder::new();
    add_fonts(&mut builder);
    let error = builder.build().unwrap_err();
    assert_eq!(
        names(&error),
        format!(
            "the app's services are not complete:\n- `dyn di_validation::Fonts` needs \
             `dyn di_validation::Platform`, which is not registered: {PLATFORM_HINT}"
        )
    );
}

#[test]
fn every_problem_is_listed_at_once_one_per_line() {
    let mut builder = RayXAppBuilder::new();
    builder.services.require::<dyn Platform>(PLATFORM_HINT);
    builder.services.require::<dyn Fonts>(FONTS_HINT);
    builder
        .services
        .require_for::<dyn Fonts, dyn Session>("add_session()");
    let error = builder.build().unwrap_err();
    let message = names(&error);
    let lines: Vec<&str> = message.lines().collect();
    assert_eq!(lines.len(), 4, "{message}");
    assert!(lines[1].contains("`dyn di_validation::Platform`") && lines[1].contains(PLATFORM_HINT));
    assert!(lines[2].contains("`dyn di_validation::Fonts`") && lines[2].contains(FONTS_HINT));
    assert!(
        lines[3].starts_with("- `dyn di_validation::Fonts` needs `dyn di_validation::Session`")
    );
    // The same requirement twice is one line.
    let mut twice = RayXAppBuilder::new();
    twice.services.require::<dyn Platform>(PLATFORM_HINT);
    twice.services.require::<dyn Platform>(PLATFORM_HINT);
    assert_eq!(names(&twice.build().unwrap_err()).lines().count(), 2);
}

#[test]
fn a_singleton_on_a_scoped_service_fails_naming_both() {
    let mut builder = RayXAppBuilder::new();
    builder
        .services
        .add_scoped_as::<dyn Session>(|_| Arc::new(Visit(1)));
    builder
        .services
        .add_singleton_as::<dyn Fonts>(Arc::new(Inter(Arc::new(Desktop))) as Arc<dyn Fonts>);
    builder
        .services
        .require_for::<dyn Fonts, dyn Session>("register the session");
    let error = builder.build().unwrap_err();
    let AppBuildError::Services(services) = &error else {
        panic!("{error}");
    };
    assert!(matches!(
        &services.problems[..],
        [ServiceValidationProblem::SingletonOnScoped { singleton, through, scoped, .. }]
            if singleton == "dyn di_validation::Fonts"
                && through.is_empty()
                && scoped == "dyn di_validation::Session"
    ));
    // A scoped service on a scoped one is fine.
    let mut scoped = RayXAppBuilder::new();
    scoped
        .services
        .add_scoped_as::<dyn Session>(|_| Arc::new(Visit(1)));
    scoped
        .services
        .add_scoped_as::<dyn Fonts>(|_| Arc::new(Inter(Arc::new(Desktop))));
    scoped
        .services
        .require_for::<dyn Fonts, dyn Session>("register the session");
    let services = scoped.build().unwrap();
    assert_eq!(services.required_service::<dyn Session>().unwrap().id(), 1);
}

/// A startup service that counts its initializations.
struct Counted(Arc<AtomicUsize>);

impl AsyncInitializable for Counted {
    fn is_initialized(&self) -> bool {
        false
    }

    fn initialize(
        &self,
        _context: StartupInitializationContext,
    ) -> LocalBoxFuture<'static, StartupInitializationResult> {
        self.0.fetch_add(1, Ordering::SeqCst);
        futures::future::ready(Ok(())).boxed_local()
    }
}

impl StartupInitializable for Counted {}

#[test]
fn nothing_runs_before_the_check_fails() {
    let made = Arc::new(AtomicUsize::new(0));
    let initialized = Arc::new(AtomicUsize::new(0));
    let mut builder = RayXAppBuilder::new();
    let counter = Arc::clone(&made);
    builder
        .services
        .add_singleton_as::<dyn Fonts>(Arc::new(Inter(Arc::new(Desktop))) as Arc<dyn Fonts>);
    builder
        .services
        .try_add_service_factory_as::<dyn Session>(move |_| {
            counter.fetch_add(1, Ordering::SeqCst);
            Arc::new(Visit(7))
        });
    let count = Arc::clone(&initialized);
    let startup = Arc::clone(&made);
    builder.services.add_startup_scoped(move |_| {
        startup.fetch_add(1, Ordering::SeqCst);
        Counted(Arc::clone(&count))
    });
    builder.services.require::<dyn Platform>(PLATFORM_HINT);
    assert!(builder.build().is_err());
    assert_eq!(made.load(Ordering::SeqCst), 0, "no factory ran");
    assert_eq!(
        initialized.load(Ordering::SeqCst),
        0,
        "no startup service ran"
    );

    // `build_async` checks first too.
    let mut builder = RayXAppBuilder::new();
    builder.services.require::<dyn Platform>(PLATFORM_HINT);
    let error = futures::executor::block_on(builder.build_async()).unwrap_err();
    assert!(matches!(error, AppBuildError::Services(_)));
}

#[test]
fn an_app_with_everything_registered_builds_as_before() {
    let mut builder = RayXAppBuilder::new();
    builder
        .services
        .add_singleton_as::<dyn Platform>(Arc::new(Desktop) as Arc<dyn Platform>);
    add_fonts(&mut builder);
    builder.services.require::<dyn Fonts>(FONTS_HINT);
    let services = builder.build().unwrap();
    assert_eq!(
        services.required_service::<dyn Fonts>().unwrap().family(),
        "Inter on desktop"
    );
    // Registered after the requirement, by a factory: still there.
    let mut later = RayXAppBuilder::new();
    later.services.require::<dyn Platform>(PLATFORM_HINT);
    later
        .services
        .try_add_service_factory_as::<dyn Platform>(|_| Arc::new(Desktop));
    assert!(later.build().is_ok());
}

/// A transient service between a singleton and a scoped one.
trait Clock: Send + Sync {}

struct Ticking;

impl Clock for Ticking {}

#[test]
fn a_singleton_keeping_a_scoped_service_through_transient_ones_fails_naming_the_chain() {
    let mut builder = RayXAppBuilder::new();
    builder
        .services
        .add_scoped_as::<dyn Session>(|_| Arc::new(Visit(1)));
    builder
        .services
        .add_transient_as::<dyn Clock>(|_| Arc::new(Ticking));
    builder
        .services
        .add_singleton_as::<dyn Fonts>(Arc::new(Inter(Arc::new(Desktop))) as Arc<dyn Fonts>);
    builder
        .services
        .require_for::<dyn Fonts, dyn Clock>("add_clock()");
    builder
        .services
        .require_for::<dyn Clock, dyn Session>("add_session()");
    // A cycle among the dependencies ends.
    builder
        .services
        .require_for::<dyn Session, dyn Clock>("add_clock()");
    let error = builder.build().unwrap_err();
    assert_eq!(
        error.to_string(),
        "the app's services are not complete:\n- `dyn di_validation::Fonts` is a singleton and \
         needs `dyn di_validation::Clock` (transient), which needs `dyn di_validation::Session`, \
         which is scoped (a singleton outlives every scope): register \
         `dyn di_validation::Session` as a singleton, or `dyn di_validation::Fonts` as scoped \
         (add_session())"
    );
}

#[test]
fn the_same_problem_declared_twice_is_one_line() {
    let mut builder = RayXAppBuilder::new();
    builder
        .services
        .add_scoped_as::<dyn Session>(|_| Arc::new(Visit(1)));
    builder
        .services
        .add_singleton_as::<dyn Fonts>(Arc::new(Inter(Arc::new(Desktop))) as Arc<dyn Fonts>);
    for _ in 0..2 {
        builder
            .services
            .require_for::<dyn Fonts, dyn Session>("register the session");
    }
    assert_eq!(builder.build().unwrap_err().to_string().lines().count(), 2);
}

#[test]
fn the_app_s_configuration_counts_as_registered() {
    let mut builder = RayXAppBuilder::new();
    builder
        .services
        .add_singleton_as::<dyn Platform>(Arc::new(Desktop) as Arc<dyn Platform>);
    builder
        .services
        .require_for::<dyn Platform, Configuration>("every app has its configuration");
    assert!(builder.build().is_ok());
    let mut builder = RayXAppBuilder::new();
    builder
        .services
        .require::<Configuration>("every app has its configuration");
    assert!(futures::executor::block_on(builder.build_async()).is_ok());
}
