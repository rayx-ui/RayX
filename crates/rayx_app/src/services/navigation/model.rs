use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{NavigationJournal, NavigationRegion, Route, RoutingData, RoutingStatus};

/// What a navigation goes to, and how.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct NavigationArgs {
    pub uri: String,
    /// Parameters passed with the navigation (the route's `passed_parameters`).
    pub parameters: Option<Value>,
    pub history_mode: NavigationHistoryMode,
    /// Whether the app's location is shown after it (the platform's own moves are not).
    pub synchronize_location: bool,
}

impl NavigationArgs {
    pub fn new(uri: impl Into<String>) -> Self {
        Self {
            uri: uri.into(),
            ..Self::default()
        }
    }
}

impl Default for NavigationArgs {
    fn default() -> Self {
        Self {
            uri: String::new(),
            parameters: None,
            history_mode: NavigationHistoryMode::Push,
            synchronize_location: true,
        }
    }
}

/// A navigation of a region.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct NavigationRequest {
    pub region: String,
    pub args: NavigationArgs,
}

/// How a navigation went: the region, its direction, its status and the route it reached (or
/// stayed at).
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct NavigationResult {
    pub region: String,
    pub direction: NavigationDirection,
    pub status: RoutingStatus,
    pub route: Route,
}

/// How a navigation changes its region's journal.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum NavigationHistoryMode {
    /// Not at all.
    None,
    /// A new entry after the current one (the ones after it go).
    Push,
    /// In place of the current entry.
    Replace,
    /// The only entry.
    Reset,
}

/// Which way a navigation went.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum NavigationDirection {
    New,
    Back,
    Forward,
    Refresh,
}

/// A region's route changed.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RouteChangedEvent {
    pub region: String,
    pub from: Option<Route>,
    pub to: Route,
}

/// A region reached a route: its routing data.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct LocationChangedEvent {
    pub region: String,
    pub data: RoutingData,
}

/// A navigation of a region, for its lifecycle subscribers: where it went from (none for the
/// region's first) and to, and which way.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct NavigationLifecycleData {
    pub region: String,
    pub direction: NavigationDirection,
    pub from: Option<RoutingData>,
    pub to: RoutingData,
}

/// What a navigation service publishes as it navigates, in this order for each successful
/// navigation: [`NavigatedFrom`](Self::NavigatedFrom) (when the region had a route),
/// [`NavigatedTo`](Self::NavigatedTo), [`RouteChanged`](Self::RouteChanged) and
/// [`LocationChanged`](Self::LocationChanged); [`RegionClosed`](Self::RegionClosed) when a
/// region closes. In TS: `{ kind: "RouteChanged", region, from, to }` and the like.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum NavigationEvent {
    /// A region leaves its route.
    NavigatedFrom(NavigationLifecycleData),
    /// A region reached its route.
    NavigatedTo(NavigationLifecycleData),
    /// A region's route changed.
    RouteChanged(RouteChangedEvent),
    /// A region reached a route: its routing data.
    LocationChanged(LocationChangedEvent),
    /// A region closed: its journal emptied and the location no longer names it.
    RegionClosed { region: String },
}

/// The navigation now: every region with its journal and current route, the root region's
/// name and the app's location.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct NavigationState {
    /// The region [`current`](Self::current) reads (`root` by default).
    pub root_region: String,
    /// Every region, by name.
    pub regions: BTreeMap<String, NavigationRegion>,
    /// The app's location: the URL region's route behind the base path, with the other
    /// regions' routes in its query (`/app/projects/12?panel=%2Fsettings`).
    pub location: String,
}

impl NavigationState {
    /// The region `name`, if it exists.
    pub fn region(&self, name: &str) -> Option<&NavigationRegion> {
        self.regions.get(name)
    }

    /// The regions' names, sorted.
    pub fn region_names(&self) -> Vec<String> {
        self.regions.keys().cloned().collect()
    }

    /// The root region's current route and its status, `None` before its first navigation.
    pub fn current(&self) -> Option<&RoutingData> {
        self.current_in_region(&self.root_region)
    }

    /// `region`'s current route and its status.
    pub fn current_in_region(&self, region: &str) -> Option<&RoutingData> {
        self.regions
            .get(region)
            .and_then(|region| region.current.as_ref())
    }

    /// `region`'s journal.
    pub fn journal(&self, region: &str) -> Option<&NavigationJournal> {
        self.regions.get(region).map(|region| &region.journal)
    }

    /// `region`'s journal up to its current entry, with their indexes: the pages of a
    /// navigation stack showing it.
    pub fn pages(&self, region: &str) -> Vec<(usize, Route)> {
        self.journal(region)
            .map(NavigationJournal::pages)
            .unwrap_or_default()
    }
}
