//! A Rust implementation of an app's own contract (`#[contract]` in an app's crate) that loads at
//! the start: `add_startup_singleton_for::<dyn Contract, Impl>` registers it as the contract and
//! as a startup service registered as it, which the contract's macro makes possible.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use futures::FutureExt as _;
use futures::future::LocalBoxFuture;
use rayx_app_nsh::di::{
    AsyncInitializable, StartupInitializable, StartupInitializationContext,
    StartupInitializationResult,
};
use rayx_app_nsh::{ServiceProvider, StateCell};

/// The app's dictionary, which loads its words at the start.
#[rayx_app_nsh::contract]
pub trait Dictionary {
    /// The words loaded.
    #[state]
    fn words(&self) -> &StateCell<Vec<String>>;
}

struct Bundled {
    words: StateCell<Vec<String>>,
    loaded: AtomicBool,
}

impl Dictionary for Bundled {
    fn words(&self) -> &StateCell<Vec<String>> {
        &self.words
    }
}

impl AsyncInitializable for Bundled {
    fn is_initialized(&self) -> bool {
        self.loaded.load(Ordering::SeqCst)
    }

    fn initialize(
        &self,
        _context: StartupInitializationContext,
    ) -> LocalBoxFuture<'static, StartupInitializationResult> {
        self.words.publish(vec!["bok".into(), "hvala".into()]);
        self.loaded.store(true, Ordering::SeqCst);
        futures::future::ready(Ok(())).boxed_local()
    }
}

impl StartupInitializable for Bundled {}

#[test]
fn nsh_contract_startup_for_an_app_contract_loads_at_the_start() {
    let mut services = ServiceProvider::new();
    services.add_startup_singleton_for::<dyn Dictionary, _>(Arc::new(Bundled {
        words: StateCell::new(Vec::new()),
        loaded: AtomicBool::new(false),
    }));

    futures::executor::block_on(
        services
            .startup_registry()
            .initialize(StartupInitializationContext::new(services.clone())),
    )
    .unwrap();

    let dictionary = services.required_service::<dyn Dictionary>().unwrap();
    assert_eq!(*dictionary.words().get(), ["bok", "hvala"]);
}
