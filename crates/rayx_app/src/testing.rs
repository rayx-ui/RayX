//! Fakes of the core's service contracts, NSH-free, for tests: each implements its trait over
//! what the test gives it and records what it is asked, and [`add_host_fakes`] registers the
//! services an NSH host requires.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, PoisonError};

use futures::FutureExt as _;
use futures::future::BoxFuture;

use crate::published::{EventStream, StateCell};
use crate::services::app::AppBuilder;
use crate::services::assets::{AssetCatalog, AssetError, AssetService};
use crate::services::fonts::{FontService, FontServiceError, FontState, RegisteredFontFace};
use crate::services::navigation::{
    LocationChangedEvent, NavigationDirection, NavigationError, NavigationEvent,
    NavigationHistoryMode, NavigationJournal, NavigationLifecycleData, NavigationRegion,
    NavigationRequest, NavigationResult, NavigationService, NavigationState, Route,
    RouteChangedEvent, RouteParams, RoutingData, RoutingStatus,
};
use crate::services::platform::{PlatformInfoService, PlatformSnapshot};
use crate::services::storage::{KeyValueStore, StorageCapabilities, StorageError};
use crate::services::suspension::{
    SuspensionError, SuspensionEvent, SuspensionEventKind, SuspensionEventReason,
    SuspensionService, SuspensionSnapshot,
};

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A platform service publishing the snapshot the test gives it.
pub struct FakePlatform {
    platform: StateCell<PlatformSnapshot>,
}

impl FakePlatform {
    /// A platform at `snapshot`.
    pub fn new(snapshot: PlatformSnapshot) -> Arc<Self> {
        Arc::new(Self {
            platform: StateCell::new(snapshot),
        })
    }

    /// Publishes `snapshot`, as a platform change would.
    pub fn publish(&self, snapshot: PlatformSnapshot) {
        self.platform.publish(snapshot);
    }
}

impl PlatformInfoService for FakePlatform {
    fn platform(&self) -> &StateCell<PlatformSnapshot> {
        &self.platform
    }
}

/// A suspension service the test drives with [`FakeSuspension::emit`]; it records the quits
/// asked of it, and asking before quitting changes its state.
#[derive(Default)]
pub struct FakeSuspension {
    suspension: StateCell<SuspensionSnapshot>,
    events: EventStream<SuspensionEvent>,
    quits: Mutex<usize>,
}

impl FakeSuspension {
    pub fn new() -> Arc<Self> {
        Arc::default()
    }

    /// Publishes `event` and the state it leads to.
    pub fn emit(&self, event: SuspensionEvent) {
        self.suspension.publish(self.suspension.get().apply(&event));
        self.events.publish(event);
    }

    /// How many times the app asked to quit.
    pub fn quits(&self) -> usize {
        *lock(&self.quits)
    }
}

impl SuspensionService for FakeSuspension {
    fn suspension(&self) -> &StateCell<SuspensionSnapshot> {
        &self.suspension
    }

    fn events(&self) -> &EventStream<SuspensionEvent> {
        &self.events
    }

    fn quit(&self) -> BoxFuture<'_, Result<(), SuspensionError>> {
        *lock(&self.quits) += 1;
        futures::future::ready(Ok(())).boxed()
    }

    fn ask_before_quit(&self, ask: bool) -> BoxFuture<'_, Result<(), SuspensionError>> {
        self.suspension.publish(SuspensionSnapshot {
            asks_before_quit: ask,
            ..*self.suspension.get()
        });
        futures::future::ready(Ok(())).boxed()
    }
}

impl FakeSuspension {
    /// Publishes that the app started.
    pub fn start(&self) {
        self.emit(SuspensionEvent::new(
            SuspensionEventKind::Started,
            SuspensionEventReason::HostStarted,
        ));
    }
}

/// An asset service over a catalog and files the test gives it (by logical path,
/// `@<package>/<path>`); its catalog is loaded from the start.
pub struct FakeAssets {
    catalog: StateCell<Option<AssetCatalog>>,
    files: Mutex<BTreeMap<String, Vec<u8>>>,
    cleared: Mutex<usize>,
}

