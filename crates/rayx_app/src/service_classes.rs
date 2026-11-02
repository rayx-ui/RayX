//! RayX's services for TSX, beside [`I18n`](crate::I18n): each class is made with `X.of(app)`
//! once `AppServices.load()` resolved, and its `onChange` listeners hear the service's changes
//! until the object is disposed.
//!
//! Values cross as plain objects (the services' snapshots, serialized). The services notify on
//! the engine's thread, where TSX listeners run.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;

use nsh_core::JsonValue;
use nsh_core::prelude::*;
use rayx_core::ChangeSubscription;
use rayx_runtime::services::application_configuration::{
    ApplicationConfigurationService, LayoutDirection,
};
use rayx_runtime::services::assets::AssetService;
use rayx_runtime::services::fonts::FontService;
use rayx_runtime::services::navigation::{NavigationService, NavigationSubscription};
use rayx_runtime::services::platform::PlatformInfoService;
use rayx_runtime::services::storage::KeyValueStore;
use rayx_runtime::services::suspension::SuspensionService;
use serde::Serialize;
use std::sync::Arc;

use crate::{AppError, AppServices};

thread_local! {
    /// TSX listeners by id: a service's listeners must be `Send`, a TSX function is not, and
    /// both live on the engine's thread.
    static LISTENERS: RefCell<HashMap<u64, Callback<JsonValue>>> = RefCell::new(HashMap::new());
    static NEXT_ID: Cell<u64> = const { Cell::new(0) };
}

/// A TSX listener, removed with its object. `subscription` is the service's side.
struct Listener {
    id: u64,
    _subscription: Box<dyn std::any::Any>,
}

impl Drop for Listener {
    fn drop(&mut self) {
        LISTENERS.with(|listeners| listeners.borrow_mut().remove(&self.id));
    }
}

/// Keeps `callback` and returns its id and a function that posts to it.
fn keep(callback: Callback<JsonValue>) -> (u64, impl Fn(JsonValue) + Send + Sync + 'static) {
    let id = NEXT_ID.with(|next| {
        let id = next.get();
        next.set(id + 1);
        id
    });
    LISTENERS.with(|listeners| listeners.borrow_mut().insert(id, callback));
    let post = move |value: JsonValue| {
        LISTENERS.with(|listeners| {
            if let Some(listener) = listeners.borrow().get(&id) {
                listener.post(value);
            }
        });
    };
    (id, post)
}

/// `value` as a plain object for TSX.
fn json(value: &impl Serialize) -> JsonValue {
    serde_json::to_value(value).unwrap_or(JsonValue::Null)
}

/// The service `T` of the app, or why not.
fn service<T: Send + Sync + 'static>(app: &Instance<AppServices>) -> Result<Arc<T>, AppError> {
    app.with(|app| app.services().required_service::<T>())
        .map_err(AppError::from)
}

/// The app's navigation, for TSX: `Navigation.of(app)`, then `navigate("/teams/12")`, `back()`
/// and `current()`; `onChange` hears every route change, with `{ region, from, to }`.
#[class]
pub struct Navigation {
    service: NavigationService,
    listeners: Vec<Listener>,
}

#[class]
impl Navigation {
    /// The app's navigation service (`add_navigation` registers it).
    pub fn of(app: Instance<AppServices>) -> Result<Instance<Navigation>, AppError> {
        let service = service::<NavigationService>(&app)?;
        Ok(Instance::new(Navigation {
            service: (*service).clone(),
            listeners: Vec::new(),
        }))
    }

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

    /// Calls `listener` with `{ region, from, to }` after every route change, until this
    /// object is disposed.
    pub fn on_change(
        &mut self,
        #[args(event)] listener: Callback<JsonValue>,
    ) -> Result<(), String> {
        let (id, post) = keep(listener);
        let subscription: NavigationSubscription = self
            .service
            .subscribe_route_changed(move |event| {
                post(serde_json::json!({
                    "region": event.region,
                    "from": event.from.as_ref().map(json),
                    "to": json(&event.to),
                }));
            })
            .map_err(|error| error.to_string())?;
        self.listeners.push(Listener {
            id,
            _subscription: Box::new(subscription),
        });
        Ok(())
    }
}

/// The app's key-value store, for TSX: `Storage.of(app)`, values by bucket and key, as text
/// (`get`, `set`) or JSON (`getJson`, `setJson`). On NSH they persist in NSH's storage, beside
/// the app's own `nsh/storage` entries.
#[class]
pub struct Storage {
    store: Arc<dyn KeyValueStore>,
}

