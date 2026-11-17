//! RayX's navigation service: the app's routes, its regions with their journals and the app's
//! location, shown through a [`NavigationLocationSynchronizer`], published as
//! [`NavigationState`] and [`NavigationEvent`]s.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, TryLockError};

use futures::FutureExt as _;
use futures::future::{BoxFuture, LocalBoxFuture};
use rayx_app::di::{
    AsyncInitializable, StartupInitializable, StartupInitializationContext,
    StartupInitializationResult, StartupInitializationState, StartupServiceType,
};
use rayx_app::services::navigation::{
    LocationChangedEvent, NavigationArgs, NavigationDirection, NavigationError, NavigationEvent,
    NavigationHistoryMode, NavigationLifecycleData, NavigationRegion, NavigationRequest,
    NavigationResult, NavigationService, NavigationState, Result, Route, RouteChangedEvent,
    RoutingData, RoutingStatus,
};
use rayx_app::{EventStream, StateCell};

use crate::options::NavigationOptions;
use crate::routes::RouteMap;
use crate::synchronizer::NavigationLocationSynchronizer;

/// RayX's navigation service: routes the app's URIs, keeps each region's journal, publishes
/// the state after every change and each navigation's events after it, and shows the app's
/// location through its synchronizer. Its commands answer at once, on any thread.
pub(crate) struct RayxNavigation {
    routes: RouteMap,
    options: NavigationOptions,
    synchronizer: Arc<dyn NavigationLocationSynchronizer>,
    regions: Mutex<HashMap<String, NavigationRegion>>,
    navigation: StateCell<NavigationState>,
    events: EventStream<NavigationEvent>,
    /// Held while the state is published: a change made meanwhile (by a subscriber, or on
    /// another thread) marks it [`stale`](Self::stale) and the holder publishes again.
    publishing: Mutex<()>,
    stale: AtomicBool,
    pub(crate) initialization: StartupInitializationState,
}

impl std::fmt::Debug for RayxNavigation {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RayxNavigation")
            .field("routes", &self.routes)
            .field("options", &self.options)
            .finish_non_exhaustive()
    }
}

impl RayxNavigation {
    /// The service of `options` (its routes checked), showing the location through
    /// `synchronizer`.
    pub(crate) fn new(
        options: NavigationOptions,
        synchronizer: Arc<dyn NavigationLocationSynchronizer>,
    ) -> Result<Self> {
        let routes = Self::route_map(&options)?;
        let root = NavigationRegion::new(
            options.root_region_name.clone(),
            options.max_journal_entries,
        );
        let mut regions = HashMap::new();
        regions.insert(options.root_region_name.clone(), root);
        let service = Self {
            routes,
            synchronizer,
            regions: Mutex::new(regions),
            navigation: StateCell::new(NavigationState::default()),
            events: EventStream::new(),
            publishing: Mutex::new(()),
            stale: AtomicBool::new(false),
            initialization: StartupInitializationState::default(),
            options,
        };
        service.publish_state();
        Ok(service)
    }

    /// Whether `options`' routes are valid.
    pub(crate) fn check(options: &NavigationOptions) -> Result<()> {
        Self::route_map(options).map(|_| ())
    }

    fn route_map(options: &NavigationOptions) -> Result<RouteMap> {
        let mut routes = RouteMap::new(options.base_uri.clone());
        for definition in options.routes.iter().cloned() {
            routes.try_add(definition)?;
        }
        Ok(routes)
    }

    pub(crate) fn options(&self) -> &NavigationOptions {
        &self.options
    }

