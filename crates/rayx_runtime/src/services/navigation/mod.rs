//! Navigation: routes matched from URIs (`/sports/{sport}`), named regions with their own
//! journals (back, forward, push, replace, reset), lifecycle subscribers and a location
//! synchronizer, without any view: the NSH layer renders the current route.

mod app_ext;
mod error;
mod journal;
mod routes;
mod service;

pub use app_ext::{NAVIGATION_OPTIONS_SECTION, NavigationAppBuilderExt};
pub use error::{NavigationError, Result};
pub use journal::{NavigationJournal, NavigationRegion};
pub use routes::{
    Route, RouteBuilder, RouteDefinition, RouteDescriptor, RouteMap, RouteParams, RoutingData,
    RoutingStatus,
};
pub use service::{
    LocationChangedEvent, NavigationArgs, NavigationDirection, NavigationHistoryMode,
    NavigationLifecycleData, NavigationLocationSynchronizer, NavigationOptions, NavigationRequest,
    NavigationResult, NavigationService, NavigationSubscription,
    NoopNavigationLocationSynchronizer, RouteChangedEvent,
};