#[class]
impl Storage {
    /// The app's store (`add_storage` registers it; on NSH it is NSH's storage).
    pub fn of(app: Instance<AppServices>) -> Result<Instance<Storage>, AppError> {
        let store = app.with(|app| app.services().required_service::<dyn KeyValueStore>())?;
        Ok(Instance::new(Storage { store }))
    }

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

/// The app's configuration (`RayX:Application`), for TSX: `AppConfiguration.of(app)`, its
/// settings (`current()`), the layout direction and whether motion is reduced; `onChange`
/// hears every change of the settings.
#[class]
pub struct AppConfiguration {
    service: ApplicationConfigurationService,
    listeners: Vec<Listener>,
}

#[class]
impl AppConfiguration {
    /// The app's configuration service (`add_application_configuration` registers it).
    pub fn of(app: Instance<AppServices>) -> Result<Instance<AppConfiguration>, AppError> {
        let service = service::<ApplicationConfigurationService>(&app)?;
        Ok(Instance::new(AppConfiguration {
            service: (*service).clone(),
            listeners: Vec::new(),
        }))
    }

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

    /// Calls `listener` with the settings after every change, until this object is disposed.
    pub fn on_change(&mut self, #[args(settings)] listener: Callback<JsonValue>) {
        let (id, post) = keep(listener);
        let subscription: ChangeSubscription = self
            .service
            .subscribe(move |snapshot| post(json(&snapshot.options)));
        self.listeners.push(Listener {
            id,
            _subscription: Box::new(subscription),
        });
    }
}

/// The app's assets, for TSX: `Assets.of(app).url("images/logo.png")`, the URL of an asset of
/// the catalog, for an `<Image source>` or a `fetch`.
#[class]
pub struct Assets {
    service: Arc<AssetService>,
}

#[class]
impl Assets {
    /// The app's asset service (`add_assets` registers it).
    pub fn of(app: Instance<AppServices>) -> Result<Instance<Assets>, AppError> {
        Ok(Instance::new(Assets {
            service: service::<AssetService>(&app)?,
        }))
    }

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

/// The app's fonts, for TSX: `Fonts.of(app).family("script")`, a `fontFamily` for an alias of
/// the font service (its family and fallbacks), or a family itself.
#[class]
pub struct Fonts {
    service: Arc<FontService>,
}

#[class]
impl Fonts {
    /// The app's font service (`add_fonts` registers it).
    pub fn of(app: Instance<AppServices>) -> Result<Instance<Fonts>, AppError> {
        Ok(Instance::new(Fonts {
            service: service::<FontService>(&app)?,
        }))
    }

    /// The `fontFamily` for `alias` (its family and fallbacks), or `alias` itself as a family.
    pub fn family(&self, alias: String) -> String {
        self.service.font_family(&alias)
    }

    /// Every alias, resolved: `{ alias, requestedFamily, resolvedFamily, glyphFallbackFamilies }`.
    pub fn aliases(&self) -> JsonValue {
        json(&self.service.resolved_aliases())
    }
}

/// The device and the app's lifecycle, for TSX: `Platform.of(app).current()` (the platform
/// service's snapshot: form factor, viewport, insets, color scheme, input, locale, …) and
/// `lifecycle()` (the suspension service's); `onChange` hears every change of the device.
#[class]
pub struct Platform {
    platform: Arc<PlatformInfoService>,
    suspension: Option<Arc<SuspensionService>>,
    listeners: Vec<Listener>,
}

#[class]
impl Platform {
    /// The app's platform service (`add_platform` registers it), and its suspension service
    /// when it has one.
    pub fn of(app: Instance<AppServices>) -> Result<Instance<Platform>, AppError> {
        let platform = service::<PlatformInfoService>(&app)?;
        let suspension = app.with(|app| app.services().service::<SuspensionService>());
        Ok(Instance::new(Platform {
            platform,
            suspension,
            listeners: Vec::new(),
        }))
    }

    /// The device and the user's settings now.
    pub fn current(&self) -> JsonValue {
        json(&self.platform.current())
    }

    /// The app's lifecycle, activation and visibility, `null` without a suspension service.
    pub fn lifecycle(&self) -> JsonValue {
        self.suspension
            .as_ref()
            .map_or(JsonValue::Null, |suspension| json(&suspension.current()))
    }

    /// Calls `listener` with the snapshot after every change of the device or the user's
    /// settings, until this object is disposed.
    pub fn on_change(&mut self, #[args(snapshot)] listener: Callback<JsonValue>) {
        let (id, post) = keep(listener);
        let subscription: ChangeSubscription = self
            .platform
            .subscribe(move |event| post(json(&event.current)));
        self.listeners.push(Listener {
            id,
            _subscription: Box::new(subscription),
        });
    }
}
