//! Navigation: routes matched from URIs (`/projects/{id}`), named regions with their own
//! journals (back, forward, push, replace, reset) and the app's location (the URL region's
//! route with the other regions' in its query), published by a [`NavigationService`]
//! implementation as [`NavigationState`] and [`NavigationEvent`]s, without any view.
//!
//! RayX's implementation is `rayx_services_navigation`'s (`add_navigation(..)`), which routes
//! the app's URIs, shows the location as NSH's app address and renders regions as NSH
//! navigation stacks; an app may register its own, in Rust or in TS. Reading the state
//! ([`NavigationState::current`], [`NavigationState::current_in_region`],
//! [`NavigationState::journal`]) is synchronous for any implementation, and so are the same
//! helpers on `dyn NavigationService`.

mod error;
mod journal;
mod model;
mod routes;

pub use error::{NavigationError, Result};
pub use journal::{NavigationJournal, NavigationRegion};
pub use model::{
    LocationChangedEvent, NavigationArgs, NavigationDirection, NavigationEvent,
    NavigationHistoryMode, NavigationLifecycleData, NavigationRequest, NavigationResult,
    NavigationState, RouteChangedEvent,
};
pub use routes::{Route, RouteParams, RoutingData, RoutingStatus};

use futures::future::BoxFuture;

use crate::ChangeSubscription;
use crate::published::{EventStream, StateCell};

/// The navigation service: regions with their journals and current routes, and the app's
/// location, published as [`NavigationState`]; each navigation's lifecycle, route and location
/// changes and each region that closes, published as [`NavigationEvent`]s; and the commands
/// that navigate. A command naming a region that does not exist makes it (except
/// [`go_to_entry`](Self::go_to_entry), which needs its journal).
#[rayx_app_macros::contract(ts = "NavigationServiceContract")]
pub trait NavigationService {
    /// The regions, their journals and current routes, and the location, now and as they
    /// change.
    #[state(ts = "NavigationState")]
    fn navigation(&self) -> &StateCell<NavigationState>;

    /// Each navigation's lifecycle (navigated from, navigated to), route and location changes,
    /// and each region that closes.
    #[events(ts = "NavigationEvent")]
    fn events(&self) -> &EventStream<NavigationEvent>;

    /// The root region's name: where `navigate_to(uri)` navigates (the TS class's `navigate`).
    #[constant]
    fn root_region_name(&self) -> String;

    /// The region whose route is the location's path (the root region unless the
    /// implementation's options name another).
    #[constant]
    fn url_region(&self) -> String;

    /// How `navigate_to(uri)` changes the root region's journal (the TS class's `navigate`).
    #[constant(ts = "NavigationHistoryMode")]
    fn default_history_mode(&self) -> NavigationHistoryMode;

    /// Navigates `request.region` to `request.args.uri`; the result has the direction, the
    /// status (`Success`, `FailedNotFound`, `SkippedSameRoute`, …) and the route.
    async fn navigate(
        &self,
        request: NavigationRequest,
    ) -> Result<NavigationResult, NavigationError>;

    /// Goes back in `region`'s journal (`SkippedNoHistory` at its first entry).
    async fn back(&self, region: String) -> Result<NavigationResult, NavigationError>;

    /// Goes forward in `region`'s journal (`SkippedNoHistory` at its last entry).
    async fn forward(&self, region: String) -> Result<NavigationResult, NavigationError>;

    /// Navigates `region` to its current route again.
    async fn refresh(&self, region: String) -> Result<NavigationResult, NavigationError>;

    /// Makes the entry at `index` of `region`'s journal the current one and navigates there
    /// (back when it is before the current entry, forward when after); an error for a region
    /// or an entry there is not.
    async fn go_to_entry(
        &self,
        region: String,
        index: usize,
    ) -> Result<NavigationResult, NavigationError>;

    /// Makes the region `region` when it does not exist (an empty journal).
    async fn ensure_region(&self, region: String) -> Result<(), NavigationError>;

    /// Closes `region` (not the URL region): its journal empties and the location no longer
    /// names it. Whether it was open.
    async fn close_region(&self, region: String) -> Result<bool, NavigationError>;

