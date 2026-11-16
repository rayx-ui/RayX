//! RayX's services for TSX, beside [`I18nService`](super::I18nService): each class carries its
//! Rust service's name and is its own token (`services.getRequiredService(NavigationService)`),
//! and each `onChange` listener hears the service's changes until the function it returned is
//! called (or the reactive owner of the call goes), or the object is disposed.
//!
//! Values cross as plain objects (the services' snapshots, serialized). The services notify on
//! the engine's thread, where TSX listeners run.

use nsh_core::JsonValue;
use nsh_core::prelude::*;
use rayx_core::ChangeSubscription;
use rayx_runtime::services::application_configuration::{
    ApplicationConfigurationService as Configuration, LayoutDirection,
};
use rayx_runtime::services::assets::AssetService as Assets;
use rayx_runtime::services::fonts::FontService as Fonts;
use rayx_runtime::services::navigation::NavigationService as Navigation;
use rayx_runtime::services::platform::PlatformInfoService as Platform;
use rayx_runtime::services::storage::KeyValueStore as Store;
use rayx_runtime::services::suspension::SuspensionService as Suspension;
use serde::Serialize;
use std::sync::Arc;

use super::NavigationRegion;
use super::listeners::Listeners;
use super::provider::ServiceClass;

/// `value` as a plain object for TSX.
fn json(value: &impl Serialize) -> JsonValue {
    serde_json::to_value(value).unwrap_or(JsonValue::Null)
}

/// The app's navigation, for TSX: `services.getRequiredService(NavigationService)`, then
/// `navigate("/projects/12")`, `back()` and `current()`; `region(name)` is a region of it, and
/// `onChange` hears every route change, with `{ region, from, to }`.
#[class]
pub struct NavigationService {
    service: Arc<Navigation>,
    listeners: Listeners,
}

impl ServiceClass for NavigationService {
    type Service = Navigation;

    fn wrap(service: Arc<Navigation>) -> Self {
        Self {
            service,
            listeners: Listeners::default(),
        }
    }
}

#[class]
impl NavigationService {
    /// Navigates the root region to `uri`; the result has the region, the direction, the status
    /// (`Success`, `FailedNotFound`, …) and the route.
    pub fn navigate(&self, uri: String) -> Result<JsonValue, String> {
        self.service
            .navigate_to(uri)
            .map(|result| json(&result))
            .map_err(|error| error.to_string())
    }

    /// Goes back in the root region's journal.
    pub fn back(&self) -> Result<JsonValue, String> {
        self.service
            .back()
            .map(|result| json(&result))
            .map_err(|error| error.to_string())
    }

    /// Goes forward in the root region's journal.
    pub fn forward(&self) -> Result<JsonValue, String> {
        self.service
            .forward()
            .map(|result| json(&result))
            .map_err(|error| error.to_string())
    }

    /// The root region's current route and its status, `null` before the first navigation.
    pub fn current(&self) -> Result<JsonValue, String> {
        self.service
            .current()
            .map(|current| current.map_or(JsonValue::Null, |data| json(&data)))
            .map_err(|error| error.to_string())
    }

    /// The navigation region `name`, made when missing: its own journal, for a `NavStack`.
    pub fn region(&self, name: String) -> Result<Instance<NavigationRegion>, String> {
        NavigationRegion::of(Arc::clone(&self.service), name).map(Instance::new)
    }

    /// Calls `listener` with `{ region, from, to }` after every route change, until the
    /// function it returns is called (or the reactive owner of the call goes), or this object
    /// is disposed.
    pub fn on_change(
        &mut self,
        #[args(event)] listener: Callback<JsonValue>,
    ) -> Result<Stop, String> {
        let service = Arc::clone(&self.service);
        self.listeners.listen(listener, move |post| {
            service
                .subscribe_route_changed(move |event| {
                    post(serde_json::json!({
                        "region": event.region,
                        "from": event.from.as_ref().map(json),
                        "to": json(&event.to),
                    }));
                })
                .map_err(|error| error.to_string())
        })
    }
}

/// The app's key-value store, for TSX: `services.getRequiredService(KeyValueStore)`, values by
/// bucket and key, as text (`get`, `set`) or JSON (`getJson`, `setJson`). On NSH they persist in
/// NSH's storage, beside the app's own `nsh/storage` entries.
#[class]
pub struct KeyValueStore {
    store: Arc<dyn Store>,
}

impl ServiceClass for KeyValueStore {
    type Service = dyn Store;

    fn wrap(store: Arc<dyn Store>) -> Self {
        Self { store }
    }
}

