//! The last registration of a key wins completely: registering a service under a key removes
//! the startup services registered as that key before (a service crate's `add_x` registers its
//! implementation and a startup service under the contract's key), unless the registration adds
//! its own startup service, which then replaces them.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use futures::FutureExt;
use futures::future::LocalBoxFuture;
use rayx_app::prelude::*;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

/// What the app resolves; RayX's implementation loads at startup, the app's does not.
trait Translator: Send + Sync {
    fn name(&self) -> &'static str;
}

rayx_app::implemented_by!(Translator);

/// An implementation that counts its startup initializations.
#[derive(Default)]
struct Loading {
    name: &'static str,
    loads: AtomicUsize,
}

impl Loading {
    fn new(name: &'static str) -> Arc<Self> {
        Arc::new(Self {
            name,
            loads: AtomicUsize::new(0),
        })
    }

    fn loads(&self) -> usize {
        self.loads.load(Ordering::SeqCst)
    }
}

impl Translator for Loading {
    fn name(&self) -> &'static str {
        self.name
    }
}

impl AsyncInitializable for Loading {
    fn is_initialized(&self) -> bool {
        self.loads() > 0
    }

    fn initialize(
        &self,
        _context: StartupInitializationContext,
    ) -> LocalBoxFuture<'static, StartupInitializationResult> {
        self.loads.fetch_add(1, Ordering::SeqCst);
        futures::future::ready(Ok(())).boxed_local()
    }
}

impl StartupInitializable for Loading {}

/// The app's own implementation: no startup service.
struct Own;

impl Translator for Own {
    fn name(&self) -> &'static str {
        "own"
    }
}

fn run(provider: &ServiceProvider) -> Result<(), StartupInitializationError> {
    futures::executor::block_on(
        provider
            .startup_registry()
            .initialize(StartupInitializationContext::new(provider.clone())),
    )
}

/// What a service crate's `add_x` does: the implementation under the contract's key, and a
/// startup service made from it when startup runs, registered as the contract.
fn add_rayx_translator(provider: &mut ServiceProvider) -> Arc<Loading> {
    let rayx = Loading::new("rayx");
    let registered = Arc::clone(&rayx);
    provider.replace_service_factory_as::<dyn Translator>(move |_| {
        Arc::clone(&registered) as Arc<dyn Translator>
    });
    let made = Arc::clone(&rayx);
    provider.add_startup_resolved_async(StartupServiceType::of::<dyn Translator>(), move |_| {
        let made = Arc::clone(&made);
        async move { Ok(made as StartupService) }.boxed_local()
    });
    rayx
}

/// A registration of the app's own translator, named.
type Register = fn(&mut ServiceProvider);

fn own() -> Arc<dyn Translator> {
    Arc::new(Own)
}

#[test]
fn di_last_registration_wins_every_plain_registration_removes_the_earlier_startup_service() {
    let registrations: [(&str, Register); 9] = [
        ("add_singleton_as", |provider| {
            provider.add_singleton_as::<dyn Translator>(own());
        }),
        ("add_singleton", |provider| {
            provider.add_singleton::<dyn Translator>(own());
        }),
        ("add_service", |provider| {
            provider.add_service::<dyn Translator>(own());
        }),
        ("add_scoped_as", |provider| {
            provider.add_scoped_as::<dyn Translator>(|_| own());
        }),
        ("add_transient_as", |provider| {
            provider.add_transient_as::<dyn Translator>(|_| own());
        }),
        ("replace_service", |provider| {
            provider.replace_service::<dyn Translator>(own());
        }),
        ("replace_service_factory_as", |provider| {
            provider.replace_service_factory_as::<dyn Translator>(|_| own());
        }),
        ("with_singleton_as", |provider| {
            *provider = provider.clone().with_singleton_as::<dyn Translator>(own());
        }),
        ("with_scoped_as", |provider| {
            *provider = provider.clone().with_scoped_as::<dyn Translator>(|_| own());
        }),
    ];
    for (name, register) in registrations {
        let mut provider = ServiceProvider::new();
        let rayx = add_rayx_translator(&mut provider);
        register(&mut provider);

        assert!(
            !provider.has_startup_role::<dyn Translator>(),
            "{name}: RayX's startup service is gone"
        );
        run(&provider).unwrap_or_else(|error| panic!("{name}: startup runs: {error}"));
        assert_eq!(rayx.loads(), 0, "{name}: RayX's implementation never loads");
        assert_eq!(
            provider
                .required_service::<dyn Translator>()
                .unwrap()
                .name(),
            "own",
            "{name}: the app's implementation resolves"
        );
    }
}