    /// Removes `region` (not the root region); whether it existed.
    async fn remove_region(&self, region: String) -> Result<bool, NavigationError>;

    /// Goes where `location` says (the platform's back, forward, a typed address, a deep
    /// link): each region to its route, to the nearest journal entry that has it or as a new
    /// entry, and the regions it no longer names closed. The location is not shown again: the
    /// platform is there already.
    async fn apply_location(
        &self,
        location: String,
    ) -> Result<Vec<NavigationResult>, NavigationError>;
}

impl dyn NavigationService {
    /// Navigates the root region to `uri`, with the implementation's default history mode.
    pub fn navigate_to(&self, uri: impl Into<String>) -> BoxFuture<'_, Result<NavigationResult>> {
        self.navigate(NavigationRequest {
            region: self.root_region_name(),
            args: NavigationArgs {
                history_mode: self.default_history_mode(),
                ..NavigationArgs::new(uri)
            },
        })
    }

    /// Navigates `region` with `args`.
    pub fn navigate_in_region(
        &self,
        region: impl Into<String>,
        args: NavigationArgs,
    ) -> BoxFuture<'_, Result<NavigationResult>> {
        self.navigate(NavigationRequest {
            region: region.into(),
            args,
        })
    }

    /// The root region's current route and its status, `None` before its first navigation.
    pub fn current(&self) -> Option<RoutingData> {
        self.navigation().get().current().cloned()
    }

    /// `region`'s current route and its status.
    pub fn current_in_region(&self, region: &str) -> Option<RoutingData> {
        self.navigation().get().current_in_region(region).cloned()
    }

    /// `region`'s journal.
    pub fn journal(&self, region: &str) -> Option<NavigationJournal> {
        self.navigation().get().journal(region).cloned()
    }

    /// The region `name`, if it exists.
    pub fn region(&self, name: &str) -> Option<NavigationRegion> {
        self.navigation().get().region(name).cloned()
    }

    /// The regions' names, sorted.
    pub fn region_names(&self) -> Vec<String> {
        self.navigation().get().region_names()
    }

    /// The app's location (see [`NavigationState::location`]).
    pub fn location(&self) -> String {
        self.navigation().get().location.clone()
    }

    /// Calls `listener` as a region leaves its route (a region's first navigation leaves
    /// none), before the region's later subscribers hear it, until the subscription is dropped.
    pub fn on_navigated_from(
        &self,
        listener: impl Fn(&NavigationLifecycleData) + Send + Sync + 'static,
    ) -> ChangeSubscription {
        self.events().subscribe(move |event| {
            if let NavigationEvent::NavigatedFrom(data) = event {
                listener(data);
            }
        })
    }

    /// Calls `listener` as a region reaches a route, until the subscription is dropped.
    pub fn on_navigated_to(
        &self,
        listener: impl Fn(&NavigationLifecycleData) + Send + Sync + 'static,
    ) -> ChangeSubscription {
        self.events().subscribe(move |event| {
            if let NavigationEvent::NavigatedTo(data) = event {
                listener(data);
            }
        })
    }

    /// Calls `listener` with every route change, until the subscription is dropped.
    pub fn on_route_changed(
        &self,
        listener: impl Fn(&RouteChangedEvent) + Send + Sync + 'static,
    ) -> ChangeSubscription {
        self.events().subscribe(move |event| {
            if let NavigationEvent::RouteChanged(changed) = event {
                listener(changed);
            }
        })
    }

    /// Calls `listener` with the routing data of every route a region reaches, until the
    /// subscription is dropped.
    pub fn on_location_changed(
        &self,
        listener: impl Fn(&LocationChangedEvent) + Send + Sync + 'static,
    ) -> ChangeSubscription {
        self.events().subscribe(move |event| {
            if let NavigationEvent::LocationChanged(changed) = event {
                listener(changed);
            }
        })
    }

    /// Calls `listener` with the name of every region that closes, until the subscription is
    /// dropped.
    pub fn on_region_closed(
        &self,
        listener: impl Fn(&str) + Send + Sync + 'static,
    ) -> ChangeSubscription {
        self.events().subscribe(move |event| {
            if let NavigationEvent::RegionClosed { region } = event {
                listener(region);
            }
        })
    }
}
