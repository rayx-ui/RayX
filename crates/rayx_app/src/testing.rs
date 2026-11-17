//! Fakes of the core's service contracts, NSH-free, for tests: a fake per trait (and of the
//! Rust-only [`MessageFormatter`]), each implementing it over what the test gives it, publishing
//! its state through the trait's cells and recording or answering the commands it is asked;
//! [`add_host_fakes`] registers the services an NSH host requires. Register one as the trait
//! (`services.add_singleton_as::<dyn PlatformInfoService>(fake as Arc<dyn PlatformInfoService>)`)
//! and every consumer of the trait uses it.
//!
//! A fake changes its state under a lock of its own and publishes it after, outside the lock (a
//! subscriber may call the fake back), one thread at a time: a change made meanwhile is published
//! right after, by whichever thread is publishing, and only a real change is published.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use futures::FutureExt as _;
use futures::future::BoxFuture;

use crate::published::{EventStream, StateCell};
use crate::services::app::AppBuilder;
use crate::services::application_configuration::{
    ApplicationConfiguration, ApplicationConfigurationError, ApplicationConfigurationService,
    ApplicationConfigurationSnapshot,
};
use crate::services::assets::{AssetCatalog, AssetError, AssetService};
use crate::services::fonts::{FontService, FontServiceError, FontState, RegisteredFontFace};
use crate::services::history::{
    HistoryError, HistoryEvent, HistoryScopeId, HistoryScopeState, HistoryService, HistoryState,
};
use crate::services::i18n::{
    I18nService, I18nServiceError, I18nState, MessageBundle, MessageFormatter,
};
use crate::services::input_map::{
    InputMapApplyError, InputMapDocument, InputMapError, InputMapService, InputMapSource,
    InputMapState,
};
use crate::services::navigation::{
    LocationChangedEvent, NavigationDirection, NavigationError, NavigationEvent,
    NavigationHistoryMode, NavigationJournal, NavigationLifecycleData, NavigationRegion,
    NavigationRequest, NavigationResult, NavigationService, NavigationState, Route,
    RouteChangedEvent, RouteParams, RoutingData, RoutingStatus,
};
use crate::services::operations::{
    OperationApplyStatus, OperationError, OperationEvent, OperationFailure, OperationOrigin,
    OperationReceipt, OperationRecord, OperationService, OperationState, OperationTargetHandle,
    OperationTargetId,
};
use crate::services::platform::{PlatformInfoService, PlatformSnapshot, TextDirection};
use crate::services::selection::{
    SelectionAreaId, SelectionError, SelectionService, SelectionState, SelectionStatus,
};
use crate::services::storage::{KeyValueStore, StorageCapabilities, StorageError};
use crate::services::suspension::{
    SuspensionError, SuspensionEvent, SuspensionEventKind, SuspensionEventReason,
    SuspensionService, SuspensionSnapshot,
};

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A fake's published state: the value changed under its lock, then published outside it.
struct FakeState<T> {
    value: Mutex<T>,
    cell: StateCell<T>,
    /// Held while the value is published; a change meanwhile marks it stale.
    publishing: Mutex<()>,
    stale: AtomicBool,
}

impl<T: Clone + PartialEq + Send + Sync + 'static> FakeState<T> {
    fn new(value: T) -> Self {
        Self {
            cell: StateCell::new(value.clone()),
            value: Mutex::new(value),
            publishing: Mutex::new(()),
            stale: AtomicBool::new(false),
        }
    }

    fn cell(&self) -> &StateCell<T> {
        &self.cell
    }

    /// Changes the value with `change` (under the lock, so changes never overwrite each
    /// other), then publishes it; what `change` returns.
    fn change<R>(&self, change: impl FnOnce(&mut T) -> R) -> R {
        let result = change(&mut lock(&self.value));
        self.publish();
        result
    }

    /// Replaces the value with `value` and publishes it.
    fn set(&self, value: T) {
        self.change(|current| *current = value);
    }

    /// Publishes the value unless it is the one published: a change made meanwhile is published
    /// right after, by whoever is publishing.
    fn publish(&self) {
        self.stale.store(true, Ordering::SeqCst);
        loop {
            let publishing = match self.publishing.try_lock() {
                Ok(guard) => guard,
                Err(std::sync::TryLockError::Poisoned(poisoned)) => poisoned.into_inner(),
                Err(std::sync::TryLockError::WouldBlock) => return,
            };
            self.stale.store(false, Ordering::SeqCst);
            let value = lock(&self.value).clone();
            if *self.cell.get() != value {
                self.cell.publish(value);
            }
            drop(publishing);
            if !self.stale.load(Ordering::SeqCst) {
                return;
            }
        }
    }
}

impl<T: Clone + Default + PartialEq + Send + Sync + 'static> Default for FakeState<T> {
    fn default() -> Self {
        Self::new(T::default())
    }
}

/// A platform service publishing the snapshot the test gives it.
pub struct FakePlatform {
    platform: FakeState<PlatformSnapshot>,
}

impl FakePlatform {
    /// A platform at `snapshot`.
    pub fn new(snapshot: PlatformSnapshot) -> Arc<Self> {
        Arc::new(Self {
            platform: FakeState::new(snapshot),
        })
    }

    /// Publishes `snapshot`, as a platform change would.
    pub fn publish(&self, snapshot: PlatformSnapshot) {
        self.platform.set(snapshot);
    }

    /// Changes the snapshot with `change` and publishes it.
    pub fn modify(&self, change: impl FnOnce(&mut PlatformSnapshot)) {
        self.platform.change(change);
    }
}

impl PlatformInfoService for FakePlatform {
    fn platform(&self) -> &StateCell<PlatformSnapshot> {
        self.platform.cell()
    }
}