#[test]
fn di_last_registration_wins_a_concrete_type_and_a_runtime_key_alike() {
    let rayx = Loading::new("rayx");
    let mut provider = ServiceProvider::new();
    provider.add_startup_singleton::<Loading>(Arc::clone(&rayx));
    provider.add_singleton::<Loading>(Loading::new("plain"));
    run(&provider).expect("startup runs");
    assert_eq!(rayx.loads(), 0, "the plain registration of `Loading` wins");

    let key = RuntimeKey::new("translator");
    let keyed = Loading::new("keyed");
    let mut provider = ServiceProvider::new();
    let made = Arc::clone(&keyed);
    provider.add_startup_resolved_async(&key, move |_| {
        let made = Arc::clone(&made);
        async move { Ok(made as StartupService) }.boxed_local()
    });
    provider.add_singleton_keyed(key.clone(), |_| {
        Ok(Arc::new(String::from("plain")) as ErasedService)
    });
    run(&provider).expect("startup runs");
    assert_eq!(keyed.loads(), 0, "the keyed registration wins");
}

#[test]
fn di_last_registration_wins_a_startup_registration_replaces_the_earlier_one() {
    let mut provider = ServiceProvider::new();
    let rayx = add_rayx_translator(&mut provider);
    let app = provider.add_startup_singleton_for::<dyn Translator, _>(Loading::new("app"));

    run(&provider).expect("startup runs");

    assert_eq!(rayx.loads(), 0, "RayX's implementation never loads");
    assert_eq!(app.loads(), 1, "the app's loads once");
    assert_eq!(
        provider
            .required_service::<dyn Translator>()
            .unwrap()
            .name(),
        "app"
    );
}

#[test]
fn di_last_registration_wins_a_later_add_x_wins_over_the_apps() {
    let mut provider = ServiceProvider::new();
    provider.add_singleton_as::<dyn Translator>(own());
    let rayx = add_rayx_translator(&mut provider);

    run(&provider).expect("startup runs");

    assert_eq!(rayx.loads(), 1, "RayX's, registered last, loads");
    assert_eq!(
        provider
            .required_service::<dyn Translator>()
            .unwrap()
            .name(),
        "rayx"
    );
}

#[test]
fn di_last_registration_wins_other_keys_keep_their_startup_services() {
    let mut provider = ServiceProvider::new();
    let rayx = add_rayx_translator(&mut provider);
    provider.add_singleton::<String>(Arc::new(String::from("another key")));

    run(&provider).expect("startup runs");

    assert_eq!(rayx.loads(), 1, "RayX's loads: nothing replaced its key");
}

#[test]
fn di_last_registration_wins_a_try_add_that_adds_nothing_removes_nothing() {
    let mut provider = ServiceProvider::new();
    let rayx = add_rayx_translator(&mut provider);
    assert!(!provider.try_add_service::<dyn Translator>(own()));

    run(&provider).expect("startup runs");

    assert_eq!(rayx.loads(), 1);
}

#[test]
fn di_last_registration_wins_a_startup_service_depending_on_the_key_waits_for_the_apps() {
    #[derive(Default)]
    struct Dependent {
        initialized: StartupInitializationState,
    }
    impl AsyncInitializable for Dependent {
        fn is_initialized(&self) -> bool {
            self.initialized.is_initialized()
        }
        fn initialize(
            &self,
            context: StartupInitializationContext,
        ) -> LocalBoxFuture<'static, StartupInitializationResult> {
            let initialized = self.initialized.clone();
            let name = context
                .services()
                .required_service::<dyn Translator>()
                .map(|translator| translator.name());
            async move {
                assert_eq!(name.map_err(|error| error.to_string())?, "own");
                initialized.mark_initialized();
                Ok(())
            }
            .boxed_local()
        }
    }
    impl StartupInitializable for Dependent {
        fn initialization_depends_on(&self) -> Vec<StartupServiceType> {
            vec![StartupServiceType::of::<dyn Translator>()]
        }
    }

    let mut provider = ServiceProvider::new();
    add_rayx_translator(&mut provider);
    provider.add_singleton_as::<dyn Translator>(own());
    provider.add_startup_singleton(Dependent::default());

    run(&provider).expect("the app's plain registration meets the dependency");
    assert!(
        provider
            .required_service::<Dependent>()
            .unwrap()
            .is_initialized()
    );
}