impl FakeAssets {
    /// Assets with `catalog` and no files.
    pub fn new(catalog: AssetCatalog) -> Arc<Self> {
        Arc::new(Self {
            catalog: StateCell::new(Some(catalog)),
            files: Mutex::default(),
            cleared: Mutex::default(),
        })
    }

    /// Adds the file at `path` (a logical path) with `bytes`.
    pub fn file(self: Arc<Self>, path: &str, bytes: impl Into<Vec<u8>>) -> Arc<Self> {
        lock(&self.files).insert(path.to_owned(), bytes.into());
        self
    }

    /// How many times the cache was cleared.
    pub fn cleared(&self) -> usize {
        *lock(&self.cleared)
    }

    fn bytes(&self, path: &str) -> Result<Vec<u8>, AssetError> {
        let files = lock(&self.files);
        if let Some(bytes) = files.get(path) {
            return Ok(bytes.clone());
        }
        let logical = (*self.catalog.get())
            .as_ref()
            .and_then(|catalog| catalog.get(path).map(|asset| asset.logical_path()));
        logical
            .and_then(|logical| files.get(&logical).cloned())
            .ok_or_else(|| AssetError::NotFound(path.to_owned()))
    }
}

impl AssetService for FakeAssets {
    fn catalog(&self) -> &StateCell<Option<AssetCatalog>> {
        &self.catalog
    }

    fn load_catalog(&self) -> BoxFuture<'_, Result<(), AssetError>> {
        futures::future::ready(Ok(())).boxed()
    }

    fn load_bytes(&self, path: String) -> BoxFuture<'_, Result<Vec<u8>, AssetError>> {
        futures::future::ready(self.bytes(&path)).boxed()
    }

    fn load_text(&self, path: String) -> BoxFuture<'_, Result<String, AssetError>> {
        let text = self.bytes(&path).and_then(|bytes| {
            String::from_utf8(bytes).map_err(|error| AssetError::Decode {
                path,
                expected: "UTF-8 text",
                reason: error.to_string(),
            })
        });
        futures::future::ready(text).boxed()
    }

    fn clear_cache(&self) {
        *lock(&self.cleared) += 1;
    }
}

/// A font service publishing the state the test gives it; a registered font's family is the
/// one the test names with [`FakeFonts::family`], `Font <n>` otherwise.
pub struct FakeFonts {
    fonts: StateCell<FontState>,
    families: Mutex<Vec<String>>,
    registered: Mutex<Vec<Vec<u8>>>,
}

impl FakeFonts {
    pub fn new(state: FontState) -> Arc<Self> {
        Arc::new(Self {
            fonts: StateCell::new(state),
            families: Mutex::default(),
            registered: Mutex::default(),
        })
    }

    /// The family the next registered font has.
    pub fn family(self: Arc<Self>, family: &str) -> Arc<Self> {
        lock(&self.families).push(family.to_owned());
        self
    }

    /// The data of the fonts registered, in order.
    pub fn registered(&self) -> Vec<Vec<u8>> {
        lock(&self.registered).clone()
    }
}

impl FontService for FakeFonts {
    fn fonts(&self) -> &StateCell<FontState> {
        &self.fonts
    }

    fn register_font(&self, data: Vec<u8>) -> BoxFuture<'_, Result<String, FontServiceError>> {
        let count = {
            let mut registered = lock(&self.registered);
            registered.push(data);
            registered.len()
        };
        let family = {
            let mut families = lock(&self.families);
            if families.is_empty() {
                format!("Font {count}")
            } else {
                families.remove(0)
            }
        };
        let mut state = (*self.fonts.get()).clone();
        state.faces.push(RegisteredFontFace {
            logical_path: None,
            family: family.clone(),
        });
        self.fonts.publish(state);
        futures::future::ready(Ok(family)).boxed()
    }

    fn register_font_assets(&self) -> BoxFuture<'_, Result<(), FontServiceError>> {
        futures::future::ready(Ok(())).boxed()
    }
}