/// A suspension service the test drives with [`FakeSuspension::emit`]; it records the quits
/// asked of it, and asking before quitting changes its state.
#[derive(Default)]
pub struct FakeSuspension {
    suspension: FakeState<SuspensionSnapshot>,
    events: EventStream<SuspensionEvent>,
    quits: Mutex<usize>,
}

impl FakeSuspension {
    pub fn new() -> Arc<Self> {
        Arc::default()
    }

    /// Publishes the state `event` leads to, then `event`.
    pub fn emit(&self, event: SuspensionEvent) {
        self.suspension.change(|state| *state = state.apply(&event));
        self.events.publish(event);
    }

    /// Publishes that the app started.
    pub fn start(&self) {
        self.emit(SuspensionEvent::new(
            SuspensionEventKind::Started,
            SuspensionEventReason::HostStarted,
        ));
    }

    /// How many times the app asked to quit.
    pub fn quits(&self) -> usize {
        *lock(&self.quits)
    }
}

impl SuspensionService for FakeSuspension {
    fn suspension(&self) -> &StateCell<SuspensionSnapshot> {
        self.suspension.cell()
    }

    fn events(&self) -> &EventStream<SuspensionEvent> {
        &self.events
    }

    fn quit(&self) -> BoxFuture<'_, Result<(), SuspensionError>> {
        *lock(&self.quits) += 1;
        futures::future::ready(Ok(())).boxed()
    }

    fn ask_before_quit(&self, ask: bool) -> BoxFuture<'_, Result<(), SuspensionError>> {
        self.suspension.change(|state| state.asks_before_quit = ask);
        futures::future::ready(Ok(())).boxed()
    }
}

/// An asset service over a catalog and files the test gives it (by logical path,
/// `@<package>/<path>`); its catalog is loaded from the start. It records the paths it is asked
/// to load and counts the catalog's loads and the cache's clears.
pub struct FakeAssets {
    catalog: FakeState<Option<AssetCatalog>>,
    files: Mutex<BTreeMap<String, Vec<u8>>>,
    loads: Mutex<Vec<String>>,
    catalog_loads: Mutex<usize>,
    cleared: Mutex<usize>,
}

impl FakeAssets {
    /// Assets with `catalog` and no files.
    pub fn new(catalog: AssetCatalog) -> Arc<Self> {
        Arc::new(Self {
            catalog: FakeState::new(Some(catalog)),
            files: Mutex::default(),
            loads: Mutex::default(),
            catalog_loads: Mutex::default(),
            cleared: Mutex::default(),
        })
    }

    /// Adds the file at `path` (a logical path) with `bytes`.
    pub fn file(self: Arc<Self>, path: &str, bytes: impl Into<Vec<u8>>) -> Arc<Self> {
        lock(&self.files).insert(path.to_owned(), bytes.into());
        self
    }

    /// Publishes `catalog`, as a catalog loaded again would.
    pub fn publish(&self, catalog: AssetCatalog) {
        self.catalog.set(Some(catalog));
    }

    /// The paths asked of `load_bytes` and `load_text`, in order.
    pub fn loads(&self) -> Vec<String> {
        lock(&self.loads).clone()
    }

    /// How many times the catalog was loaded.
    pub fn catalog_loads(&self) -> usize {
        *lock(&self.catalog_loads)
    }

    /// How many times the cache was cleared.
    pub fn cleared(&self) -> usize {
        *lock(&self.cleared)
    }

    fn bytes(&self, path: &str) -> Result<Vec<u8>, AssetError> {
        lock(&self.loads).push(path.to_owned());
        let files = lock(&self.files);
        if let Some(bytes) = files.get(path) {
            return Ok(bytes.clone());
        }
        let logical = (*self.catalog.cell().get())
            .as_ref()
            .and_then(|catalog| catalog.get(path).map(|asset| asset.logical_path()));
        logical
            .and_then(|logical| files.get(&logical).cloned())
            .ok_or_else(|| AssetError::NotFound(path.to_owned()))
    }
}

impl AssetService for FakeAssets {
    fn catalog(&self) -> &StateCell<Option<AssetCatalog>> {
        self.catalog.cell()
    }

    fn load_catalog(&self) -> BoxFuture<'_, Result<(), AssetError>> {
        *lock(&self.catalog_loads) += 1;
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
    fonts: FakeState<FontState>,
    families: Mutex<Vec<String>>,
    registered: Mutex<Vec<Vec<u8>>>,
    asset_registrations: Mutex<usize>,
}

impl FakeFonts {
    pub fn new(state: FontState) -> Arc<Self> {
        Arc::new(Self {
            fonts: FakeState::new(state),
            families: Mutex::default(),
            registered: Mutex::default(),
            asset_registrations: Mutex::default(),
        })
    }

    /// Publishes `state`, as fonts registered elsewhere would.
    pub fn publish(&self, state: FontState) {
        self.fonts.set(state);
    }

    /// How many times the app asked to register the catalog's fonts.
    pub fn asset_registrations(&self) -> usize {
        *lock(&self.asset_registrations)
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
        self.fonts.cell()
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
        self.fonts.change(|state| {
            state.faces.push(RegisteredFontFace {
                logical_path: None,
                family: family.clone(),
            });
        });
        futures::future::ready(Ok(family)).boxed()
    }

    fn register_font_assets(&self) -> BoxFuture<'_, Result<(), FontServiceError>> {
        *lock(&self.asset_registrations) += 1;
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
    navigation: FakeState<NavigationState>,
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
            navigation: FakeState::new(state),
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

