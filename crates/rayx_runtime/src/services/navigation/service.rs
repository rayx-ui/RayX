use super::{
    NavigationError, NavigationRegion, Result, Route, RouteDefinition, RouteMap, RoutingData,
    RoutingStatus,
};
use futures::FutureExt;
use futures::future::LocalBoxFuture;
use rayx_core::di::{
    AppService, AsyncInitializable, StartupInitializable, StartupInitializationContext,
    StartupInitializationResult, StartupInitializationState, StartupServiceType,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NavigationOptions {
    pub base_uri: String,
    pub default_route: String,
    pub default_history_mode: NavigationHistoryMode,
    pub root_region_name: String,
    pub max_journal_entries: usize,
    pub synchronize_location: bool,
    /// The region whose route is the location's path (default: the root region).
    pub url_region: Option<String>,
    /// The location's prefix before the app's routes (`/app`; default none): the location of
    /// `/teams/12` is `/app/teams/12`.
    pub base_path: String,
    /// The other regions the location carries in its query, by name (`?panel=%2Fbets`), so a
    /// location read at startup reaches them before they exist; a region made at run time is
    /// carried too.
    pub query_regions: Vec<String>,
    pub not_found_route: Option<String>,
    pub external_route: Option<String>,
    pub component_not_found_route: Option<String>,
    pub routes: Vec<RouteDefinition>,
}

impl Default for NavigationOptions {
    fn default() -> Self {
        Self {
            base_uri: "app://".to_string(),
            default_route: "/".to_string(),
            default_history_mode: NavigationHistoryMode::Push,
            root_region_name: "root".to_string(),
            max_journal_entries: 40,
            synchronize_location: true,
            url_region: None,
            base_path: String::new(),
            query_regions: Vec::new(),
            not_found_route: None,
            external_route: None,
            component_not_found_route: None,
            routes: Vec::new(),
        }
    }
}

impl NavigationOptions {
    pub fn add_route(&mut self, definition: RouteDefinition) -> &mut Self {
        self.routes.push(definition);
        self
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct NavigationArgs {
    pub uri: String,
    pub parameters: Option<Value>,
    pub history_mode: NavigationHistoryMode,
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

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct NavigationRequest {
    pub region: String,
    pub args: NavigationArgs,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct NavigationResult {
    pub region: String,
    pub direction: NavigationDirection,
    pub status: RoutingStatus,
    pub route: Route,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum NavigationHistoryMode {
    None,
    Push,
    Replace,
    Reset,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum NavigationDirection {
    New,
    Back,
    Forward,
    Refresh,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RouteChangedEvent {
    pub region: String,
    pub from: Option<Route>,
    pub to: Route,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocationChangedEvent {
    pub region: String,
    pub data: RoutingData,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NavigationLifecycleData {
    pub region: String,
    pub direction: NavigationDirection,
    pub from: Option<RoutingData>,
    pub to: RoutingData,
}

/// Where the app's location is shown and comes from: the browser's address bar, an embedding
/// app, a deep link. The location is the URL region's route (behind the base path) with the
/// other regions' routes in its query: `/teams/12?panel=%2Fbets%2Fslip`.
pub trait NavigationLocationSynchronizer: Send + Sync + 'static {
    /// The platform's location as the app starts (a deep link, the address bar), if it has one.
    fn location(&self) -> Option<String> {
        None
    }

    /// Shows `location` after a navigation, as a new entry of the platform's history (`Push`)
    /// or in place of the current one (the other modes).
    fn synchronize(&self, location: &str, history_mode: NavigationHistoryMode) -> Result<()>;
}

impl AppService for dyn NavigationLocationSynchronizer {}

#[derive(Clone, Debug, Default)]
pub struct NoopNavigationLocationSynchronizer;

impl NavigationLocationSynchronizer for NoopNavigationLocationSynchronizer {
    fn synchronize(&self, _location: &str, _history_mode: NavigationHistoryMode) -> Result<()> {
        Ok(())
    }
}

#[derive(Clone, AppService)]
#[app_service(startup)]
pub struct NavigationService {
    routes: RouteMap,
    options: NavigationOptions,
    location_synchronizer: Arc<dyn NavigationLocationSynchronizer>,
    state: Arc<Mutex<NavigationState>>,
    initialization: StartupInitializationState,
}

impl std::fmt::Debug for NavigationService {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("NavigationService")
            .field("routes", &self.routes)
            .field("options", &self.options)
            .finish_non_exhaustive()
    }
}

#[derive(Default)]
struct NavigationState {
    regions: HashMap<String, NavigationRegion>,
    route_subscribers: BTreeMap<usize, RouteSubscriber>,
    location_subscribers: BTreeMap<usize, LocationSubscriber>,
    closed_subscribers: BTreeMap<usize, ClosedSubscriber>,
    navigated_from_subscribers: BTreeMap<usize, LifecycleSubscriber>,
    navigated_to_subscribers: BTreeMap<usize, LifecycleSubscriber>,
    next_subscription_id: usize,
}

type RouteSubscriber = Arc<dyn Fn(&RouteChangedEvent) + Send + Sync>;
type LocationSubscriber = Arc<dyn Fn(&LocationChangedEvent) + Send + Sync>;
type ClosedSubscriber = Arc<dyn Fn(&str) + Send + Sync>;
type LifecycleSubscriber = Arc<dyn Fn(&NavigationLifecycleData) + Send + Sync>;

pub struct NavigationSubscription {
    state: Arc<Mutex<NavigationState>>,
    id: usize,
    kind: NavigationSubscriptionKind,
}

#[derive(Clone, Copy)]
enum NavigationSubscriptionKind {
    Route,
    Location,
    Closed,
    NavigatedFrom,
    NavigatedTo,
}

impl Drop for NavigationSubscription {
    fn drop(&mut self) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };

        match self.kind {
            NavigationSubscriptionKind::Route => {
                state.route_subscribers.remove(&self.id);
            }
            NavigationSubscriptionKind::Location => {
                state.location_subscribers.remove(&self.id);
            }
            NavigationSubscriptionKind::Closed => {
                state.closed_subscribers.remove(&self.id);
            }
            NavigationSubscriptionKind::NavigatedFrom => {
                state.navigated_from_subscribers.remove(&self.id);
            }
            NavigationSubscriptionKind::NavigatedTo => {
                state.navigated_to_subscribers.remove(&self.id);
            }
        }
    }
}

impl NavigationService {
    pub fn new(routes: RouteMap) -> Self {
        let options = NavigationOptions {
            base_uri: routes.base_uri().to_string(),
            routes: routes.routes().to_vec(),
            ..NavigationOptions::default()
        };
        Self::with_route_map_and_options(routes, options)
    }

    pub fn with_options(options: NavigationOptions) -> Result<Self> {
        let mut options = options;
        Self::finalize_options(&mut options)?;
        Self::from_final_options(options)
    }

    pub(crate) fn finalize_options(options: &mut NavigationOptions) -> Result<()> {
        Self::route_map(options).map(|_| ())
    }

    pub(crate) fn from_final_options(options: NavigationOptions) -> Result<Self> {
        let routes = Self::route_map(&options)?;
        Ok(Self::with_route_map_and_options(routes, options))
    }

    fn route_map(options: &NavigationOptions) -> Result<RouteMap> {
        let mut routes = RouteMap::new(options.base_uri.clone());
        for definition in options.routes.iter().cloned() {
            routes.try_add(definition)?;
        }
        Ok(routes)
    }

    pub fn with_location_synchronizer(
        mut self,
        synchronizer: Arc<dyn NavigationLocationSynchronizer>,
    ) -> Self {
        self.location_synchronizer = synchronizer;
        self
    }

    fn with_route_map_and_options(routes: RouteMap, options: NavigationOptions) -> Self {
        let root = NavigationRegion::new(
            options.root_region_name.clone(),
            options.max_journal_entries,
        );
        let mut regions = HashMap::new();
        regions.insert(options.root_region_name.clone(), root);
        Self {
            routes,
            options,
            location_synchronizer: Arc::new(NoopNavigationLocationSynchronizer),
            state: Arc::new(Mutex::new(NavigationState {
                regions,
                ..NavigationState::default()
            })),
            initialization: StartupInitializationState::default(),
        }
    }

    pub fn root_region_name(&self) -> &str {
        &self.options.root_region_name
    }

    pub fn options(&self) -> &NavigationOptions {
        &self.options
    }

    pub fn get_or_create_region(&self, name: impl Into<String>) -> Result<NavigationRegion> {
        let name = name.into();
        let mut state = self.state.lock().map_err(|_| NavigationError::Poisoned)?;
        Ok(state
            .regions
            .entry(name.clone())
            .or_insert_with(|| NavigationRegion::new(name, self.options.max_journal_entries))
            .clone())
    }

    pub fn try_get_region(&self, name: &str) -> Result<Option<NavigationRegion>> {
        Ok(self
            .state
            .lock()
            .map_err(|_| NavigationError::Poisoned)?
            .regions
            .get(name)
            .cloned())
    }

    pub fn remove_region(&self, name: &str) -> Result<bool> {
        if name == self.options.root_region_name {
            return Ok(false);
        }

        Ok(self
            .state
            .lock()
            .map_err(|_| NavigationError::Poisoned)?
            .regions
            .remove(name)
            .is_some())
    }

    pub fn region_names(&self) -> Result<Vec<String>> {
        let mut names = self
            .state
            .lock()
            .map_err(|_| NavigationError::Poisoned)?
            .regions
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        names.sort();
        Ok(names)
    }

    pub fn current(&self) -> Result<Option<RoutingData>> {
        self.current_in_region(&self.options.root_region_name)
    }

    pub fn current_in_region(&self, region: &str) -> Result<Option<RoutingData>> {
        Ok(self
            .state
            .lock()
            .map_err(|_| NavigationError::Poisoned)?
            .regions
            .get(region)
            .and_then(|region| region.current.clone()))
    }

    pub fn journal(&self, region: &str) -> Result<Option<super::NavigationJournal>> {
        Ok(self
            .state
            .lock()
            .map_err(|_| NavigationError::Poisoned)?
            .regions
            .get(region)
            .map(|region| region.journal.clone()))
    }

    pub fn navigate_to(&self, uri: impl Into<String>) -> Result<NavigationResult> {
        self.navigate(NavigationRequest {
            region: self.options.root_region_name.clone(),
            args: NavigationArgs {
                history_mode: self.options.default_history_mode,
                ..NavigationArgs::new(uri)
            },
        })
    }

    pub fn navigate(&self, request: NavigationRequest) -> Result<NavigationResult> {
        self.navigate_core(
            request.region,
            request.args,
            NavigationDirection::New,
            false,
        )
    }

    pub fn navigate_in_region(
        &self,
        region: impl Into<String>,
        args: NavigationArgs,
    ) -> Result<NavigationResult> {
        self.navigate_core(region.into(), args, NavigationDirection::New, false)
    }

    pub fn back(&self) -> Result<NavigationResult> {
        self.back_in_region(&self.options.root_region_name)
    }

    pub fn back_in_region(&self, region: &str) -> Result<NavigationResult> {
        let route = {
            let mut state = self.state.lock().map_err(|_| NavigationError::Poisoned)?;
            let region = state
                .regions
                .entry(region.to_string())
                .or_insert_with(|| NavigationRegion::new(region, self.options.max_journal_entries));
            let Some(route) = region.journal.try_go_back() else {
                let route = region.current_route().cloned().unwrap_or(
                    self.routes
                        .process_route(&self.options.default_route, None)?
                        .route,
                );
                return Ok(NavigationResult {
                    region: region.name.clone(),
                    direction: NavigationDirection::Back,
                    status: RoutingStatus::SkippedNoHistory,
                    route,
                });
            };
            route
        };

        let args = NavigationArgs {
            uri: route.location.clone(),
            parameters: route.passed_parameters.clone(),
            history_mode: NavigationHistoryMode::None,
            synchronize_location: false,
        };
        let result =
            self.navigate_core(region.to_string(), args, NavigationDirection::Back, false)?;
        if !matches!(
            result.status,
            RoutingStatus::Success | RoutingStatus::SkippedSameRoute
        ) {
            let mut state = self.state.lock().map_err(|_| NavigationError::Poisoned)?;
            if let Some(region) = state.regions.get_mut(region) {
                let _ = region.journal.try_go_forward();
            }
        }
        if result.status == RoutingStatus::Success {
            self.sync(NavigationHistoryMode::Replace, true)?;
        }
        Ok(result)
    }

    pub fn forward(&self) -> Result<NavigationResult> {
        self.forward_in_region(&self.options.root_region_name)
    }

    pub fn forward_in_region(&self, region: &str) -> Result<NavigationResult> {
        let route = {
            let mut state = self.state.lock().map_err(|_| NavigationError::Poisoned)?;
            let region = state
                .regions
                .entry(region.to_string())
                .or_insert_with(|| NavigationRegion::new(region, self.options.max_journal_entries));
            let Some(route) = region.journal.try_go_forward() else {
                let route = region.current_route().cloned().unwrap_or(
                    self.routes
                        .process_route(&self.options.default_route, None)?
                        .route,
                );
                return Ok(NavigationResult {
                    region: region.name.clone(),
                    direction: NavigationDirection::Forward,
                    status: RoutingStatus::SkippedNoHistory,
                    route,
                });
            };
            route
        };

        let args = NavigationArgs {
            uri: route.location.clone(),
            parameters: route.passed_parameters.clone(),
            history_mode: NavigationHistoryMode::None,
            synchronize_location: false,
        };
        let result = self.navigate_core(
            region.to_string(),
            args,
            NavigationDirection::Forward,
            false,
        )?;
        if !matches!(
            result.status,
            RoutingStatus::Success | RoutingStatus::SkippedSameRoute
        ) {
            let mut state = self.state.lock().map_err(|_| NavigationError::Poisoned)?;
            if let Some(region) = state.regions.get_mut(region) {
                let _ = region.journal.try_go_back();
            }
        }
        if result.status == RoutingStatus::Success {
            self.sync(NavigationHistoryMode::Replace, true)?;
        }
        Ok(result)
    }

    pub fn refresh(&self) -> Result<NavigationResult> {
        self.refresh_in_region(&self.options.root_region_name)
    }

    pub fn refresh_in_region(&self, region: &str) -> Result<NavigationResult> {
        let Some(current) = self.current_in_region(region)? else {
            return Ok(NavigationResult {
                region: region.to_string(),
                direction: NavigationDirection::Refresh,
                status: RoutingStatus::SkippedNoHistory,
                route: self
                    .routes
                    .process_route(&self.options.default_route, None)?
                    .route,
            });
        };
        let args = NavigationArgs {
            uri: current.route.location.clone(),
            parameters: current.route.passed_parameters.clone(),
            history_mode: NavigationHistoryMode::None,
            synchronize_location: false,
        };
        let result =
            self.navigate_core(region.to_string(), args, NavigationDirection::Refresh, true)?;
        if result.status == RoutingStatus::Success {
            self.sync(NavigationHistoryMode::Replace, true)?;
        }
        Ok(result)
    }

    /// The region whose route is the location's path ([`NavigationOptions::url_region`]).
    pub fn url_region(&self) -> &str {
        self.options
            .url_region
            .as_deref()
            .unwrap_or(&self.options.root_region_name)
    }

    /// The app's location: the URL region's route behind the base path, with the other
    /// regions' routes in its query (`/app/teams/12?panel=%2Fbets`), the regions sorted by name.
    pub fn location(&self) -> Result<String> {
        let state = self.state.lock().map_err(|_| NavigationError::Poisoned)?;
        let url_region = self.url_region();
        let main = state
            .regions
            .get(url_region)
            .and_then(NavigationRegion::current_route)
            .map_or_else(
                || self.options.default_route.clone(),
                |route| route.location.clone(),
            );
        let mut others: Vec<(&String, &Route)> = state
            .regions
            .iter()
            .filter(|(name, _)| name.as_str() != url_region)
            .filter_map(|(name, region)| region.current_route().map(|route| (name, route)))
            .collect();
        others.sort_by(|a, b| a.0.cmp(b.0));
        let (without_fragment, fragment) = match main.split_once('#') {
            Some((path, fragment)) => (path.to_string(), Some(fragment.to_string())),
            None => (main, None),
        };
        let mut location = format!("{}{without_fragment}", self.base_path());
        if !others.is_empty() {
            let mut query = url::form_urlencoded::Serializer::new(String::new());
            for (name, route) in others {
                query.append_pair(name, &route.location);
            }
            location.push(if location.contains('?') { '&' } else { '?' });
            location.push_str(&query.finish());
        }
        if let Some(fragment) = fragment {
            location.push('#');
            location.push_str(&fragment);
        }
        Ok(location)
    }

    /// Goes where `location` says (the platform's back, forward, a typed address, a deep
    /// link): each region to its route, to the nearest journal entry that has it (so back and
    /// forward move the journal) or as a new entry, and the query regions it no longer names
    /// closed. The location is not shown again: the platform is there already.
    pub fn apply_location(&self, location: &str) -> Result<Vec<NavigationResult>> {
        let (main, regions) = self.parse_location(location)?;
        let mut results = vec![self.follow(self.url_region().to_string(), main)?];
        let named: Vec<String> = regions.iter().map(|(name, _)| name.clone()).collect();
        for (name, uri) in regions {
            results.push(self.follow(name, uri)?);
        }
        let open: Vec<String> = {
            let state = self.state.lock().map_err(|_| NavigationError::Poisoned)?;
            state
                .regions
                .iter()
                .filter(|(name, region)| {
                    name.as_str() != self.url_region()
                        && region.current.is_some()
                        && !named.contains(name)
                })
                .map(|(name, _)| name.clone())
                .collect()
        };
        for name in open {
            self.close_region_core(&name, false)?;
        }
        Ok(results)
    }

    /// Makes the entry at `index` of `region`'s journal the current one and navigates there
    /// (back when it is before the current entry, forward when after).
    pub fn go_to_entry(&self, region: &str, index: usize) -> Result<NavigationResult> {
        self.go_to_entry_core(region, index, true)
    }

    /// Closes `region` (not the URL region): its journal empties and the location no longer
    /// names it. Whether it was open.
    pub fn close_region(&self, region: &str) -> Result<bool> {
        self.close_region_core(region, true)
    }

    /// Calls `subscriber` with the name of every region that closes.
    pub fn subscribe_region_closed(
        &self,
        subscriber: impl Fn(&str) + Send + Sync + 'static,
    ) -> Result<NavigationSubscription> {
        let mut state = self.state.lock().map_err(|_| NavigationError::Poisoned)?;
        let id = state.next_subscription_id;
        state.next_subscription_id += 1;
        state.closed_subscribers.insert(id, Arc::new(subscriber));
        Ok(NavigationSubscription {
            state: Arc::clone(&self.state),
            id,
            kind: NavigationSubscriptionKind::Closed,
        })
    }

    fn base_path(&self) -> &str {
        self.options.base_path.trim_end_matches('/')
    }

    /// The URL region's route and the query regions' routes `location` names.
    fn parse_location(&self, location: &str) -> Result<(String, Vec<(String, String)>)> {
        let base = self.base_path();
        let relative = match location.strip_prefix(base) {
            Some(rest)
                if base.is_empty() || rest.is_empty() || rest.starts_with(['/', '?', '#']) =>
            {
                rest
            }
            _ => {
                return Err(NavigationError::Location(format!(
                    "{location} is not under the base path {base}"
                )));
            }
        };
        let (without_fragment, fragment) = match relative.split_once('#') {
            Some((path, fragment)) => (path, Some(fragment)),
            None => (relative, None),
        };
        let (path, query) = without_fragment
            .split_once('?')
            .unwrap_or((without_fragment, ""));
        let known: Vec<String> = {
            let state = self.state.lock().map_err(|_| NavigationError::Poisoned)?;
            self.options
                .query_regions
                .iter()
                .cloned()
                .chain(state.regions.keys().cloned())
                .filter(|name| name != self.url_region())
                .collect()
        };
        let mut kept = url::form_urlencoded::Serializer::new(String::new());
        let mut kept_any = false;
        let mut regions = Vec::new();
        for (key, value) in url::form_urlencoded::parse(query.as_bytes()) {
            if known.iter().any(|name| *name == key) {
                regions.push((key.into_owned(), value.into_owned()));
            } else {
                kept.append_pair(&key, &value);
                kept_any = true;
            }
        }
        let mut main = if path.is_empty() {
            "/".to_string()
        } else {
            path.to_string()
        };
        if kept_any {
            main.push('?');
            main.push_str(&kept.finish());
        }
        if let Some(fragment) = fragment {
            main.push('#');
            main.push_str(fragment);
        }
        Ok((main, regions))
    }

    /// Takes `region` to `uri` without showing the location: to the nearest journal entry
    /// that has it, else as a new entry.
    fn follow(&self, region: String, uri: String) -> Result<NavigationResult> {
        let (current, entry) = {
            let state = self.state.lock().map_err(|_| NavigationError::Poisoned)?;
            let region_state = state.regions.get(&region);
            (
                region_state
                    .and_then(NavigationRegion::current_route)
                    .map(|route| route.location.clone()),
                region_state.and_then(|region| region.journal.nearest(&uri)),
            )
        };
        if current.as_deref() == Some(uri.as_str()) {
            let route = self.routes.process_route(&uri, None)?.route;
            return Ok(NavigationResult {
                region,
                direction: NavigationDirection::New,
                status: RoutingStatus::SkippedSameRoute,
                route,
            });
        }
        match entry {
            Some(index) => self.go_to_entry_core(&region, index, false),
            None => self.navigate_core(
                region,
                NavigationArgs {
                    synchronize_location: false,
                    ..NavigationArgs::new(uri)
                },
                NavigationDirection::New,
                false,
            ),
        }
    }

    fn go_to_entry_core(&self, region: &str, index: usize, show: bool) -> Result<NavigationResult> {
        let (route, direction, previous_index) = {
            let mut state = self.state.lock().map_err(|_| NavigationError::Poisoned)?;
            let Some(region_state) = state.regions.get_mut(region) else {
                return Err(NavigationError::Location(format!("no region {region}")));
            };
            let previous_index = region_state.journal.current_index();
            let direction = match previous_index {
                Some(current) if index < current => NavigationDirection::Back,
                Some(current) if index == current => NavigationDirection::Refresh,
                _ => NavigationDirection::Forward,
            };
            let Some(route) = region_state.journal.go_to(index) else {
                return Err(NavigationError::Location(format!(
                    "region {region} has no journal entry {index}"
                )));
            };
            (route, direction, previous_index)
        };
        let args = NavigationArgs {
            uri: route.location.clone(),
            parameters: route.passed_parameters.clone(),
            history_mode: NavigationHistoryMode::None,
            synchronize_location: false,
        };
        let result = self.navigate_core(
            region.to_string(),
            args,
            direction,
            direction == NavigationDirection::Refresh,
        )?;
        if !matches!(
            result.status,
            RoutingStatus::Success | RoutingStatus::SkippedSameRoute
        ) && let Some(previous) = previous_index
        {
            let mut state = self.state.lock().map_err(|_| NavigationError::Poisoned)?;
            if let Some(region) = state.regions.get_mut(region) {
                let _ = region.journal.go_to(previous);
            }
        }
        if result.status == RoutingStatus::Success {
            self.sync(NavigationHistoryMode::Replace, show)?;
        }
        Ok(result)
    }

    fn close_region_core(&self, region: &str, show: bool) -> Result<bool> {
        if region == self.url_region() {
            return Ok(false);
        }
        let subscribers: Vec<ClosedSubscriber> = {
            let mut state = self.state.lock().map_err(|_| NavigationError::Poisoned)?;
            let Some(region_state) = state.regions.get_mut(region) else {
                return Ok(false);
            };
            if region_state.current.is_none() {
                return Ok(false);
            }
            region_state.current = None;
            region_state.journal.clear();
            state.closed_subscribers.values().cloned().collect()
        };
        for subscriber in subscribers {
            subscriber(region);
        }
        self.sync(self.options.default_history_mode, show)?;
        Ok(true)
    }

    pub fn on_navigated_from(
        &self,
        subscriber: impl Fn(&NavigationLifecycleData) + Send + Sync + 'static,
    ) -> Result<NavigationSubscription> {
        let mut state = self.state.lock().map_err(|_| NavigationError::Poisoned)?;
        let id = state.next_subscription_id;
        state.next_subscription_id += 1;
        state
            .navigated_from_subscribers
            .insert(id, Arc::new(subscriber));
        Ok(NavigationSubscription {
            state: Arc::clone(&self.state),
            id,
            kind: NavigationSubscriptionKind::NavigatedFrom,
        })
    }

    pub fn on_navigated_to(
        &self,
        subscriber: impl Fn(&NavigationLifecycleData) + Send + Sync + 'static,
    ) -> Result<NavigationSubscription> {
        let mut state = self.state.lock().map_err(|_| NavigationError::Poisoned)?;
        let id = state.next_subscription_id;
        state.next_subscription_id += 1;
        state
            .navigated_to_subscribers
            .insert(id, Arc::new(subscriber));
        Ok(NavigationSubscription {
            state: Arc::clone(&self.state),
            id,
            kind: NavigationSubscriptionKind::NavigatedTo,
        })
    }

    pub fn subscribe_route_changed(
        &self,
        subscriber: impl Fn(&RouteChangedEvent) + Send + Sync + 'static,
    ) -> Result<NavigationSubscription> {
        let mut state = self.state.lock().map_err(|_| NavigationError::Poisoned)?;
        let id = state.next_subscription_id;
        state.next_subscription_id += 1;
        state.route_subscribers.insert(id, Arc::new(subscriber));
        Ok(NavigationSubscription {
            state: Arc::clone(&self.state),
            id,
            kind: NavigationSubscriptionKind::Route,
        })
    }

    pub fn subscribe_location_changed(
        &self,
        subscriber: impl Fn(&LocationChangedEvent) + Send + Sync + 'static,
    ) -> Result<NavigationSubscription> {
        let mut state = self.state.lock().map_err(|_| NavigationError::Poisoned)?;
        let id = state.next_subscription_id;
        state.next_subscription_id += 1;
        state.location_subscribers.insert(id, Arc::new(subscriber));
        Ok(NavigationSubscription {
            state: Arc::clone(&self.state),
            id,
            kind: NavigationSubscriptionKind::Location,
        })
    }

    fn navigate_core(
        &self,
        region: String,
        args: NavigationArgs,
        direction: NavigationDirection,
        allow_same_route: bool,
    ) -> Result<NavigationResult> {
        let mut data = self
            .routes
            .process_route(&args.uri, args.parameters.clone())?;
        if has_errors(data.status)
            && let Some(fallback) = self.resolve_error_route(&data)?
        {
            data = fallback;
        }

        let mut route_event = None;
        let mut location_event = None;
        let mut lifecycle_event = None;
        let mut route_subscribers = Vec::new();
        let mut location_subscribers = Vec::new();
        let mut navigated_from_subscribers = Vec::new();
        let mut navigated_to_subscribers = Vec::new();

        let result = {
            let mut state = self.state.lock().map_err(|_| NavigationError::Poisoned)?;
            let region_state = state.regions.entry(region.clone()).or_insert_with(|| {
                NavigationRegion::new(region.clone(), self.options.max_journal_entries)
            });

            if !allow_same_route
                && region_state
                    .current_route()
                    .is_some_and(|current| current.uri == data.route.uri)
            {
                data.status = RoutingStatus::SkippedSameRoute;
                return Ok(NavigationResult {
                    region,
                    direction,
                    status: data.status,
                    route: data.route,
                });
            }

            if data.status == RoutingStatus::Success {
                let previous = region_state.current.clone();
                region_state
                    .journal
                    .record(data.route.clone(), args.history_mode);
                region_state.current = Some(data.clone());
                route_event = Some(RouteChangedEvent {
                    region: region.clone(),
                    from: previous.as_ref().map(|value| value.route.clone()),
                    to: data.route.clone(),
                });
                location_event = Some(LocationChangedEvent {
                    region: region.clone(),
                    data: data.clone(),
                });
                lifecycle_event = Some(NavigationLifecycleData {
                    region: region.clone(),
                    direction,
                    from: previous,
                    to: data.clone(),
                });

                route_subscribers = state.route_subscribers.values().cloned().collect();
                location_subscribers = state.location_subscribers.values().cloned().collect();
                navigated_from_subscribers =
                    state.navigated_from_subscribers.values().cloned().collect();
                navigated_to_subscribers =
                    state.navigated_to_subscribers.values().cloned().collect();
            }

            NavigationResult {
                region: region.clone(),
                direction,
                status: data.status,
                route: data.route.clone(),
            }
        };

        if let Some(event) = &lifecycle_event {
            if event.from.is_some() {
                for subscriber in navigated_from_subscribers {
                    subscriber(event);
                }
            }
            for subscriber in navigated_to_subscribers {
                subscriber(event);
            }
        }
        if let Some(event) = &route_event {
            for subscriber in route_subscribers {
                subscriber(event);
            }
        }
        if let Some(event) = &location_event {
            for subscriber in location_subscribers {
                subscriber(event);
            }
        }

        if result.status == RoutingStatus::Success {
            self.sync(args.history_mode, args.synchronize_location)?;
        }

        Ok(result)
    }

    fn resolve_error_route(&self, failed_data: &RoutingData) -> Result<Option<RoutingData>> {
        let route = match failed_data.status {
            RoutingStatus::FailedNotFound => self.options.not_found_route.as_deref(),
            RoutingStatus::SkippedExternalRoute => self.options.external_route.as_deref(),
            RoutingStatus::ComponentNotFound => self.options.component_not_found_route.as_deref(),
            _ => None,
        };

        let Some(route) = route else {
            return Ok(None);
        };

        let fallback = self.routes.process_route(route, None)?;
        if fallback.status != RoutingStatus::Success
            || fallback
                .route
                .path
                .eq_ignore_ascii_case(&failed_data.route.path)
        {
            return Ok(None);
        }

        Ok(Some(fallback))
    }

    /// Shows the app's [`location`](Self::location) through the synchronizer.
    fn sync(&self, history_mode: NavigationHistoryMode, synchronize_location: bool) -> Result<()> {
        if !self.options.synchronize_location || !synchronize_location {
            return Ok(());
        }

        self.location_synchronizer
            .synchronize(&self.location()?, history_mode)
    }

    fn initialize_immediate(&self) -> Result<bool> {
        if self.initialization.is_initialized() {
            return Ok(true);
        }

        if self.routes.is_empty() {
            self.initialization.mark_initialized();
            return Ok(true);
        }

        // The platform's location (a deep link) first, else the default route.
        if let Some(location) = self.location_synchronizer.location() {
            let results = self.apply_location(&location)?;
            let url_region = self.url_region().to_string();
            if results.iter().any(|result| {
                result.region == url_region && result.status == RoutingStatus::Success
            }) {
                self.initialization.mark_initialized();
                return Ok(true);
            }
        }

        let result = self.navigate(NavigationRequest {
            region: self.url_region().to_string(),
            args: NavigationArgs {
                uri: self.options.default_route.clone(),
                history_mode: NavigationHistoryMode::Replace,
                ..NavigationArgs::default()
            },
        })?;

        if result.status == RoutingStatus::Success {
            self.initialization.mark_initialized();
            return Ok(true);
        }

        Ok(false)
    }
}

impl AsyncInitializable for NavigationService {
    fn is_initialized(&self) -> bool {
        self.initialization.is_initialized()
    }

    fn initialize(
        &self,
        _context: StartupInitializationContext,
    ) -> LocalBoxFuture<'static, StartupInitializationResult> {
        let service = self.clone();
        async move {
            if service
                .initialize_immediate()
                .map_err(|error| error.to_string())?
            {
                Ok(())
            } else {
                Err("navigation default route did not resolve".into())
            }
        }
        .boxed_local()
    }
}

impl StartupInitializable for NavigationService {
    fn startup_service_type(&self) -> StartupServiceType {
        StartupServiceType::of::<Self>()
    }
}

fn has_errors(status: RoutingStatus) -> bool {
    matches!(
        status,
        RoutingStatus::FailedNotFound
            | RoutingStatus::SkippedExternalRoute
            | RoutingStatus::ComponentNotFound
    )
}