    fn lock(&self) -> Result<MutexGuard<'_, HashMap<String, NavigationRegion>>> {
        self.regions.lock().map_err(|_| NavigationError::Poisoned)
    }

    fn new_region(&self, name: &str) -> NavigationRegion {
        NavigationRegion::new(name, self.options.max_journal_entries)
    }

    /// Publishes the state now, unless it is the one published: a change made while it is
    /// published is published right after, by whoever is publishing.
    fn publish_state(&self) {
        self.stale.store(true, Ordering::SeqCst);
        loop {
            let publishing = match self.publishing.try_lock() {
                Ok(guard) => guard,
                Err(TryLockError::Poisoned(poisoned)) => poisoned.into_inner(),
                Err(TryLockError::WouldBlock) => return,
            };
            self.stale.store(false, Ordering::SeqCst);
            let state = {
                let regions = self.regions.lock().unwrap_or_else(PoisonError::into_inner);
                self.snapshot(&regions)
            };
            if *self.navigation.get() != state {
                self.navigation.publish(state);
            }
            drop(publishing);
            if !self.stale.load(Ordering::SeqCst) {
                return;
            }
        }
    }

    fn snapshot(&self, regions: &HashMap<String, NavigationRegion>) -> NavigationState {
        NavigationState {
            root_region: self.options.root_region_name.clone(),
            regions: regions
                .iter()
                .map(|(name, region)| (name.clone(), region.clone()))
                .collect(),
            location: self.location_of(regions),
        }
    }

    /// The region whose route is the location's path ([`NavigationOptions::url_region`]).
    pub(crate) fn url_region(&self) -> &str {
        self.options
            .url_region
            .as_deref()
            .unwrap_or(&self.options.root_region_name)
    }

    /// The app's location now.
    pub(crate) fn location(&self) -> Result<String> {
        let regions = self.lock()?;
        Ok(self.location_of(&regions))
    }

    /// The app's location: the URL region's route behind the base path, with the other
    /// regions' routes in its query (`/app/projects/12?panel=%2Fsettings`), the regions sorted by
    /// name.
    fn location_of(&self, regions: &HashMap<String, NavigationRegion>) -> String {
        let url_region = self.url_region();
        let main = regions
            .get(url_region)
            .and_then(NavigationRegion::current_route)
            .map_or_else(
                || self.options.default_route.clone(),
                |route| route.location.clone(),
            );
        let mut others: Vec<(&String, &Route)> = regions
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
        location
    }

    pub(crate) fn ensure_region_now(&self, name: &str) -> Result<()> {
        let made = {
            let mut regions = self.lock()?;
            if regions.contains_key(name) {
                false
            } else {
                regions.insert(name.to_owned(), self.new_region(name));
                true
            }
        };
        if made {
            self.publish_state();
        }
        Ok(())
    }

    pub(crate) fn remove_region_now(&self, name: &str) -> Result<bool> {
        if name == self.options.root_region_name {
            return Ok(false);
        }
        let removed = self.lock()?.remove(name).is_some();
        if removed {
            self.publish_state();
        }
        Ok(removed)
    }

    pub(crate) fn navigate_now(&self, request: NavigationRequest) -> Result<NavigationResult> {
        self.navigate_core(
            request.region,
            request.args,
            NavigationDirection::New,
            false,
        )
    }

    /// The default route, for a navigation that went nowhere.
    fn default_route(&self) -> Result<Route> {
        Ok(self
            .routes
            .process_route(&self.options.default_route, None)?
            .route)
    }

    /// Moves `region`'s journal with `step` (made when missing) and navigates to the entry it
    /// gives, moving it back with `undo` when the navigation fails.
    fn step(
        &self,
        region: &str,
        direction: NavigationDirection,
        step: impl FnOnce(&mut NavigationRegion) -> Option<Route>,
        undo: impl FnOnce(&mut NavigationRegion),
    ) -> Result<NavigationResult> {
        let route = {
            let mut regions = self.lock()?;
            let state = regions
                .entry(region.to_string())
                .or_insert_with(|| self.new_region(region));
            let Some(route) = step(state) else {
                let route = match state.current_route().cloned() {
                    Some(route) => route,
                    None => self.default_route()?,
                };
                return Ok(NavigationResult {
                    region: state.name.clone(),
                    direction,
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
        let result = self.navigate_core(region.to_string(), args, direction, false)?;
        if !matches!(
            result.status,
            RoutingStatus::Success | RoutingStatus::SkippedSameRoute
        ) {
            let mut regions = self.lock()?;
            if let Some(state) = regions.get_mut(region) {
                undo(state);
            }
        }
        self.publish_state();
        if result.status == RoutingStatus::Success {
            self.sync(NavigationHistoryMode::Replace, true)?;
        }
        Ok(result)
    }

    pub(crate) fn back_now(&self, region: &str) -> Result<NavigationResult> {
        self.step(
            region,
            NavigationDirection::Back,
            |state| state.journal.try_go_back(),
            |state| {
                let _ = state.journal.try_go_forward();
            },
        )
    }

    pub(crate) fn forward_now(&self, region: &str) -> Result<NavigationResult> {
        self.step(
            region,
            NavigationDirection::Forward,
            |state| state.journal.try_go_forward(),
            |state| {
                let _ = state.journal.try_go_back();
            },
        )
    }

    pub(crate) fn refresh_now(&self, region: &str) -> Result<NavigationResult> {
        let current = self
            .lock()?
            .get(region)
            .and_then(|region| region.current.clone());
        let Some(current) = current else {
            return Ok(NavigationResult {
                region: region.to_string(),
                direction: NavigationDirection::Refresh,
                status: RoutingStatus::SkippedNoHistory,
                route: self.default_route()?,
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

    /// Goes where `location` says (the platform's back, forward, a typed address, a deep
    /// link): each region to its route, to the nearest journal entry that has it (so back and
    /// forward move the journal) or as a new entry, and the query regions it no longer names
    /// closed. The location is not shown again: the platform is there already.
    pub(crate) fn apply_location_now(&self, location: &str) -> Result<Vec<NavigationResult>> {
        let (main, regions) = self.parse_location(location)?;
        let mut results = vec![self.follow(self.url_region().to_string(), main)?];
        let named: Vec<String> = regions.iter().map(|(name, _)| name.clone()).collect();
        for (name, uri) in regions {
            results.push(self.follow(name, uri)?);
        }
        let open: Vec<String> = self
            .lock()?
            .iter()
            .filter(|(name, region)| {
                name.as_str() != self.url_region()
                    && region.current.is_some()
                    && !named.contains(name)
            })
            .map(|(name, _)| name.clone())
            .collect();
        for name in open {
            self.close_region_core(&name, false)?;
        }
        Ok(results)
    }

    pub(crate) fn go_to_entry_now(&self, region: &str, index: usize) -> Result<NavigationResult> {
        self.go_to_entry_core(region, index, true)
    }

    pub(crate) fn close_region_now(&self, region: &str) -> Result<bool> {
        self.close_region_core(region, true)
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
            let regions = self.lock()?;
            self.options
                .query_regions
                .iter()
                .cloned()
                .chain(regions.keys().cloned())
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
            let regions = self.lock()?;
            let region_state = regions.get(&region);
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
            let mut regions = self.lock()?;
            let Some(region_state) = regions.get_mut(region) else {
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
            let mut regions = self.lock()?;
            if let Some(region) = regions.get_mut(region) {
                let _ = region.journal.go_to(previous);
            }
        }
        self.publish_state();
        if result.status == RoutingStatus::Success {
            self.sync(NavigationHistoryMode::Replace, show)?;
        }
        Ok(result)
    }

    fn close_region_core(&self, region: &str, show: bool) -> Result<bool> {
        if region == self.url_region() {
            return Ok(false);
        }
        {
            let mut regions = self.lock()?;
            let Some(region_state) = regions.get_mut(region) else {
                return Ok(false);
            };
            if region_state.current.is_none() {
                return Ok(false);
            }
            region_state.current = None;
            region_state.journal.clear();
        }
        self.publish_state();
        self.events.publish(NavigationEvent::RegionClosed {
            region: region.to_owned(),
        });
        self.sync(self.options.default_history_mode, show)?;
        Ok(true)
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

        let mut previous = None;
        {
            let mut regions = self.lock()?;
            let region_state = regions
                .entry(region.clone())
                .or_insert_with(|| self.new_region(&region));

            if !allow_same_route
                && region_state
                    .current_route()
                    .is_some_and(|current| current.uri == data.route.uri)
            {
                return Ok(NavigationResult {
                    region,
                    direction,
                    status: RoutingStatus::SkippedSameRoute,
                    route: data.route,
                });
            }

            if data.status == RoutingStatus::Success {
                previous = Some(region_state.current.clone());
                region_state
                    .journal
                    .record(data.route.clone(), args.history_mode);
                region_state.current = Some(data.clone());
            }
        }

        let result = NavigationResult {
            region: region.clone(),
            direction,
            status: data.status,
            route: data.route.clone(),
        };
        let Some(previous) = previous else {
            // Nothing changed (a route that did not resolve, without a fallback).
            return Ok(result);
        };

        // The state first, so the events' subscribers read where the navigation went.
        self.publish_state();
        let lifecycle = NavigationLifecycleData {
            region: region.clone(),
            direction,
            from: previous.clone(),
            to: data.clone(),
        };
        if previous.is_some() {
            self.events
                .publish(NavigationEvent::NavigatedFrom(lifecycle.clone()));
        }
        self.events.publish(NavigationEvent::NavigatedTo(lifecycle));
        self.events
            .publish(NavigationEvent::RouteChanged(RouteChangedEvent {
                region: region.clone(),
                from: previous.map(|value| value.route),
                to: data.route.clone(),
            }));
        self.events
            .publish(NavigationEvent::LocationChanged(LocationChangedEvent {
                region,
                data,
            }));

        self.sync(args.history_mode, args.synchronize_location)?;
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

        self.synchronizer
            .synchronize(&self.location()?, history_mode)
    }

    /// Goes to the platform's location (a deep link), else to the default route.
    fn initialize_now(&self) -> Result<bool> {
        if self.initialization.is_initialized() {
            return Ok(true);
        }

        if self.routes.is_empty() {
            self.initialization.mark_initialized();
            return Ok(true);
        }

        if let Some(location) = self.synchronizer.location() {
            let results = self.apply_location_now(&location)?;
            let url_region = self.url_region().to_string();
            if results.iter().any(|result| {
                result.region == url_region && result.status == RoutingStatus::Success
            }) {
                self.initialization.mark_initialized();
                return Ok(true);
            }
        }

        let result = self.navigate_now(NavigationRequest {
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

fn has_errors(status: RoutingStatus) -> bool {
    matches!(
        status,
        RoutingStatus::FailedNotFound
            | RoutingStatus::SkippedExternalRoute
            | RoutingStatus::ComponentNotFound
    )
}

/// A command's answer: RayX's are ready at once.
fn answer<T: Send + 'static>(value: T) -> BoxFuture<'static, T> {
    futures::future::ready(value).boxed()
}

impl NavigationService for RayxNavigation {
    fn navigation(&self) -> &StateCell<NavigationState> {
        &self.navigation
    }

    fn events(&self) -> &EventStream<NavigationEvent> {
        &self.events
    }

    fn root_region_name(&self) -> String {
        self.options.root_region_name.clone()
    }

    fn url_region(&self) -> String {
        RayxNavigation::url_region(self).to_owned()
    }

    fn default_history_mode(&self) -> NavigationHistoryMode {
        self.options.default_history_mode
    }

    fn navigate(&self, request: NavigationRequest) -> BoxFuture<'_, Result<NavigationResult>> {
        answer(self.navigate_now(request))
    }

    fn back(&self, region: String) -> BoxFuture<'_, Result<NavigationResult>> {
        answer(self.back_now(&region))
    }

    fn forward(&self, region: String) -> BoxFuture<'_, Result<NavigationResult>> {
        answer(self.forward_now(&region))
    }

    fn refresh(&self, region: String) -> BoxFuture<'_, Result<NavigationResult>> {
        answer(self.refresh_now(&region))
    }

    fn go_to_entry(&self, region: String, index: usize) -> BoxFuture<'_, Result<NavigationResult>> {
        answer(self.go_to_entry_now(&region, index))
    }

    fn ensure_region(&self, region: String) -> BoxFuture<'_, Result<()>> {
        answer(self.ensure_region_now(&region))
    }

    fn close_region(&self, region: String) -> BoxFuture<'_, Result<bool>> {
        answer(self.close_region_now(&region))
    }

    fn remove_region(&self, region: String) -> BoxFuture<'_, Result<bool>> {
        answer(self.remove_region_now(&region))
    }

    fn apply_location(&self, location: String) -> BoxFuture<'_, Result<Vec<NavigationResult>>> {
        answer(self.apply_location_now(&location))
    }
}

/// The service's startup role: it goes to the platform's location (a deep link), else to the
/// default route, when the app starts.
pub(crate) struct Starting(pub(crate) Arc<RayxNavigation>);

impl AsyncInitializable for Starting {
    fn is_initialized(&self) -> bool {
        self.0.initialization.is_initialized()
    }

    fn initialize(
        &self,
        _context: StartupInitializationContext,
    ) -> LocalBoxFuture<'static, StartupInitializationResult> {
        let service = Arc::clone(&self.0);
        async move {
            if service
                .initialize_now()
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

impl StartupInitializable for Starting {
    fn startup_service_type(&self) -> Option<StartupServiceType> {
        Some(StartupServiceType::of::<dyn NavigationService>())
    }
}