    /// Takes `region` to `route` in `state` (recorded in its journal with `mode`): the result
    /// and the events to publish once the state is.
    fn go(
        state: &mut NavigationState,
        region: &str,
        route: Route,
        mode: NavigationHistoryMode,
        direction: NavigationDirection,
    ) -> (NavigationResult, Vec<NavigationEvent>) {
        let is_root = region == state.root_region;
        let entry = state
            .regions
            .entry(region.to_owned())
            .or_insert_with(|| NavigationRegion::new(region, 40));
        if direction == NavigationDirection::New
            && entry
                .current_route()
                .is_some_and(|current| current.uri == route.uri)
        {
            let result = NavigationResult {
                region: region.to_owned(),
                direction,
                status: RoutingStatus::SkippedSameRoute,
                route,
            };
            return (result, Vec::new());
        }
        let data = RoutingData {
            status: RoutingStatus::Success,
            route: route.clone(),
        };
        let previous = entry.current.replace(data.clone());
        entry.journal.record(route.clone(), mode);
        if is_root {
            state.location = route.location.clone();
        }
        let lifecycle = NavigationLifecycleData {
            region: region.to_owned(),
            direction,
            from: previous.clone(),
            to: data.clone(),
        };
        let mut events = Vec::new();
        if previous.is_some() {
            events.push(NavigationEvent::NavigatedFrom(lifecycle.clone()));
        }
        events.push(NavigationEvent::NavigatedTo(lifecycle));
        events.push(NavigationEvent::RouteChanged(RouteChangedEvent {
            region: region.to_owned(),
            from: previous.map(|previous| previous.route),
            to: route.clone(),
        }));
        events.push(NavigationEvent::LocationChanged(LocationChangedEvent {
            region: region.to_owned(),
            data,
        }));
        let result = NavigationResult {
            region: region.to_owned(),
            direction,
            status: RoutingStatus::Success,
            route,
        };
        (result, events)
    }

    /// Changes the state with `change`, publishes it, then the events `change` gave; the
    /// result `change` gave.
    fn navigate_with<R>(
        &self,
        change: impl FnOnce(&mut NavigationState) -> (R, Vec<NavigationEvent>),
    ) -> R {
        let (result, events) = self.navigation.change(change);
        for event in events {
            self.events.publish(event);
        }
        result
    }

    /// Moves `region`'s journal with `step` and goes to the entry it gives.
    fn step(
        &self,
        region: &str,
        direction: NavigationDirection,
        step: impl FnOnce(&mut NavigationJournal) -> Option<Route>,
    ) -> Result<NavigationResult, NavigationError> {
        self.navigate_with(|state| {
            let Some(entry) = state.regions.get_mut(region) else {
                let error = NavigationError::Location(format!("no region {region}"));
                return (Err(error), Vec::new());
            };
            let Some(route) = step(&mut entry.journal) else {
                let route = entry
                    .current_route()
                    .cloned()
                    .unwrap_or_else(|| Self::route("/"));
                let result = NavigationResult {
                    region: region.to_owned(),
                    direction,
                    status: RoutingStatus::SkippedNoHistory,
                    route,
                };
                return (Ok(result), Vec::new());
            };
            let (result, events) =
                Self::go(state, region, route, NavigationHistoryMode::None, direction);
            (Ok(result), events)
        })
    }
}

fn ready<T: Send + 'static>(value: T) -> BoxFuture<'static, T> {
    futures::future::ready(value).boxed()
}

impl NavigationService for FakeNavigation {
    fn navigation(&self) -> &StateCell<NavigationState> {
        self.navigation.cell()
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
        ready(Ok(self.navigate_with(|state| {
            Self::go(
                state,
                &request.region,
                route,
                request.args.history_mode,
                NavigationDirection::New,
            )
        })))
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
        self.navigation.change(|state| {
            state
                .regions
                .entry(region.clone())
                .or_insert_with(|| NavigationRegion::new(region, 40));
        });
        ready(Ok(()))
    }

    fn close_region(&self, region: String) -> BoxFuture<'_, Result<bool, NavigationError>> {
        let open = self.navigate_with(|state| {
            let open = region != state.root_region
                && state.regions.get_mut(&region).is_some_and(|entry| {
                    let open = entry.current.take().is_some();
                    entry.journal.clear();
                    open
                });
            let events = if open {
                vec![NavigationEvent::RegionClosed {
                    region: region.clone(),
                }]
            } else {
                Vec::new()
            };
            (open, events)
        });
        ready(Ok(open))
    }

    fn remove_region(&self, region: String) -> BoxFuture<'_, Result<bool, NavigationError>> {
        let removed = self
            .navigation
            .change(|state| region != state.root_region && state.regions.remove(&region).is_some());
        ready(Ok(removed))
    }

    fn apply_location(
        &self,
        location: String,
    ) -> BoxFuture<'_, Result<Vec<NavigationResult>, NavigationError>> {
        let route = Self::route(&location);
        let result = self.navigate_with(|state| {
            let root = state.root_region.clone();
            Self::go(
                state,
                &root,
                route,
                NavigationHistoryMode::Push,
                NavigationDirection::New,
            )
        });
        ready(Ok(vec![result]))
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

/// An operation service applying records through their targets' handlers as RayX's does
/// (transactions queue their events until they commit and undo their operations, newest first,
/// when they abort; a released target refuses operations), publishing its targets and every
/// event, without diagnostics. For testing what consumes operations (history) without RayX's.
pub struct FakeOperations {
    instance_id: u64,
    next_transaction: AtomicU64,
    state: Mutex<FakeOperationState>,
    operations: FakeState<OperationState>,
    events: EventStream<OperationEvent>,
}

#[derive(Default)]
struct FakeOperationState {
    targets: BTreeMap<OperationTargetId, OperationTargetHandle>,
    released: std::collections::BTreeSet<OperationTargetId>,
    /// The open transactions' receipts, with the handler that applied each.
    pending: BTreeMap<u64, Vec<(OperationReceipt, OperationTargetHandle)>>,
}

