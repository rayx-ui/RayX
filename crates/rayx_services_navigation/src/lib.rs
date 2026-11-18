//! RayX's navigation service on NSH: the app's routes (`/projects/{id}`), named regions with
//! their own journals and the app's location (the URL region's route with the other regions'
//! in its query), published by RayX's implementation of
//! [`rayx_app::services::navigation::NavigationService`]. The location is shown as NSH's app
//! address (the web's address bar and history, the link that opened a native app): the
//! navigation starts at the address the app started at (a deep link) and follows the
//! platform's moves of it and the links that open the app while it runs. A region renders as
//! an NSH navigation stack ([`navigation_region`]) or as its current route
//! ([`navigation_view`]), over whichever `dyn NavigationService` is registered.
//!
//! - Rust: `RayXAppBuilder::new().add_navigation(|options| { .. })`
//!   ([`NavigationAppBuilderExt`]).
//! - TS: `new RayXAppBuilder().addNavigation((options) => { options.addRoute("/"); })`, with the
//!   classes [`NavigationService`] (the contract's token) and [`NavigationRegion`]
//!   (`navigationService.region("panel")`).
//! - The app's set: [`register`].

mod builder;
mod navigation_ts;
mod nsh_location;
mod options;
mod options_ts;
mod region;
mod region_ts;
mod routes;
mod service;
mod synchronizer;

use nsh_core::element::{Registry, TsAlias};
use rayx_app::services::navigation::*;
use rayx_app::{EventStream, StateCell};
use rayx_app_nsh::ts::ContractRegistryExt as _;

pub use builder::{NavigationAppBuilderExt, NavigationBuilder};
pub use navigation_ts::NavigationService;
pub use nsh_location::location_of_address;
pub use options::{NAVIGATION_OPTIONS_SECTION, NavigationOptions};
pub use options_ts::{NavigationOptions as TsNavigationOptions, RouteOptions};
pub use region::{navigation_region, navigation_view};
pub use region_ts::NavigationRegion;
pub use routes::{RouteBuilder, RouteDefinition, RouteDescriptor, RouteMap, parse_route};

rayx_app_nsh::contract_nsh!(
    rayx_app::services::navigation::NavigationService as NavigationContract
);

/// The navigation's state in TS.
pub const NAVIGATION_STATE: TsAlias = TsAlias {
    name: "NavigationState",
    ts: "{ root_region: string; regions: Record<string, unknown>; location: string }",
    doc: "The regions, their journals and current routes, and the app's location: the JSON of Rust's `NavigationState`.",
};

/// A navigation event in TS.
pub const NAVIGATION_EVENT: TsAlias = TsAlias {
    name: "NavigationEvent",
    ts: "{ kind: string; region: string; [key: string]: unknown }",
    doc: "A navigation's lifecycle, route or location change, or a region closing: the JSON of Rust's `NavigationEvent` (`kind` as Rust names it).",
};

/// A navigation request in TS.
pub const NAVIGATION_REQUEST: TsAlias = TsAlias {
    name: "NavigationRequest",
    ts: "{ region: string; args: { uri: string; parameters: unknown; history_mode: NavigationHistoryMode; synchronize_location: boolean } }",
    doc: "A navigation of a region: the JSON of Rust's `NavigationRequest`.",
};

/// A navigation's result in TS.
pub const NAVIGATION_RESULT: TsAlias = TsAlias {
    name: "NavigationResult",
    ts: "{ region: string; direction: NavigationDirection; status: RoutingStatus; route: NavigationRoute }",
    doc: "How a navigation went: the JSON of Rust's `NavigationResult`.",
};

/// A route in TS.
pub const NAVIGATION_ROUTE: TsAlias = TsAlias {
    name: "NavigationRoute",
    ts: "{ uri: string; base_uri: string; path: string; query: string; fragment: string; location: string; segments: string[]; params: Record<string, string>; query_params: Record<string, string>; passed_parameters: unknown; component: string | null; pattern: string; prewarm: boolean }",
    doc: "A URI resolved against the navigation's routes: its parts, the route it matched (`pattern`, `component`, `params`) and the parameters passed with the navigation (`passed_parameters`, `null` when none): the JSON of Rust's `Route`.",
};