/// A key-value store in memory (values live as long as the store), reporting the capabilities
/// the test gives it ([`FakeKeyValueStore::with_capabilities`]; none by default).
#[derive(Default)]
pub struct FakeKeyValueStore {
    capabilities: StorageCapabilities,
    buckets: Mutex<BTreeMap<String, BTreeMap<String, Vec<u8>>>>,
}

impl FakeKeyValueStore {
    pub fn new() -> Arc<Self> {
        Arc::default()
    }

    /// An empty store reporting `capabilities`.
    pub fn with_capabilities(capabilities: StorageCapabilities) -> Arc<Self> {
        Arc::new(Self {
            capabilities,
            buckets: Mutex::default(),
        })
    }

    fn with<R>(
        &self,
        f: impl FnOnce(&mut BTreeMap<String, BTreeMap<String, Vec<u8>>>) -> R,
    ) -> BoxFuture<'_, Result<R, StorageError>>
    where
        R: Send + 'static,
    {
        futures::future::ready(Ok(f(&mut lock(&self.buckets)))).boxed()
    }
}

impl KeyValueStore for FakeKeyValueStore {
    fn capabilities(&self) -> StorageCapabilities {
        self.capabilities
    }

    fn get(
        &self,
        bucket: String,
        key: String,
    ) -> BoxFuture<'_, Result<Option<Vec<u8>>, StorageError>> {
        self.with(move |buckets| {
            buckets
                .get(&bucket)
                .and_then(|values| values.get(&key))
                .cloned()
        })
    }

    fn set(
        &self,
        bucket: String,
        key: String,
        value: Vec<u8>,
    ) -> BoxFuture<'_, Result<(), StorageError>> {
        self.with(move |buckets| {
            buckets.entry(bucket).or_default().insert(key, value);
        })
    }

    fn remove(&self, bucket: String, key: String) -> BoxFuture<'_, Result<bool, StorageError>> {
        self.with(move |buckets| {
            buckets
                .get_mut(&bucket)
                .is_some_and(|values| values.remove(&key).is_some())
        })
    }

    fn keys(&self, bucket: String) -> BoxFuture<'_, Result<Vec<String>, StorageError>> {
        self.with(move |buckets| {
            buckets
                .get(&bucket)
                .map(|values| values.keys().cloned().collect())
                .unwrap_or_default()
        })
    }

    fn clear(&self, bucket: String) -> BoxFuture<'_, Result<(), StorageError>> {
        self.with(move |buckets| {
            buckets.remove(&bucket);
        })
    }
}

/// A navigation service whose every URI is a route (`/projects/1` matches itself, with no
/// parameters): it keeps the regions' journals as RayX's does, publishes the state and the
/// events, and records the navigations it is asked. Its location is the root region's route;
/// the root region, `root`, is the URL region.
pub struct FakeNavigation {
    navigation: StateCell<NavigationState>,
    events: EventStream<NavigationEvent>,
    requests: Mutex<Vec<NavigationRequest>>,
}

impl FakeNavigation {
    pub fn new() -> Arc<Self> {
        let mut state = NavigationState {
            root_region: "root".to_owned(),
            location: "/".to_owned(),
            ..NavigationState::default()
        };
        state
            .regions
            .insert("root".to_owned(), NavigationRegion::new("root", 40));
        Arc::new(Self {
            navigation: StateCell::new(state),
            events: EventStream::new(),
            requests: Mutex::default(),
        })
    }

    /// The navigations asked of it, in order.
    pub fn requests(&self) -> Vec<NavigationRequest> {
        lock(&self.requests).clone()
    }