impl FakeOperations {
    pub fn new() -> Arc<Self> {
        /// Numbers from far above RayX's service's, so a test tells them apart.
        static NEXT_INSTANCE: AtomicU64 = AtomicU64::new(1 << 32);
        Arc::new(Self {
            instance_id: NEXT_INSTANCE.fetch_add(1, Ordering::Relaxed),
            next_transaction: AtomicU64::new(0),
            state: Mutex::default(),
            operations: FakeState::default(),
            events: EventStream::new(),
        })
    }

    fn next_transaction_id(&self) -> u64 {
        self.next_transaction.fetch_add(1, Ordering::SeqCst) + 1
    }

    /// Publishes the targets registered now (read under the state's lock, so the last change
    /// published has the latest).
    fn publish_targets(&self) {
        self.operations.change(|published| {
            published.targets = lock(&self.state).targets.keys().cloned().collect();
        });
    }

    fn target(
        &self,
        target_id: &OperationTargetId,
    ) -> Result<OperationTargetHandle, OperationError> {
        let state = lock(&self.state);
        match state.targets.get(target_id) {
            Some(target) => Ok(target.clone()),
            None if state.released.contains(target_id) => Err(OperationError::TargetReleased(
                target_id.as_str().to_owned(),
            )),
            None => Err(OperationError::UnknownTarget(target_id.as_str().to_owned())),
        }
    }

    fn submit_now(
        &self,
        record: OperationRecord,
        origin: OperationOrigin,
        transaction_id: Option<u64>,
    ) -> Result<OperationReceipt, OperationError> {
        let transaction_id = transaction_id.unwrap_or_else(|| self.next_transaction_id());
        let target = self.target(&record.target_id)?;
        let result = match target.apply(&record, &origin) {
            Ok(result) => result,
            Err(error) => {
                self.events
                    .publish(OperationEvent::Failed(OperationFailure {
                        transaction_id,
                        target_id: record.target_id.clone(),
                        kind: record.kind.clone(),
                        version: record.version,
                        origin,
                        error_code: error.code().to_owned(),
                    }));
                return Err(error);
            }
        };
        let receipt = OperationReceipt {
            transaction_id,
            target_id: record.target_id.clone(),
            forward: record,
            inverse: result.inverse,
            origin,
            status: result.status,
            affected_ids: result.affected_ids,
            before_revision: result.before_revision,
            after_revision: result.after_revision,
        };
        let queued = match lock(&self.state).pending.get_mut(&transaction_id) {
            Some(pending) => {
                pending.push((receipt.clone(), target));
                true
            }
            None => false,
        };
        if !queued {
            self.events
                .publish(OperationEvent::Committed(receipt.clone()));
        }
        Ok(receipt)
    }
}

impl OperationService for FakeOperations {
    fn operations(&self) -> &StateCell<OperationState> {
        self.operations.cell()
    }

    fn events(&self) -> &EventStream<OperationEvent> {
        &self.events
    }

    fn instance_id(&self) -> u64 {
        self.instance_id
    }

    fn register_target(
        &self,
        target_id: OperationTargetId,
        handler: OperationTargetHandle,
    ) -> BoxFuture<'_, Result<(), OperationError>> {
        {
            let mut state = lock(&self.state);
            if state.targets.contains_key(&target_id) {
                return ready(Err(OperationError::DuplicateTarget(
                    target_id.as_str().to_owned(),
                )));
            }
            state.released.remove(&target_id);
            state.targets.insert(target_id, handler);
        }
        self.publish_targets();
        ready(Ok(()))
    }

    fn release_target(
        &self,
        target_id: OperationTargetId,
    ) -> BoxFuture<'_, Result<bool, OperationError>> {
        let released = {
            let mut state = lock(&self.state);
            let released = state.targets.remove(&target_id).is_some();
            if released {
                state.released.insert(target_id);
            }
            released
        };
        self.publish_targets();
        ready(Ok(released))
    }

    fn begin(&self) -> BoxFuture<'_, Result<u64, OperationError>> {
        let transaction_id = self.next_transaction_id();
        lock(&self.state).pending.insert(transaction_id, Vec::new());
        ready(Ok(transaction_id))
    }

    fn commit(&self, transaction_id: u64) -> BoxFuture<'_, Result<(), OperationError>> {
        let Some(pending) = lock(&self.state).pending.remove(&transaction_id) else {
            return ready(Err(OperationError::TransactionClosed));
        };
        for (receipt, _) in pending {
            self.events.publish(OperationEvent::Committed(receipt));
        }
        ready(Ok(()))
    }

    fn abort(&self, transaction_id: u64) -> BoxFuture<'_, Result<(), OperationError>> {
        let Some(pending) = lock(&self.state).pending.remove(&transaction_id) else {
            return ready(Err(OperationError::TransactionClosed));
        };
        for (receipt, target) in pending.into_iter().rev() {
            if let Some(inverse) = &receipt.inverse
                && let Err(error) = target.apply(inverse, &OperationOrigin::HistoryUndo)
            {
                return ready(Err(error));
            }
        }
        ready(Ok(()))
    }

    fn submit_record(
        &self,
        record: OperationRecord,
        origin: OperationOrigin,
        transaction_id: Option<u64>,
    ) -> BoxFuture<'_, Result<OperationReceipt, OperationError>> {
        ready(self.submit_now(record, origin, transaction_id))
    }

    fn publish_remote_record(
        &self,
        record: OperationRecord,
        peer_id: String,
        before_revision: u64,
        after_revision: u64,
        affected_ids: Vec<String>,
    ) -> BoxFuture<'_, Result<OperationReceipt, OperationError>> {
        if let Err(error) = self.target(&record.target_id) {
            return ready(Err(error));
        }
        let receipt = OperationReceipt {
            transaction_id: self.next_transaction_id(),
            target_id: record.target_id.clone(),
            forward: record,
            inverse: None,
            origin: OperationOrigin::Remote(peer_id),
            status: OperationApplyStatus::Applied,
            affected_ids,
            before_revision,
            after_revision,
        };
        self.events
            .publish(OperationEvent::Committed(receipt.clone()));
        ready(Ok(receipt))
    }
}

