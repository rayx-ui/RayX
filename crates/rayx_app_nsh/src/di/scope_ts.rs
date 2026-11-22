//! The TS `ScopeRegistrations`: what `services.createScope(configure)` gives `configure`, to
//! register services written in TS in the new scope alone.

use nsh_core::prelude::*;
use rayx_app::di::ServiceLifetime;

use crate::di::container_ts::{self as container, Factory, SharedTsService, TsService};
use crate::di::contract_ts::ContractDef;
use crate::di::provider_ts::ServiceKeyArg;

/// The registrations of a scope of its own, for TSX: `createScope(configure)` calls `configure`
/// with one at once, and what it registers there is the new scope's alone.
///
/// ```ts
/// const scope = services.createScope((scope) => {
///   scope.addSingletonAs(Clock, () => new FixedClock(7));
/// });
/// ```
///
/// For a token it registers, its registration wins in the scope and its descendants over the
/// root's, also once the root made it. A singleton is one object per scope that registers it,
/// shared by its descendants and always made with that scope: once the scope is disposed, a
/// descendant that resolves it first gets one made over the scope's registrations and a scoped
/// cache of its own, never over the descendant's scoped services, and the other descendants
/// share it; it is released when the scope and all its descendants are disposed. Scoped and
/// transient ones follow their lifetimes there. Each factory gets the `ServiceProvider` it resolves in.
/// Startup services belong to the root: the startup methods throw, naming the token.
#[class]
pub struct ScopeRegistrations {
    recorded: Vec<Recorded>,
}

/// One registration `configure` made.
pub(crate) struct Recorded {
    lifetime: ServiceLifetime,
    service: SharedTsService,
    contract: Option<ContractDef>,
}

impl ScopeRegistrations {
    pub(crate) fn new() -> Self {
        Self {
            recorded: Vec::new(),
        }
    }

    /// What `configure` registered, in order.
    pub(crate) fn take(&mut self) -> Vec<Recorded> {
        std::mem::take(&mut self.recorded)
    }
}

#[class]
impl ScopeRegistrations {
    /// Registers a singleton written in TS under `token` (a TS class, a `ServiceToken`, or a
    /// contract's token): `factory` makes it on its first resolve, once for this scope and its
    /// descendants.
    #[ts("<T>(token: ContractClass<T>, factory: (services: ServiceProvider) => T): this")]
    #[ts("<T>(token: ServiceKey<T>, factory: (services: ServiceProvider) => T): this")]
    pub fn add_singleton(
        this: Instance<Self>,
        token: ServiceKeyArg,
        #[args(services)] factory: Factory,
        sys: System,
    ) -> Result<Instance<Self>, String> {
        add(this, ServiceLifetime::Singleton, &token, factory, &sys)
    }

    /// [`addSingleton`](Self::add_singleton) for an implementation registered under an
    /// interface's token.
    #[ts("<T>(token: ContractClass<T>, factory: (services: ServiceProvider) => T): this")]
    #[ts("<T>(token: ServiceKey<T>, factory: (services: ServiceProvider) => T): this")]
    pub fn add_singleton_as(
        this: Instance<Self>,
        token: ServiceKeyArg,
        #[args(services)] factory: Factory,
        sys: System,
    ) -> Result<Instance<Self>, String> {
        add(this, ServiceLifetime::Singleton, &token, factory, &sys)
    }

    /// Registers a scoped service written in TS under `token`: one object per scope (this one
    /// and each of its descendants).
    #[ts("<T>(token: ContractClass<T>, factory: (services: ServiceProvider) => T): this")]
    #[ts("<T>(token: ServiceKey<T>, factory: (services: ServiceProvider) => T): this")]
    pub fn add_scoped(
        this: Instance<Self>,
        token: ServiceKeyArg,
        #[args(services)] factory: Factory,
        sys: System,
    ) -> Result<Instance<Self>, String> {
        add(this, ServiceLifetime::Scoped, &token, factory, &sys)
    }

    /// [`addScoped`](Self::add_scoped) for an implementation registered under an interface's
    /// token.
    #[ts("<T>(token: ContractClass<T>, factory: (services: ServiceProvider) => T): this")]
    #[ts("<T>(token: ServiceKey<T>, factory: (services: ServiceProvider) => T): this")]
    pub fn add_scoped_as(
        this: Instance<Self>,
        token: ServiceKeyArg,
        #[args(services)] factory: Factory,
        sys: System,
    ) -> Result<Instance<Self>, String> {
        add(this, ServiceLifetime::Scoped, &token, factory, &sys)
    }