#[class]
impl KeyValueStore {
    /// The text at `key` in `bucket`, `null` when there is none.
    pub fn get(&self, bucket: String, key: String) -> Result<Option<String>, String> {
        self.store
            .get(&bucket, &key)
            .map_err(|error| error.to_string())?
            .map(|bytes| {
                String::from_utf8(bytes).map_err(|_| format!("{bucket}/{key} holds no text"))
            })
            .transpose()
    }

    /// Sets the text at `key` in `bucket`.
    pub fn set(&self, bucket: String, key: String, value: String) -> Result<(), String> {
        self.store
            .set(&bucket, &key, value.into_bytes())
            .map_err(|error| error.to_string())
    }

    /// The JSON value at `key` in `bucket`, `null` when there is none.
    pub fn get_json(&self, bucket: String, key: String) -> Result<JsonValue, String> {
        self.store
            .bucket(bucket)
            .try_get_json::<JsonValue>(&key)
            .map(|value| value.unwrap_or(JsonValue::Null))
            .map_err(|error| error.to_string())
    }

    /// Sets `key` in `bucket` to `value` as JSON.
    pub fn set_json(&self, bucket: String, key: String, value: JsonValue) -> Result<(), String> {
        self.store
            .bucket(bucket)
            .set_json(&key, &value)
            .map_err(|error| error.to_string())
    }

    /// Removes `key` from `bucket`; whether it was there.
    pub fn remove(&self, bucket: String, key: String) -> Result<bool, String> {
        self.store
            .remove(&bucket, &key)
            .map_err(|error| error.to_string())
    }

    /// The keys of `bucket`, sorted.
    pub fn keys(&self, bucket: String) -> Result<Vec<String>, String> {
        self.store.keys(&bucket).map_err(|error| error.to_string())
    }

    /// Removes every value of `bucket`.
    pub fn clear(&self, bucket: String) -> Result<(), String> {
        self.store.clear(&bucket).map_err(|error| error.to_string())
    }
}

/// The app's configuration (`RayX:Application`), for TSX:
/// `services.getRequiredService(ApplicationConfigurationService)`, its settings (`current()`),
/// the layout direction and whether motion is reduced; `onChange` hears every change of the
/// settings.
#[class]
pub struct ApplicationConfigurationService {
    service: Arc<Configuration>,
    listeners: Listeners,
}

impl ServiceClass for ApplicationConfigurationService {
    type Service = Configuration;

    fn wrap(service: Arc<Configuration>) -> Self {
        Self {
            service,
            listeners: Listeners::default(),
        }
    }
}

#[class]
impl ApplicationConfigurationService {
    /// The settings now.
    pub fn current(&self) -> JsonValue {
        json(&self.service.options())
    }

    /// `"ltr"` or `"rtl"`.
    pub fn layout_direction(&self) -> String {
        match self.service.layout_direction() {
            LayoutDirection::Ltr => "ltr",
            LayoutDirection::Rtl => "rtl",
        }
        .to_owned()
    }

    /// Whether motion is reduced, by the user's platform setting or the app's.
    pub fn reduced_motion(&self) -> bool {
        self.service.effective_motion(None).is_reduced()
    }

    /// Calls `listener` with the settings after every change, until the function it returns is
    /// called (or the reactive owner of the call goes), or this object is disposed.
    pub fn on_change(
        &mut self,
        #[args(settings)] listener: Callback<JsonValue>,
    ) -> Result<Stop, String> {
        let service = Arc::clone(&self.service);
        self.listeners.listen(listener, move |post| {
            Ok::<ChangeSubscription, String>(
                service.subscribe(move |snapshot| post(json(&snapshot.options))),
            )
        })
    }
}

/// The app's assets, for TSX: `services.getRequiredService(AssetService).url("images/logo.png")`,
/// the URL of an asset of the catalog, for an `<Image source>` or a `fetch`.
#[class]
pub struct AssetService {
    service: Arc<Assets>,
}

impl ServiceClass for AssetService {
    type Service = Assets;

    fn wrap(service: Arc<Assets>) -> Self {
        Self { service }
    }
}

#[class]
impl AssetService {
    /// The URL of the asset at `path`.
    pub fn url(&self, path: String) -> Result<String, String> {
        self.service.uri(&path).map_err(|error| error.to_string())
    }

    /// The URL of the asset at `path` for the pixel ratio `scale` (its `@2x`, `@3x` variant).
    pub fn url_for_scale(&self, path: String, scale: f64) -> Result<String, String> {
        self.service
            .uri_for_scale(&path, scale as f32)
            .map_err(|error| error.to_string())
    }
}