/// A history whose scopes' depths the test sets ([`FakeHistory::set_depths`]): undo and redo
/// move one entry between a scope's stacks (replaying nothing), clearing and bounding a scope
/// change its depths, each publishing its state and event as RayX's history does; it records
/// the bindings it is asked. For testing what consumes the history (an undo button) without
/// RayX's.
#[derive(Default)]
pub struct FakeHistory {
    history: FakeState<HistoryState>,
    events: EventStream<HistoryEvent>,
    bindings: Mutex<Vec<(OperationTargetId, HistoryScopeId)>>,
}

impl FakeHistory {
    pub fn new() -> Arc<Self> {
        Arc::default()
    }

    /// Sets the depths of `scope` (a step of the user's elsewhere), publishing them.
    pub fn set_depths(&self, scope: &HistoryScopeId, undo: usize, redo: usize) {
        self.change(scope, |state| {
            state.undo_depth = undo;
            state.redo_depth = redo;
        });
    }

    /// The targets bound to scopes, in the order they were.
    pub fn bindings(&self) -> Vec<(OperationTargetId, HistoryScopeId)> {
        lock(&self.bindings).clone()
    }

    /// Changes `scope`'s state with `change`, then publishes the state; what `change`
    /// returns.
    fn change<R>(
        &self,
        scope: &HistoryScopeId,
        change: impl FnOnce(&mut HistoryScopeState) -> R,
    ) -> R {
        self.history.change(|state| {
            let entry = state.scopes.entry(scope.clone()).or_default();
            let result = change(entry);
            entry.revision += 1;
            result
        })
    }

    /// Moves one entry of `scope` from one stack to the other; whether there was one.
    fn step(&self, scope: HistoryScopeId, undo: bool) -> bool {
        let stepped = self.change(&scope, |state| {
            let (from, to) = if undo {
                (&mut state.undo_depth, &mut state.redo_depth)
            } else {
                (&mut state.redo_depth, &mut state.undo_depth)
            };
            if *from > 0 {
                *from -= 1;
                *to += 1;
                true
            } else {
                false
            }
        });
        if stepped {
            self.events.publish(if undo {
                HistoryEvent::UndoCompleted(scope)
            } else {
                HistoryEvent::RedoCompleted(scope)
            });
        }
        stepped
    }
}

impl HistoryService for FakeHistory {
    fn history(&self) -> &StateCell<HistoryState> {
        self.history.cell()
    }

    fn events(&self) -> &EventStream<HistoryEvent> {
        &self.events
    }

    fn bind_target_scope(
        &self,
        target_id: OperationTargetId,
        scope_id: HistoryScopeId,
    ) -> BoxFuture<'_, Result<(), HistoryError>> {
        lock(&self.bindings).push((target_id, scope_id));
        ready(Ok(()))
    }

    fn undo(&self, scope_id: HistoryScopeId) -> BoxFuture<'_, Result<bool, HistoryError>> {
        ready(Ok(self.step(scope_id, true)))
    }

    fn redo(&self, scope_id: HistoryScopeId) -> BoxFuture<'_, Result<bool, HistoryError>> {
        ready(Ok(self.step(scope_id, false)))
    }

    fn clear_scope(&self, scope_id: HistoryScopeId) -> BoxFuture<'_, Result<(), HistoryError>> {
        self.change(&scope_id, |state| {
            state.undo_depth = 0;
            state.redo_depth = 0;
        });
        self.events.publish(HistoryEvent::ScopeCleared(scope_id));
        ready(Ok(()))
    }

    fn set_scope_entry_limit(
        &self,
        scope_id: HistoryScopeId,
        limit: usize,
    ) -> BoxFuture<'_, Result<(), HistoryError>> {
        if limit == 0 {
            return ready(Err(HistoryError::InvalidEntryLimit));
        }
        self.change(&scope_id, |state| {
            state.entry_limit = Some(limit);
            let excess = (state.undo_depth + state.redo_depth).saturating_sub(limit);
            let undo_excess = excess.min(state.undo_depth);
            state.undo_depth -= undo_excess;
            state.redo_depth -= excess - undo_excess;
        });
        self.events
            .publish(HistoryEvent::AvailabilityChanged(scope_id));
        ready(Ok(()))
    }

    fn retained_records(
        &self,
        _scope_id: HistoryScopeId,
    ) -> BoxFuture<'_, Result<Vec<OperationRecord>, HistoryError>> {
        ready(Ok(Vec::new()))
    }
}

/// A selection command a [`FakeSelection`] was asked.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SelectionCall {
    PublishStatus(SelectionAreaId, SelectionStatus),
    Activate(SelectionAreaId, SelectionStatus),
    Deactivate(SelectionAreaId),
    Clear,
}

/// A selection service publishing the state the test gives it ([`FakeSelection::publish`]); it
/// records the commands asked of it and applies them to its state's area and status (only the
/// active area publishes or deactivates), leaving the revisions to the test.
#[derive(Default)]
pub struct FakeSelection {
    selection: FakeState<SelectionState>,
    calls: Mutex<Vec<SelectionCall>>,
}

impl FakeSelection {
    pub fn new() -> Arc<Self> {
        Arc::default()
    }

    /// Publishes `state`, as a selection change would.
    pub fn publish(&self, state: SelectionState) {
        self.selection.set(state);
    }

    /// The commands asked so far, in order.
    pub fn calls(&self) -> Vec<SelectionCall> {
        lock(&self.calls).clone()
    }

