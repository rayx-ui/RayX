//! Fakes of the core's service contracts, NSH-free, for tests: each implements its trait over
//! what the test gives it and records what it is asked, and [`add_host_fakes`] registers the
//! services an NSH host requires.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use futures::FutureExt as _;
use futures::future::BoxFuture;

use crate::published::{EventStream, StateCell};
use crate::services::app::AppBuilder;
use crate::services::assets::{AssetCatalog, AssetError, AssetService};
use crate::services::fonts::{FontService, FontServiceError, FontState, RegisteredFontFace};
use crate::services::history::{
    HistoryError, HistoryEvent, HistoryScopeId, HistoryScopeState, HistoryService, HistoryState,
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
use crate::services::platform::{PlatformInfoService, PlatformSnapshot};
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

/// An operation service applying records through their targets' handlers as RayX's does
/// (transactions queue their events until they commit and undo their operations, newest first,
/// when they abort; a released target refuses operations), publishing its targets and every
/// event, without diagnostics. For testing what consumes operations (history) without RayX's.
pub struct FakeOperations {
    instance_id: u64,
    next_transaction: AtomicU64,
    state: Mutex<FakeOperationState>,
    operations: StateCell<OperationState>,
    events: EventStream<OperationEvent>,
    /// Held while the targets are published; a change meanwhile marks them stale.
    publishing: Mutex<()>,
    stale: AtomicBool,
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
            operations: StateCell::new(OperationState::default()),
            events: EventStream::new(),
            publishing: Mutex::new(()),
            stale: AtomicBool::new(false),
        })
    }

    fn next_transaction_id(&self) -> u64 {
        self.next_transaction.fetch_add(1, Ordering::SeqCst) + 1
    }

    /// Publishes the targets unless they are the ones published: a change made meanwhile is
    /// published right after, by whoever is publishing.
    fn publish_targets(&self) {
        self.stale.store(true, Ordering::SeqCst);
        loop {
            let publishing = match self.publishing.try_lock() {
                Ok(guard) => guard,
                Err(std::sync::TryLockError::Poisoned(poisoned)) => poisoned.into_inner(),
                Err(std::sync::TryLockError::WouldBlock) => return,
            };
            self.stale.store(false, Ordering::SeqCst);
            let targets: Vec<OperationTargetId> =
                lock(&self.state).targets.keys().cloned().collect();
            if self.operations.get().targets != targets {
                self.operations.publish(OperationState {
                    targets,
                    ..OperationState::default()
                });
            }
            drop(publishing);
            if !self.stale.load(Ordering::SeqCst) {
                return;
            }
        }
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
        &self.operations
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
    state: Mutex<HistoryState>,
    history: StateCell<HistoryState>,
    events: EventStream<HistoryEvent>,
    /// Held while the state is published; a change meanwhile marks it stale.
    publishing: Mutex<()>,
    stale: AtomicBool,
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

    /// Changes `scope`'s state with `change`, then publishes the state.
    fn change(&self, scope: &HistoryScopeId, change: impl FnOnce(&mut HistoryScopeState)) {
        {
            let mut state = lock(&self.state);
            let entry = state.scopes.entry(scope.clone()).or_default();
            change(entry);
            entry.revision += 1;
        }
        self.publish_state();
    }

    /// Publishes the state unless it is the one published: a change made meanwhile is published
    /// right after, by whoever is publishing.
    fn publish_state(&self) {
        self.stale.store(true, Ordering::SeqCst);
        loop {
            let publishing = match self.publishing.try_lock() {
                Ok(guard) => guard,
                Err(std::sync::TryLockError::Poisoned(poisoned)) => poisoned.into_inner(),
                Err(std::sync::TryLockError::WouldBlock) => return,
            };
            self.stale.store(false, Ordering::SeqCst);
            let state = lock(&self.state).clone();
            if *self.history.get() != state {
                self.history.publish(state);
            }
            drop(publishing);
            if !self.stale.load(Ordering::SeqCst) {
                return;
            }
        }
    }

    /// Moves one entry of `scope` from one stack to the other; whether there was one.
    fn step(&self, scope: HistoryScopeId, undo: bool) -> bool {
        let mut stepped = false;
        self.change(&scope, |state| {
            let (from, to) = if undo {
                (&mut state.undo_depth, &mut state.redo_depth)
            } else {
                (&mut state.redo_depth, &mut state.undo_depth)
            };
            if *from > 0 {
                *from -= 1;
                *to += 1;
                stepped = true;
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
        &self.history
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
    selection: StateCell<SelectionState>,
    calls: Mutex<Vec<SelectionCall>>,
}

impl FakeSelection {
    pub fn new() -> Arc<Self> {
        Arc::default()
    }

    /// Publishes `state`, as a selection change would.
    pub fn publish(&self, state: SelectionState) {
        self.selection.publish(state);
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
        let state = self.selection.get();
        if applies(&state) {
            self.selection.publish(SelectionState {
                active_area: area,
                status,
                ..*state
            });
        }
        ready(Ok(()))
    }
}

impl SelectionService for FakeSelection {
    fn selection(&self) -> &StateCell<SelectionState> {
        &self.selection
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