    /// The route the fake gives `uri`: its path, query and fragment, split.
    pub fn route(uri: &str) -> Route {
        let (without_fragment, fragment) = uri.split_once('#').unwrap_or((uri, ""));
        let (path, query) = without_fragment
            .split_once('?')
            .unwrap_or((without_fragment, ""));
        Route {
            uri: uri.to_owned(),
            base_uri: "app://".to_owned(),
            path: path.to_owned(),
            query: query.to_owned(),
            fragment: fragment.to_owned(),
            location: uri.to_owned(),
            segments: path.split('/').map(ToOwned::to_owned).collect(),
            params: RouteParams::default(),
            query_params: RouteParams::default(),
            passed_parameters: None,
            component: None,
            pattern: path.to_owned(),
            prewarm: false,
        }
    }

    /// Takes `region` to `route` (recorded in its journal with `mode`), publishing the state,
    /// then the events.
    fn go(
        &self,
        region: &str,
        route: Route,
        mode: NavigationHistoryMode,
        direction: NavigationDirection,
    ) -> NavigationResult {
        let mut state = (*self.navigation.get()).clone();
        let entry = state
            .regions
            .entry(region.to_owned())
            .or_insert_with(|| NavigationRegion::new(region, 40));
        if direction == NavigationDirection::New
            && entry
                .current_route()
                .is_some_and(|current| current.uri == route.uri)
        {
            return NavigationResult {
                region: region.to_owned(),
                direction,
                status: RoutingStatus::SkippedSameRoute,
                route,
            };
        }
        let data = RoutingData {
            status: RoutingStatus::Success,
            route: route.clone(),
        };
        let previous = entry.current.replace(data.clone());
        entry.journal.record(route.clone(), mode);
        if region == state.root_region {
            state.location = route.location.clone();
        }
        self.navigation.publish(state);
        let lifecycle = NavigationLifecycleData {
            region: region.to_owned(),
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
                region: region.to_owned(),
                from: previous.map(|previous| previous.route),
                to: route.clone(),
            }));
        self.events
            .publish(NavigationEvent::LocationChanged(LocationChangedEvent {
                region: region.to_owned(),
                data,
            }));
        NavigationResult {
            region: region.to_owned(),
            direction,
            status: RoutingStatus::Success,
            route,
        }
    }

    /// Moves `region`'s journal with `step` and goes to the entry it gives.
    fn step(
        &self,
        region: &str,
        direction: NavigationDirection,
        step: impl FnOnce(&mut NavigationJournal) -> Option<Route>,
    ) -> Result<NavigationResult, NavigationError> {
        let mut state = (*self.navigation.get()).clone();
        let Some(entry) = state.regions.get_mut(region) else {
            return Err(NavigationError::Location(format!("no region {region}")));
        };
        let Some(route) = step(&mut entry.journal) else {
            let route = entry
                .current_route()
                .cloned()
                .unwrap_or_else(|| Self::route("/"));
            return Ok(NavigationResult {
                region: region.to_owned(),
                direction,
                status: RoutingStatus::SkippedNoHistory,
                route,
            });
        };
        self.navigation.publish(state);
        Ok(self.go(region, route, NavigationHistoryMode::None, direction))
    }
}

fn ready<T: Send + 'static>(value: T) -> BoxFuture<'static, T> {
    futures::future::ready(value).boxed()
}

impl NavigationService for FakeNavigation {
    fn navigation(&self) -> &StateCell<NavigationState> {
        &self.navigation
    }

    fn events(&self) -> &EventStream<NavigationEvent> {
        &self.events
    }

    fn root_region_name(&self) -> String {
        "root".to_owned()
    }

    fn url_region(&self) -> String {
        "root".to_owned()
    }

    fn default_history_mode(&self) -> NavigationHistoryMode {
        NavigationHistoryMode::Push
    }