/// A route with how resolving it went, in TS.
pub const ROUTING_DATA: TsAlias = TsAlias {
    name: "RoutingData",
    ts: "{ status: RoutingStatus; route: NavigationRoute }",
    doc: "A route and how resolving it went: the JSON of Rust's `RoutingData`.",
};

/// How a navigation went, in TS.
pub const ROUTING_STATUS: TsAlias = TsAlias {
    name: "RoutingStatus",
    ts: "\"None\" | \"Success\" | \"FailedNotFound\" | \"ComponentNotFound\" | \"SkippedSameRoute\" | \"SkippedExternalRoute\" | \"SkippedNoHistory\" | \"Cancelled\"",
    doc: "How a navigation went: Rust's `RoutingStatus`.",
};

/// Which way a navigation went, in TS.
pub const NAVIGATION_DIRECTION: TsAlias = TsAlias {
    name: "NavigationDirection",
    ts: "\"New\" | \"Back\" | \"Forward\" | \"Refresh\"",
    doc: "Which way a navigation went: Rust's `NavigationDirection`.",
};

/// A region's route change in TS.
pub const ROUTE_CHANGED_EVENT: TsAlias = TsAlias {
    name: "RouteChangedEvent",
    ts: "{ region: string; from: NavigationRoute | null; to: NavigationRoute }",
    doc: "A region's route changed (`from`: `null` for the region's first): the JSON of Rust's `RouteChangedEvent`.",
};

/// A history mode in TS.
pub const NAVIGATION_HISTORY_MODE: TsAlias = TsAlias {
    name: "NavigationHistoryMode",
    ts: "\"None\" | \"Push\" | \"Replace\" | \"Reset\"",
    doc: "How a navigation changes its region's journal.",
};

/// Adds the navigation service's TS side to an app's set: its classes [`NavigationService`]
/// and [`NavigationRegion`], its options classes, the contract a TS implementation follows (the
/// TS interface `NavigationServiceContract`, [`NavigationContract`] in Rust) and
/// `builder.addNavigation(..)`.
pub fn register(registry: &mut Registry) {
    registry
        .ts_alias(NAVIGATION_STATE)
        .ts_alias(NAVIGATION_EVENT)
        .ts_alias(NAVIGATION_REQUEST)
        .ts_alias(NAVIGATION_RESULT)
        .ts_alias(NAVIGATION_HISTORY_MODE)
        .ts_alias(NAVIGATION_ROUTE)
        .ts_alias(ROUTING_DATA)
        .ts_alias(ROUTING_STATUS)
        .ts_alias(NAVIGATION_DIRECTION)
        .ts_alias(ROUTE_CHANGED_EVENT)
        .class::<TsNavigationOptions>()
        .class::<NavigationRegion>()
        .contract_class::<NavigationContract, NavigationService>()
        .class_extension::<NavigationBuilder>();
}

/// RayX's navigation service over a location synchronizer of the test's own, for the crate's
/// tests and an app's.
#[doc(hidden)]
pub mod testing {
    use std::sync::Arc;

    use rayx_app::AppBuilder;
    use rayx_app::services::navigation::NavigationService;

    use crate::NavigationOptions;
    pub use crate::synchronizer::{
        NavigationLocationSynchronizer, NoopNavigationLocationSynchronizer,
    };

    /// `add_navigation`, showing the location through `synchronizer` in place of NSH.
    pub fn add_navigation_with<B: AppBuilder>(
        builder: B,
        configure: impl Fn(&mut NavigationOptions) + Send + Sync + 'static,
        synchronizer: Arc<dyn NavigationLocationSynchronizer>,
    ) -> B {
        crate::builder::add_navigation_with(builder, configure, move |_| Arc::clone(&synchronizer))
    }

    /// RayX's navigation service with `options`, showing the location through `synchronizer`,
    /// not started (no route yet).
    pub fn navigation(
        options: NavigationOptions,
        synchronizer: Arc<dyn NavigationLocationSynchronizer>,
    ) -> std::result::Result<
        Arc<dyn NavigationService>,
        rayx_app::services::navigation::NavigationError,
    > {
        Ok(Arc::new(crate::service::RayxNavigation::new(
            options,
            synchronizer,
        )?))
    }
}
