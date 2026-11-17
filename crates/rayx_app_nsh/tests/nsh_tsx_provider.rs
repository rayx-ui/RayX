//! TS `ServiceProvider` (what `await builder.start()` resolves with) through NSH's engine,
//! headless: a RayX service's class is its own token, a singleton is one object, a missing token
//! gives `null` or throws naming it, a scope has scoped services of its own, and `dispose()`
//! releases them and deactivates the view models activated through it.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod headless;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use nsh_core::prelude::*;
use rayx_app_nsh::ts::{RayXAppBuilder, ServiceProvider};
use rayx_app_nsh::{ActivatableViewModel, AppService, ServiceAppBuilderExt, ViewModelActivator};

static DROPPED: AtomicUsize = AtomicUsize::new(0);
static NEXT: AtomicUsize = AtomicUsize::new(0);

/// A singleton: one per surface.
#[derive(AppService)]
struct Session;

/// A scoped service: one per scope, counted when it goes.
#[derive(AppService)]
struct PageState {
    id: usize,
}

impl Drop for PageState {
    fn drop(&mut self) {
        DROPPED.fetch_add(1, Ordering::SeqCst);
    }
}

/// A scoped view model.
#[derive(AppService)]
struct PageViewModel {
    activator: ViewModelActivator,
    page: Arc<PageState>,
}

impl ActivatableViewModel for PageViewModel {
    fn activator(&self) -> &ViewModelActivator {
        &self.activator
    }
}

thread_local! {
    /// The view model `Probe.page` resolved last.
    static VIEW_MODEL: std::cell::RefCell<Option<Arc<PageViewModel>>> =
        const { std::cell::RefCell::new(None) };
}

/// The test's Rust side: its services and what it checks of them.
#[module]
struct Probe;

#[module]
impl Probe {
    /// Adds the test's services to `builder`.
    pub(crate) fn add_pages(
        builder: Instance<RayXAppBuilder>,
    ) -> Result<Instance<RayXAppBuilder>, String> {
        builder
            .with_mut(|builder| {
                builder.extend(|builder| {
                    builder
                        .add_singleton(Session)
                        .add_scoped(|_| PageState {
                            id: NEXT.fetch_add(1, Ordering::SeqCst),
                        })
                        .add_scoped(|provider| PageViewModel {
                            activator: ViewModelActivator::default(),
                            page: provider.required_service::<PageState>().unwrap(),
                        })
                })
            })
            .map_err(|error| error.to_string())?;
        Ok(builder)
    }

    /// The page id of the provider's view model, activated until the provider is disposed.
    pub(crate) fn page(services: Instance<ServiceProvider>) -> Result<u32, String> {
        let view_model = services
            .with_mut(|services| services.view_model::<PageViewModel>())
            .map_err(|error| error.to_string())?;
        let page = view_model.page.id as u32;
        VIEW_MODEL.with_borrow_mut(|kept| *kept = Some(view_model));
        Ok(page)
    }

    /// Whether the view model `page` resolved last is active.
    pub(crate) fn active() -> bool {
        VIEW_MODEL.with_borrow(|kept| {
            kept.as_ref()
                .is_some_and(|view_model| view_model.activator().is_active())
        })
    }

    /// Lets go of the view model `page` resolved last.
    pub(crate) fn release() {
        VIEW_MODEL.with_borrow_mut(|kept| kept.take());
    }

    /// Whether the two providers see the same singleton.
    pub(crate) fn same_session(
        first: Instance<ServiceProvider>,
        second: Instance<ServiceProvider>,
    ) -> Result<bool, String> {
        let session = |services: &Instance<ServiceProvider>| {
            services
                .with(|services| services.services().required_service::<Session>())
                .map_err(|error| error.to_string())
        };
        Ok(Arc::ptr_eq(&session(&first)?, &session(&second)?))
    }