    /// Registers a transient service written in TS under `token`: `factory` makes a new object
    /// on every resolve.
    #[ts("<T>(token: ContractClass<T>, factory: (services: ServiceProvider) => T): this")]
    #[ts("<T>(token: ServiceKey<T>, factory: (services: ServiceProvider) => T): this")]
    pub fn add_transient(
        this: Instance<Self>,
        token: ServiceKeyArg,
        #[args(services)] factory: Factory,
        sys: System,
    ) -> Result<Instance<Self>, String> {
        add(this, ServiceLifetime::Transient, &token, factory, &sys)
    }

    /// [`addTransient`](Self::add_transient) for an implementation registered under an
    /// interface's token.
    #[ts("<T>(token: ContractClass<T>, factory: (services: ServiceProvider) => T): this")]
    #[ts("<T>(token: ServiceKey<T>, factory: (services: ServiceProvider) => T): this")]
    pub fn add_transient_as(
        this: Instance<Self>,
        token: ServiceKeyArg,
        #[args(services)] factory: Factory,
        sys: System,
    ) -> Result<Instance<Self>, String> {
        add(this, ServiceLifetime::Transient, &token, factory, &sys)
    }

    /// Refused: startup services belong to the root (the app's builder). Throws, naming
    /// `token`.
    pub fn add_startup_singleton(
        &self,
        token: ServiceKeyArg,
        #[args(services)] factory: Factory,
    ) -> Result<(), String> {
        drop(factory);
        Err(startup_refused(&token))
    }

    /// Refused: startup services belong to the root (the app's builder). Throws, naming
    /// `token`.
    pub fn add_startup_singleton_as(
        &self,
        token: ServiceKeyArg,
        #[args(services)] factory: Factory,
    ) -> Result<(), String> {
        drop(factory);
        Err(startup_refused(&token))
    }

    /// Refused: startup services belong to the root (the app's builder). Throws, naming
    /// `token`.
    pub fn add_startup_scoped(
        &self,
        token: ServiceKeyArg,
        #[args(services)] factory: Factory,
    ) -> Result<(), String> {
        drop(factory);
        Err(startup_refused(&token))
    }

    /// Refused: startup services belong to the root (the app's builder). Throws, naming
    /// `token`.
    pub fn add_startup_scoped_as(
        &self,
        token: ServiceKeyArg,
        #[args(services)] factory: Factory,
    ) -> Result<(), String> {
        drop(factory);
        Err(startup_refused(&token))
    }

    /// Refused: startup services belong to the root (the app's builder). Throws, naming
    /// `token`.
    pub fn add_startup_transient(
        &self,
        token: ServiceKeyArg,
        #[args(services)] factory: Factory,
    ) -> Result<(), String> {
        drop(factory);
        Err(startup_refused(&token))
    }

    /// Refused: startup services belong to the root (the app's builder). Throws, naming
    /// `token`.
    pub fn add_startup_transient_as(
        &self,
        token: ServiceKeyArg,
        #[args(services)] factory: Factory,
    ) -> Result<(), String> {
        drop(factory);
        Err(startup_refused(&token))
    }
}

/// Why a scope refuses a startup service under `token`.
fn startup_refused(token: &ServiceKeyArg) -> String {
    format!(
        "{} is a startup service: startup services belong to the root, so register it with the \
         app's builder, not in a scope",
        token.name()
    )
}

/// `this` with the TS service `factory` makes recorded under `token` with `lifetime`.
fn add(
    this: Instance<ScopeRegistrations>,
    lifetime: ServiceLifetime,
    token: &ServiceKeyArg,
    factory: Factory,
    sys: &System,
) -> Result<Instance<ScopeRegistrations>, String> {
    if token.is_rayx_service() {
        return Err(format!(
            "{} is a RayX service's class: register a TS service under a TS class or a ServiceToken",
            token.name()
        ));
    }
    let service = TsService::new(token, factory, sys)?;
    this.with_mut(|this| {
        this.recorded.push(Recorded {
            lifetime,
            service,
            contract: token.contract(),
        });
    });
    Ok(this)
}

/// Adds `recorded` to `scope`, the new scope's own registrations: each TS service, and the
/// adapter of the contract it implements.
pub(crate) fn apply(
    scope: &mut rayx_app::di::ScopeRegistrations,
    recorded: Vec<Recorded>,
) -> Result<(), String> {
    for Recorded {
        lifetime,
        service,
        contract,
    } in recorded
    {
        container::add_in_scope(scope, lifetime, service.clone())?;
        if let Some(contract) = contract {
            (contract.add_in_scope)(scope, lifetime, service);
        }
    }
    Ok(())
}