    /// Records `call` and publishes `area` and `status` when `applies` holds for the state.
    fn answer(
        &self,
        call: SelectionCall,
        applies: impl FnOnce(&SelectionState) -> bool,
        area: Option<SelectionAreaId>,
        status: SelectionStatus,
    ) -> BoxFuture<'_, Result<(), SelectionError>> {
        lock(&self.calls).push(call);
        self.selection.change(|state| {
            if applies(state) {
                state.active_area = area;
                state.status = status;
            }
        });
        ready(Ok(()))
    }
}

impl SelectionService for FakeSelection {
    fn selection(&self) -> &StateCell<SelectionState> {
        self.selection.cell()
    }

    fn publish_status(
        &self,
        active_area: SelectionAreaId,
        status: SelectionStatus,
    ) -> BoxFuture<'_, Result<(), SelectionError>> {
        self.answer(
            SelectionCall::PublishStatus(active_area, status),
            |state| state.active_area == Some(active_area),
            Some(active_area),
            status,
        )
    }

    fn activate(
        &self,
        active_area: SelectionAreaId,
        status: SelectionStatus,
    ) -> BoxFuture<'_, Result<(), SelectionError>> {
        self.answer(
            SelectionCall::Activate(active_area, status),
            |_| true,
            Some(active_area),
            status,
        )
    }

    fn deactivate(
        &self,
        active_area: SelectionAreaId,
    ) -> BoxFuture<'_, Result<(), SelectionError>> {
        self.answer(
            SelectionCall::Deactivate(active_area),
            |state| state.active_area == Some(active_area),
            None,
            SelectionStatus::default(),
        )
    }

    fn clear(&self) -> BoxFuture<'_, Result<(), SelectionError>> {
        self.answer(
            SelectionCall::Clear,
            |_| true,
            None,
            SelectionStatus::default(),
        )
    }
}

/// An input map publishing the documents the test gives it ([`FakeInputMap::new`]); `load`
/// answers the user's document as it is, and `apply_user_document` replaces it in memory, as
/// RayX's does (the revision checked, the candidate validated and given the next revision).
pub struct FakeInputMap {
    input_map: FakeState<InputMapState>,
    applied: Mutex<Vec<InputMapDocument>>,
    loads: Mutex<usize>,
}

impl FakeInputMap {
    /// An input map at `state`.
    pub fn new(state: InputMapState) -> Arc<Self> {
        Arc::new(Self {
            input_map: FakeState::new(state),
            applied: Mutex::default(),
            loads: Mutex::default(),
        })
    }

    /// Publishes `state`, as documents loaded elsewhere would.
    pub fn publish(&self, state: InputMapState) {
        self.input_map.set(state);
    }

    /// How many times the user's document was loaded.
    pub fn loads(&self) -> usize {
        *lock(&self.loads)
    }

    /// The user's documents applied so far, in order.
    pub fn applied(&self) -> Vec<InputMapDocument> {
        lock(&self.applied).clone()
    }
}

impl InputMapService for FakeInputMap {
    fn input_map(&self) -> &StateCell<InputMapState> {
        self.input_map.cell()
    }

    fn load(&self) -> BoxFuture<'_, Result<InputMapDocument, InputMapApplyError>> {
        *lock(&self.loads) += 1;
        futures::future::ready(Ok(self.input_map.cell().get().user.clone())).boxed()
    }

    fn apply_user_document(
        &self,
        expected_revision: u64,
        mut candidate: InputMapDocument,
    ) -> BoxFuture<'_, Result<InputMapDocument, InputMapApplyError>> {
        // Checked and applied under the state's lock, so two applications of one revision
        // never both succeed.
        let result = self.input_map.change(|state| {
            if candidate.source != InputMapSource::User {
                return Err(InputMapError::UserDocumentRequired.into());
            }
            if state.revision != expected_revision {
                return Err(InputMapApplyError::RevisionConflict {
                    expected: expected_revision,
                    actual: state.revision,
                });
            }
            candidate.revision = expected_revision + 1;
            candidate.validate()?;
            state.user = candidate.clone();
            state.revision = candidate.revision;
            lock(&self.applied).push(candidate.clone());
            Ok(candidate)
        });
        futures::future::ready(result).boxed()
    }
}

/// An application configuration publishing the snapshot the test gives it
/// ([`FakeApplicationConfiguration::new`]); `replace_options` takes valid settings as RayX's does
/// (refusing a changed name or description) and publishes them at the next revision.
pub struct FakeApplicationConfiguration {
    configuration: FakeState<ApplicationConfigurationSnapshot>,
    replaced: Mutex<Vec<ApplicationConfiguration>>,
}

impl FakeApplicationConfiguration {
    /// A configuration at `snapshot`.
    pub fn new(snapshot: ApplicationConfigurationSnapshot) -> Arc<Self> {
        Arc::new(Self {
            configuration: FakeState::new(snapshot),
            replaced: Mutex::default(),
        })
    }

    /// Publishes `snapshot`, as a change of the settings or the platform would.
    pub fn publish(&self, snapshot: ApplicationConfigurationSnapshot) {
        self.configuration.set(snapshot);
    }

    /// The settings `replace_options` was asked to take, in order.
    pub fn replaced(&self) -> Vec<ApplicationConfiguration> {
        lock(&self.replaced).clone()
    }
}

impl ApplicationConfigurationService for FakeApplicationConfiguration {
    fn configuration(&self) -> &StateCell<ApplicationConfigurationSnapshot> {
        self.configuration.cell()
    }

    fn replace_options(
        &self,
        options: ApplicationConfiguration,
    ) -> BoxFuture<'_, Result<bool, ApplicationConfigurationError>> {
        lock(&self.replaced).push(options.clone());
        let result = self.configuration.change(|snapshot| {
            if let Err(reason) = options.validate() {
                Err(ApplicationConfigurationError::InvalidOptions(reason))
            } else if snapshot.options.name != options.name {
                Err(ApplicationConfigurationError::StartupOnlyOptionChanged(
                    "name",
                ))
            } else if snapshot.options.description != options.description {
                Err(ApplicationConfigurationError::StartupOnlyOptionChanged(
                    "description",
                ))
            } else if snapshot.options == options {
                Ok(false)
            } else {
                snapshot.options = options;
                snapshot.revision += 1;
                Ok(true)
            }
        });
        futures::future::ready(result).boxed()
    }
}