    /// How many page states went so far.
    pub(crate) fn dropped() -> u32 {
        DROPPED.load(Ordering::SeqCst) as u32
    }
}

const TSX: &str = r#"import { mount, signal, Text } from "nsh";
import {
  I18nService, KeyValueStore, NavigationService, Probe, RayXAppBuilder, ServiceToken,
} from "test/app";

const shown = signal("waiting");
setTimeout(async () => {
  const lines = [];
  const thrown = (f) => { try { f(); return "no throw"; } catch (e) { return e.message; } };
  try {
    const services = await Probe.addPages(new RayXAppBuilder().addHostFakes().addStorage().addI18n()).start();

    // A class is its own token; a singleton is one object.
    const i18n = services.getRequiredService(I18nService);
    lines.push(`${i18n instanceof I18nService} ${i18n === services.getRequiredService(I18nService)}`);
    lines.push(String(services.getService(KeyValueStore) === services.getRequiredService(KeyValueStore)));
    lines.push(typeof i18n.locale());

    // Nothing registered: `null`, or an error naming the token.
    lines.push(String(services.getService(NavigationService)));
    lines.push(thrown(() => services.getRequiredService(NavigationService)).includes("NavigationService") ? "named" : "unnamed");
    class Mine {}
    lines.push(String(services.getService(Mine)));
    lines.push(thrown(() => services.getRequiredService(Mine)));
    const clock = new ServiceToken("clock");
    lines.push(`${clock.name} ${services.getService(clock)}`);
    lines.push(thrown(() => services.getRequiredService(clock)));
    lines.push(thrown(() => services.getService(3)));

    // A scope: its own scoped services, the root's singletons.
    const before = Probe.dropped();
    const scope = services.createScope();
    const inner = scope.createScope();
    const page = Probe.page(scope);
    lines.push(String(page === Probe.page(scope) && Probe.active()));
    lines.push(String(page !== Probe.page(inner)));
    lines.push(String(Probe.sameSession(services, scope) && Probe.sameSession(scope, inner)));
    lines.push(String(scope.getRequiredService(I18nService) instanceof I18nService));
    inner.dispose();
    lines.push(`${Probe.dropped() - before} ${Probe.active()}`);
    Probe.page(scope);
    scope.dispose();
    lines.push(String(Probe.active()));
    Probe.release();
    lines.push(String(Probe.dropped() - before));
    lines.push(thrown(() => scope.getService(I18nService)).includes("disposed") ? "disposed" : "used");
    lines.push(thrown(() => scope.createScope()).includes("disposed") ? "disposed" : "used");
  } catch (error) {
    lines.push(`threw ${error}`);
  }
  shown.set(`done ${lines.join("|")}`);
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn the_provider_resolves_by_class_and_token_with_scopes_disposed() {
    let set = headless::set(|set| {
        set.module::<Probe>();
    });
    assert_eq!(
        headless::run(set, TSX),
        "done true true|true|string|null|named|null|no service is registered for Mine\
         |clock null|no service is registered for ServiceToken(clock)\
         |ServiceProvider.getService: `token`: expected an object, got 3\
         |true|true|true|true|0 false|false|2|disposed|disposed"
    );
}

#[test]
fn the_declarations_type_both_lookups_over_service_key() {
    let set = headless::set(|_| {});
    let ts = headless::declarations(&set);
    for declared in [
        "export type ServiceKey<T> = { readonly prototype: T } | ServiceToken<T>;",
        "export declare class ServiceProvider {",
        "  getService<T>(token: ServiceKey<T>): T | null;",
        "  getRequiredService<T>(token: ServiceKey<T>): T;",
        "  createScope(): ServiceProvider;",
        "export declare class ServiceToken<T> {",
        "  constructor(name: string);",
    ] {
        assert!(ts.contains(declared), "no `{declared}` in\n{ts}");
    }
    assert_eq!(set.undocumented(), Vec::<String>::new());
}