/// The app's fonts, for TSX: `services.getRequiredService(FontService).family("script")`, a
/// `fontFamily` for an alias of the font service (its family and fallbacks), or a family itself.
#[class]
pub struct FontService {
    service: Arc<Fonts>,
}

impl ServiceClass for FontService {
    type Service = Fonts;

    fn wrap(service: Arc<Fonts>) -> Self {
        Self { service }
    }
}

#[class]
impl FontService {
    /// The `fontFamily` for `alias` (its family and fallbacks), or `alias` itself as a family.
    pub fn family(&self, alias: String) -> String {
        self.service.font_family(&alias)
    }

    /// Every alias, resolved: `{ alias, requestedFamily, resolvedFamily, glyphFallbackFamilies }`.
    pub fn aliases(&self) -> JsonValue {
        json(&self.service.resolved_aliases())
    }
}

/// The device and the user's settings, for TSX:
/// `services.getRequiredService(PlatformInfoService).current()` (the platform service's snapshot:
/// platform, form factor, input, viewport, insets, keyboard, color scheme, accessibility, locale,
/// features, displays, window, …); `onChange` hears every change of them.
#[class]
pub struct PlatformInfoService {
    platform: Arc<Platform>,
    listeners: Listeners,
}

impl ServiceClass for PlatformInfoService {
    type Service = Platform;

    fn wrap(platform: Arc<Platform>) -> Self {
        Self {
            platform,
            listeners: Listeners::default(),
        }
    }
}

#[class]
impl PlatformInfoService {
    /// The device and the user's settings now.
    pub fn current(&self) -> JsonValue {
        json(&self.platform.current())
    }

    /// Calls `listener` with the snapshot after every change of the device or the user's
    /// settings, until the function it returns is called (or the reactive owner of the call
    /// goes), or this object is disposed.
    pub fn on_change(
        &mut self,
        #[args(snapshot)] listener: Callback<JsonValue>,
    ) -> Result<Stop, String> {
        let platform = Arc::clone(&self.platform);
        self.listeners.listen(listener, move |post| {
            Ok::<ChangeSubscription, String>(
                platform.subscribe(move |event| post(json(&event.current))),
            )
        })
    }
}

/// The app's lifecycle, for TSX: `services.getRequiredService(SuspensionService).lifecycle()`
/// (the suspension service's snapshot: lifecycle, activation and visibility); `onLifecycle`
/// hears every lifecycle event (`{ kind, reason, … }`, `kind` as Rust names it: `"Started"`,
/// `"Activated"`, `"Backgrounded"`, `"Woke"`, `"QuitRequested"`, `"Destroying"`, …).
/// `askBeforeQuit(listener)` makes the user's requests to quit come to `listener` until this
/// object is disposed, and `quit()` quits (a desktop's app).
#[class]
pub struct SuspensionService {
    suspension: Arc<Suspension>,
    listeners: Listeners,
}

impl ServiceClass for SuspensionService {
    type Service = Suspension;

    fn wrap(suspension: Arc<Suspension>) -> Self {
        Self {
            suspension,
            listeners: Listeners::default(),
        }
    }
}

#[class]
impl SuspensionService {
    /// The app's lifecycle, activation and visibility now.
    pub fn lifecycle(&self) -> JsonValue {
        json(&self.suspension.current())
    }

    /// Calls `listener` with every lifecycle event (`{ kind, reason, timestamp_unix_millis,
    /// window }`: the app's start, focus, background and foreground, its window's changes, the
    /// system's wakes from sleep, its quit), until the function it returns is called (or the
    /// reactive owner of the call goes), or this object is disposed.
    pub fn on_lifecycle(
        &mut self,
        #[args(event)] listener: Callback<JsonValue>,
    ) -> Result<Stop, String> {
        let suspension = Arc::clone(&self.suspension);
        self.listeners.listen(listener, move |post| {
            Ok::<_, String>(suspension.subscribe_events(move |event| post(json(event))))
        })
    }

    /// Until the function it returns is called (or the reactive owner of the call goes), or
    /// this object is disposed, the user's requests to quit (the app's Quit item, its quit key)
    /// call `listener` instead of quitting: the app asks, saves or waits, then calls `quit()`.
    /// Once no listener is left, a request quits again. Desktops only.
    pub fn ask_before_quit(&mut self, listener: Callback<JsonValue>) -> Result<Stop, String> {
        let suspension = Arc::clone(&self.suspension);
        self.listeners.listen(listener, move |post| {
            Ok::<_, String>(suspension.ask_before_quit(move || post(JsonValue::Null)))
        })
    }

    /// Quits the app, whether or not it asks first (a desktop's; elsewhere nothing happens).
    pub fn quit(&self) {
        self.suspension.quit();
    }
}