/// An i18n command a [`FakeI18n`] was asked.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum I18nCall {
    SetLanguage(String),
    FollowPlatform,
    LoadMessages,
    AddResource {
        language: String,
        namespace: Option<String>,
        json: String,
    },
    SetTimeZone(String),
}

/// An i18n service publishing the state the test gives it ([`FakeI18n::new`],
/// [`FakeI18n::publish`]); it records the commands asked of it and applies them to its state as
/// RayX's does, under the next version: `set_language` picks a language that reads (its locale
/// and direction with it), `follow_platform` goes back to the default language (the fake follows
/// no platform), `add_resource` adds a JSON resource's messages (nested objects making dotted
/// ids, a namespace other than `translation` prefixing them) and `set_time_zone` takes any name.
/// `load_messages` loads nothing.
pub struct FakeI18n {
    i18n: FakeState<I18nState>,
    calls: Mutex<Vec<I18nCall>>,
}

impl FakeI18n {
    /// An i18n service at `state`.
    pub fn new(state: I18nState) -> Arc<Self> {
        Arc::new(Self {
            i18n: FakeState::new(state),
            calls: Mutex::default(),
        })
    }

    /// An i18n service in `language` (also its default), with no messages, in UTC.
    pub fn in_language(language: &str) -> Arc<Self> {
        let mut state = I18nState {
            default_language: language.to_owned(),
            time_zone: "UTC".to_owned(),
            ..I18nState::default()
        };
        if let Ok(locale) = dxn_intl::parse_locale(language) {
            show_in(&mut state, &locale);
        }
        Self::new(state)
    }

    /// Publishes `state`, as a change of the user's languages would.
    pub fn publish(&self, state: I18nState) {
        self.i18n.set(state);
    }

    /// The commands asked so far, in order.
    pub fn calls(&self) -> Vec<I18nCall> {
        lock(&self.calls).clone()
    }

    /// Records `call`, then changes the state with `change` under the next version, unless it
    /// fails.
    fn answer(
        &self,
        call: I18nCall,
        change: impl FnOnce(&mut I18nState) -> Result<(), I18nServiceError>,
    ) -> BoxFuture<'_, Result<(), I18nServiceError>> {
        lock(&self.calls).push(call);
        let result = self.i18n.change(|state| {
            let mut changed = state.clone();
            change(&mut changed)?;
            if changed != *state {
                changed.version = state.version + 1;
                *state = changed;
            }
            Ok(())
        });
        ready(result)
    }
}

/// `state` shown in `locale`: its language, locale tag and direction.
fn show_in(state: &mut I18nState, locale: &dxn_intl::Locale) {
    state.language = locale.id.to_string();
    state.locale = locale.to_string();
    state.direction = match dxn_intl::direction(locale) {
        dxn_intl::Direction::Rtl => TextDirection::Rtl,
        _ => TextDirection::Ltr,
    };
}

impl I18nService for FakeI18n {
    fn i18n(&self) -> &StateCell<I18nState> {
        self.i18n.cell()
    }

    fn set_language(&self, language: String) -> BoxFuture<'_, Result<(), I18nServiceError>> {
        self.answer(I18nCall::SetLanguage(language.clone()), |state| {
            let locale = dxn_intl::parse_locale(&language)
                .map_err(|_| I18nServiceError::InvalidLanguage(language.clone()))?;
            show_in(state, &locale);
            state.chosen_language = Some(state.language.clone());
            Ok(())
        })
    }

    fn follow_platform(&self) -> BoxFuture<'_, Result<(), I18nServiceError>> {
        self.answer(I18nCall::FollowPlatform, |state| {
            state.chosen_language = None;
            if let Ok(locale) = dxn_intl::parse_locale(&state.default_language) {
                show_in(state, &locale);
            }
            Ok(())
        })
    }

    fn load_messages(&self) -> BoxFuture<'_, Result<(), I18nServiceError>> {
        self.answer(I18nCall::LoadMessages, |_| Ok(()))
    }

    fn add_resource(
        &self,
        language: String,
        namespace: Option<String>,
        json: String,
    ) -> BoxFuture<'_, Result<(), I18nServiceError>> {
        let call = I18nCall::AddResource {
            language: language.clone(),
            namespace: namespace.clone(),
            json: json.clone(),
        };
        self.answer(call, |state| {
            let failed = |reason: String| I18nServiceError::Resource {
                path: format!(
                    "{language}/{}",
                    namespace.as_deref().unwrap_or("translation")
                ),
                reason,
            };
            let locale = dxn_intl::parse_locale(&language)
                .map_err(|_| I18nServiceError::InvalidLanguage(language.clone()))?;
            let prefix = match namespace.as_deref() {
                Some(namespace) if namespace != "translation" => namespace,
                _ => "",
            };
            let mut bundle = dxn_i18n::Bundle::new(locale.clone());
            let loaded = bundle.add_json_with_prefix(prefix, &json);
            let tag = locale.to_string();
            let index = match state.messages.iter().position(|b| b.language == tag) {
                Some(index) => index,
                None => {
                    state.messages.push(MessageBundle {
                        language: tag.clone(),
                        messages: BTreeMap::new(),
                    });
                    state.available_languages.push(tag);
                    state.messages.len() - 1
                }
            };
            // The messages that parse are added even when others do not.
            state.messages[index]
                .messages
                .extend(bundle.ids().filter_map(|id| {
                    bundle
                        .get(id)
                        .map(|message| (id.to_owned(), message.source().to_owned()))
                }));
            loaded.map_err(|error| failed(error.to_string()))
        })
    }

    fn set_time_zone(&self, time_zone: String) -> BoxFuture<'_, Result<(), I18nServiceError>> {
        self.answer(I18nCall::SetTimeZone(time_zone.clone()), |state| {
            state.time_zone = time_zone;
            Ok(())
        })
    }
}