    fn navigate(
        &self,
        request: NavigationRequest,
    ) -> BoxFuture<'_, Result<NavigationResult, NavigationError>> {
        lock(&self.requests).push(request.clone());
        let route = Self::route(&request.args.uri);
        ready(Ok(self.go(
            &request.region,
            route,
            request.args.history_mode,
            NavigationDirection::New,
        )))
    }

    fn back(&self, region: String) -> BoxFuture<'_, Result<NavigationResult, NavigationError>> {
        ready(self.step(
            &region,
            NavigationDirection::Back,
            NavigationJournal::try_go_back,
        ))
    }

    fn forward(&self, region: String) -> BoxFuture<'_, Result<NavigationResult, NavigationError>> {
        ready(self.step(
            &region,
            NavigationDirection::Forward,
            NavigationJournal::try_go_forward,
        ))
    }

    fn refresh(&self, region: String) -> BoxFuture<'_, Result<NavigationResult, NavigationError>> {
        ready(self.step(&region, NavigationDirection::Refresh, |journal| {
            journal.current().cloned()
        }))
    }

    fn go_to_entry(
        &self,
        region: String,
        index: usize,
    ) -> BoxFuture<'_, Result<NavigationResult, NavigationError>> {
        let result = self.step(&region, NavigationDirection::Back, |journal| {
            journal.go_to(index)
        });
        ready(result.and_then(|result| {
            if result.status == RoutingStatus::SkippedNoHistory {
                Err(NavigationError::Location(format!(
                    "region {region} has no journal entry {index}"
                )))
            } else {
                Ok(result)
            }
        }))
    }

    fn ensure_region(&self, region: String) -> BoxFuture<'_, Result<(), NavigationError>> {
        if self.navigation.get().region(&region).is_none() {
            let mut state = (*self.navigation.get()).clone();
            state
                .regions
                .insert(region.clone(), NavigationRegion::new(region, 40));
            self.navigation.publish(state);
        }
        ready(Ok(()))
    }

    fn close_region(&self, region: String) -> BoxFuture<'_, Result<bool, NavigationError>> {
        let mut state = (*self.navigation.get()).clone();
        let open = region != state.root_region
            && state.regions.get_mut(&region).is_some_and(|entry| {
                let open = entry.current.take().is_some();
                entry.journal.clear();
                open
            });
        if open {
            self.navigation.publish(state);
            self.events
                .publish(NavigationEvent::RegionClosed { region });
        }
        ready(Ok(open))
    }

    fn remove_region(&self, region: String) -> BoxFuture<'_, Result<bool, NavigationError>> {
        let mut state = (*self.navigation.get()).clone();
        let removed = region != state.root_region && state.regions.remove(&region).is_some();
        if removed {
            self.navigation.publish(state);
        }
        ready(Ok(removed))
    }

    fn apply_location(
        &self,
        location: String,
    ) -> BoxFuture<'_, Result<Vec<NavigationResult>, NavigationError>> {
        let route = Self::route(&location);
        ready(Ok(vec![self.go(
            "root",
            route,
            NavigationHistoryMode::Push,
            NavigationDirection::New,
        )]))
    }
}

/// Registers fakes of the services an NSH host requires (platform, suspension, assets and
/// fonts, at their defaults) where the app registered none.
pub fn add_host_fakes<B: AppBuilder>(mut builder: B) -> B {
    let services = builder.services_mut();
    if !services.contains_service::<dyn PlatformInfoService>() {
        services.add_singleton_as::<dyn PlatformInfoService>(FakePlatform::new(
            PlatformSnapshot::default(),
        )
            as Arc<dyn PlatformInfoService>);
    }
    if !services.contains_service::<dyn SuspensionService>() {
        services.add_singleton_as::<dyn SuspensionService>(
            FakeSuspension::new() as Arc<dyn SuspensionService>
        );
    }
    if !services.contains_service::<dyn AssetService>() {
        services.add_singleton_as::<dyn AssetService>(
            FakeAssets::new(AssetCatalog::default()) as Arc<dyn AssetService>
        );
    }
    if !services.contains_service::<dyn FontService>() {
        services.add_singleton_as::<dyn FontService>(
            FakeFonts::new(FontState::default()) as Arc<dyn FontService>
        );
    }
    builder
}