/// A message formatter over the messages the test gives it ([`FakeMessageFormatter::message`],
/// MessageFormat 2 sources) in one locale, formatting them, numbers, dates and plurals with
/// [`dxn_i18n`] and [`dxn_intl`] as RayX's does, with no i18n service behind it (a missing
/// message shows its id); it records the message ids it is asked. Its wall times are the ones
/// the test gives it ([`FakeMessageFormatter::local_time_at`]), as it has no time zone database.
pub struct FakeMessageFormatter {
    translator: Mutex<dxn_i18n::Translator>,
    local_times: Mutex<BTreeMap<String, dxn_intl::DateTime>>,
    asked: Mutex<Vec<String>>,
}

impl FakeMessageFormatter {
    /// A formatter in `locale` (English if it does not read), with no messages.
    pub fn new(locale: &str) -> Arc<Self> {
        let locale = dxn_intl::parse_locale(locale)
            .or_else(|_| dxn_intl::parse_locale("en"))
            .unwrap_or(dxn_intl::Locale::UNKNOWN);
        Arc::new(Self {
            translator: Mutex::new(dxn_i18n::Translator::new(locale)),
            local_times: Mutex::default(),
            asked: Mutex::default(),
        })
    }

    /// Adds the message `id` with the MessageFormat 2 `source`, in the formatter's locale.
    ///
    /// # Panics
    ///
    /// When `source` is not a MessageFormat 2 message: a broken fixture.
    pub fn message(self: Arc<Self>, id: &str, source: &str) -> Arc<Self> {
        {
            let mut translator = lock(&self.translator);
            let mut bundle = dxn_i18n::Bundle::new(translator.locale().clone());
            if let Err(error) = bundle.add(id, source) {
                panic!("the message `{id}` does not parse: {error}");
            }
            translator.add_bundle(bundle);
        }
        self
    }

    /// Answers `local_time(instant)` (an RFC 3339 timestamp, or milliseconds after the Unix
    /// epoch as `local_time_from_millis` asks, written in decimal) with `wall_time`.
    pub fn local_time_at(
        self: Arc<Self>,
        instant: &str,
        wall_time: dxn_intl::DateTime,
    ) -> Arc<Self> {
        lock(&self.local_times).insert(instant.to_owned(), wall_time);
        self
    }

    /// The wall time the test gave `instant`.
    fn wall_time(&self, instant: &str) -> Result<dxn_intl::DateTime, I18nServiceError> {
        lock(&self.local_times)
            .get(instant)
            .copied()
            .ok_or_else(|| {
                I18nServiceError::Format(format!(
                    "the fake formatter has no wall time for {instant}"
                ))
            })
    }

    /// The message ids asked so far (`contains`, `t`, `format` and the like), in order.
    pub fn asked(&self) -> Vec<String> {
        lock(&self.asked).clone()
    }

    /// Records `id` and runs `f` with the translator.
    fn with<R>(&self, id: &str, f: impl FnOnce(&dxn_i18n::Translator) -> R) -> R {
        lock(&self.asked).push(id.to_owned());
        f(&lock(&self.translator))
    }
}

impl MessageFormatter for FakeMessageFormatter {
    fn locale(&self) -> dxn_intl::Locale {
        lock(&self.translator).locale().clone()
    }

    fn contains(&self, id: &str) -> bool {
        self.with(id, |translator| translator.contains(id))
    }

    fn format(&self, id: &str, args: &dxn_i18n::Args) -> String {
        self.with(id, |translator| translator.format(id, args))
    }

    fn try_format(
        &self,
        id: &str,
        args: &dxn_i18n::Args,
    ) -> Result<String, dxn_i18n::TranslateError> {
        self.with(id, |translator| translator.try_format(id, args))
    }

    fn format_to_parts(
        &self,
        id: &str,
        args: &dxn_i18n::Args,
    ) -> Option<dxn_i18n::Formatted<Vec<dxn_i18n::Part>>> {
        self.with(id, |translator| translator.format_to_parts(id, args))
    }

    fn format_number(
        &self,
        number: dxn_intl::Number,
        options: dxn_intl::NumberOptions,
    ) -> Result<String, I18nServiceError> {
        dxn_intl::NumberFormatter::new(&self.locale(), options)
            .and_then(|formatter| formatter.format(number))
            .map_err(|error| I18nServiceError::Format(error.to_string()))
    }

    fn format_date_time(
        &self,
        value: &dxn_intl::DateTime,
        options: dxn_intl::DateTimeOptions,
    ) -> Result<String, I18nServiceError> {
        dxn_intl::DateTimeFormatter::new(&self.locale(), options)
            .and_then(|formatter| formatter.format(value))
            .map_err(|error| I18nServiceError::Format(error.to_string()))
    }

    fn plural_category(&self, number: dxn_intl::Number) -> dxn_intl::PluralCategory {
        dxn_intl::PluralRules::new(&self.locale(), dxn_intl::PluralKind::Cardinal)
            .map_or(dxn_intl::PluralCategory::Other, |rules| {
                rules.category(number)
            })
    }

    fn local_time(&self, instant: &str) -> Result<dxn_intl::DateTime, I18nServiceError> {
        self.wall_time(instant)
    }

    fn local_time_from_millis(&self, millis: i64) -> Result<dxn_intl::DateTime, I18nServiceError> {
        self.wall_time(&millis.to_string())
    }
}
