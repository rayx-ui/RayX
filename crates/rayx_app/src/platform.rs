//! What RayX's services need from NSH: reading the app's files, registering fonts, and the
//! device's state and the app's lifecycle (its app state, its window, the system's wakes from
//! sleep, its quitting) copied into the platform and suspension services.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

use futures::FutureExt as _;
use futures::future::LocalBoxFuture;
use nsh_core::desktop::{Display, WindowMode as NshWindowMode, WindowState};
use nsh_core::element::cell_size;
use nsh_core::element::desktop::{displays, window_state};
use nsh_core::geometry::{Bounds, Points};
use nsh_core::platform::{Idiom, Lifecycle, Os, Weekday};
use nsh_core::prelude::{
    Owner, apple_keys, can_pick_screen_color, cleanup, dark, effect, fetch, font_scale, global,
    idiom, is_catalyst, is_desktop, keyboard_duration, keyboard_height, lifecycle, locale,
    locale_settings, os, os_version, pixel_ratio, platform_name, reads_active_descendants,
    reduce_motion, register_font, safe_area, screen_reader, shaders_available, spawn,
    speech_available, untracked, wake_count, web_evaluate_available, web_input_available,
    web_view_available, window,
};
use rayx_core::{AppBuilder, ServiceProvider};
use rayx_runtime::services::assets::{AssetAppBuilderExt as _, AssetReader};
use rayx_runtime::services::fonts::FontRegistrar;
use rayx_runtime::services::platform::{
    ColorScheme, DisplaySnapshot, FormFactor, InputCapabilities, Insets, LocaleSnapshot,
    PlatformFamily, PlatformFeatures, PlatformInfoService, PlatformRect, PlatformSnapshot,
    TextDirection, ViewportSnapshot, WindowMode, WindowSnapshot,
};
use rayx_runtime::services::storage::KeyValueStore;
use rayx_runtime::services::suspension::{
    SuspensionEvent, SuspensionEventKind, SuspensionEventReason, SuspensionHost, SuspensionService,
    WindowActivity,
};

use rayx_runtime::services::navigation::NavigationLocationSynchronizer;

use crate::suspension::{QuitWiring, keep_quitting};
use crate::{
    AppState, NshKeyValueStore, NshLocationSynchronizer, NshSuspensionHost, app_services, app_state,
};

/// Reads the app's files with NSH's `fetch` of their `asset:/` URLs (from the app's package on
/// device, the dev server during `nsh run`, the page's assets on the web).
#[derive(Clone, Copy, Debug, Default)]
pub struct NshAssetReader;

impl AssetReader for NshAssetReader {
    fn read(&self, uri: &str) -> LocalBoxFuture<'static, Result<Vec<u8>, String>> {
        let uri = uri.to_owned();
        async move {
            let response = fetch(uri.as_str()).await?;
            if response.ok() {
                Ok(response.body)
            } else {
                Err(format!("{uri} answered {}", response.status()))
            }
        }
        .boxed_local()
    }
}

/// Registers fonts with NSH's `register_font` (TrueType, OpenType, WOFF, WOFF2), which reads
/// their family names. The fonts the app ships NSH registered before the first frame (every
/// font of its UI sets' crates' `assets/`), so for those it only reads the family.
#[derive(Clone, Copy, Debug, Default)]
pub struct NshFontRegistrar;

impl FontRegistrar for NshFontRegistrar {
    fn register(&self, data: Vec<u8>) -> LocalBoxFuture<'static, Result<String, String>> {
        async move { register_font(data).await }.boxed_local()
    }

    fn register_asset(
        &self,
        _uri: &str,
        data: Vec<u8>,
    ) -> LocalBoxFuture<'static, Result<String, String>> {
        async move { nsh_core::font_family(&data) }.boxed_local()
    }
}

/// Registers NSH's [`NshAssetReader`] and [`NshFontRegistrar`] as the app's `dyn AssetReader`
/// and `dyn FontRegistrar`, its [`NshKeyValueStore`] (a startup service, which loads on the
/// engine thread) as its `dyn KeyValueStore`, its [`NshLocationSynchronizer`] as its
/// `dyn NavigationLocationSynchronizer` and its [`NshSuspensionHost`] as its
/// `dyn SuspensionHost`, unless others are registered. Every surface's builder starts with them
/// ([`start_app`](crate::start_app)), so on NSH `add_storage()` keeps NSH's persistent store,
/// `add_navigation()` shows its location as NSH's and `add_suspension()` quits the app.
pub trait NshServicesAppBuilderExt: AppBuilder {
    /// Adds the files a crate ships, as NSH's `assets!()` lists them (the list its UI set
    /// registers), to the asset catalog: `builder.add_asset_files(assets!())`. The crate's
    /// manifest (`assets.toml`) is then optional, for metadata (tags, locale, theme, ...).
    fn add_asset_files(self, files: &[nsh_core::element::AssetFile]) -> Self {
        self.add_bundled_assets(files.iter().map(|file| file.uri))
    }

    fn add_nsh_services(mut self) -> Self {
        let services = self.services_mut();
        services.try_add_service_factory_as::<dyn AssetReader>(|_| Arc::new(NshAssetReader));
        services.try_add_service_factory_as::<dyn FontRegistrar>(|_| Arc::new(NshFontRegistrar));
        services.try_add_app_service_factory::<NshKeyValueStore>(|_| NshKeyValueStore::new());
        services.try_add_service_factory_as::<dyn KeyValueStore>(|services| {
            services
                .required_service::<NshKeyValueStore>()
                .unwrap_or_else(|error| panic!("NSH's key-value store is registered: {error}"))
        });
        services.try_add_service_factory_as::<NshLocationSynchronizer>(|_| {
            Arc::new(NshLocationSynchronizer::new())
        });
        services.try_add_service_factory_as::<dyn NavigationLocationSynchronizer>(|services| {
            services
                .required_service::<NshLocationSynchronizer>()
                .unwrap_or_else(|error| {
                    panic!("NSH's location synchronizer is registered: {error}")
                })
        });
        services.try_add_service_factory_as::<NshSuspensionHost>(|_| {
            Arc::new(NshSuspensionHost::new())
        });
        services.try_add_service_factory_as::<dyn SuspensionHost>(|services| {
            services
                .required_service::<NshSuspensionHost>()
                .unwrap_or_else(|error| panic!("NSH's suspension host is registered: {error}"))
        });
        self
    }
}

impl<T> NshServicesAppBuilderExt for T where T: AppBuilder {}

/// Gives the root's platform service NSH's environment as it is now, without subscribing:
/// [`start_app`](crate::start_app) does it before startup, so startup services see the device
/// and the user's settings rather than the snapshot's defaults. [`use_platform_sync`] keeps it
/// current from then on.
pub(crate) fn seed_platform(root: &ServiceProvider) {
    if let Some(platform) = root.service::<PlatformInfoService>() {
        platform.update(untracked(platform_snapshot));
    }
}

/// What the platform answers asynchronously of what it can do (web views, speech), once
/// [`use_platform_sync`] or `AppServices.load()` asked: `None` until then.
#[derive(Clone, Copy, Default)]
struct AnsweredFeatures(Option<Answered>);

#[derive(Clone, Copy)]
struct Answered {
    web_views: bool,
    web_evaluate: bool,
    web_input: bool,
    speech: bool,
}

/// Asks the platform what it answers asynchronously of what it can do, once a surface.
fn ask_features() {
    if global::<AnsweredFeatures>().with_untracked(|answered| answered.0.is_some()) {
        return;
    }
    drop(spawn(async {
        let answered = Answered {
            web_views: web_view_available().await,
            web_evaluate: web_evaluate_available().await,
            web_input: web_input_available().await,
            speech: speech_available().await,
        };
        global::<AnsweredFeatures>().set(AnsweredFeatures(Some(answered)));
    }));
}

/// The device and the user's settings, as NSH's environment reports them now. Reading it in
/// an effect subscribes to every part.
pub fn platform_snapshot() -> PlatformSnapshot {
    let form_factor = match idiom() {
        Idiom::Phone => FormFactor::Phone,
        Idiom::Pad => FormFactor::Tablet,
        Idiom::Tv => FormFactor::Tv,
        // A terminal keeps desktop behavior (NSH's `Idiom::desktop_on`).
        Idiom::Mac | Idiom::Desktop | Idiom::Terminal => FormFactor::Desktop,
    };
    let viewport = window();
    let insets = safe_area();
    let settings = locale_settings();
    let languages: Vec<String> = settings
        .languages
        .iter()
        .map(|tag| tag.as_str().to_owned())
        .collect();
    let answered = global::<AnsweredFeatures>().with(|answered| answered.0);
    let features = PlatformFeatures {
        shaders: shaders_available(),
        screen_color_picker: can_pick_screen_color(),
        web_views: answered.is_some_and(|answered| answered.web_views),
        web_evaluate: answered.is_some_and(|answered| answered.web_evaluate),
        web_input: answered.is_some_and(|answered| answered.web_input),
        speech: answered.is_some_and(|answered| answered.speech),
    };
    PlatformSnapshot {
        family: match os() {
            Os::Ios => PlatformFamily::Ios,
            Os::Android => PlatformFamily::Android,
            Os::MacOs => PlatformFamily::MacOS,
            Os::Windows => PlatformFamily::Windows,
            Os::Linux => PlatformFamily::Linux,
            Os::Web => PlatformFamily::Web,
            Os::Tizen => PlatformFamily::Tizen,
        },
        platform_name: platform_name().to_owned(),
        catalyst: is_catalyst(),
        os_version: os_version(),
        form_factor,
        desktop: is_desktop(),
        input: InputCapabilities::for_form_factor(if is_desktop() {
            FormFactor::Desktop
        } else {
            form_factor
        }),
        apple_keyboard: apple_keys(),
        viewport: ViewportSnapshot {
            width: viewport.width.0,
            height: viewport.height.0,
        },
        scale_factor: pixel_ratio(),
        safe_area: Insets {
            top: insets.top.0,
            right: insets.right.0,
            bottom: insets.bottom.0,
            left: insets.left.0,
        },
        keyboard_height: keyboard_height().0,
        keyboard_animation_millis: keyboard_duration(),
        color_scheme: if dark() {
            ColorScheme::Dark
        } else {
            ColorScheme::Light
        },
        font_scale: font_scale(),
        reduced_motion: reduce_motion(),
        screen_reader: screen_reader(),
        reads_active_descendants: reads_active_descendants(),
        locale: LocaleSnapshot {
            locale: locale().as_str().to_owned(),
            text_direction: text_direction(&languages),
            languages,
            time_zone: settings.time_zone.as_str().to_owned(),
            hour_cycle: settings.hour_cycle.name().to_owned(),
            first_day_of_week: weekday_name(settings.first_day_of_week).to_owned(),
            measurement_system: settings.measurement_system.name().to_owned(),
            temperature_unit: settings.temperature_unit.name().to_owned(),
        },
        terminal_cell: cell_size().map(|cell| ViewportSnapshot {
            width: cell.width.0,
            height: cell.height.0,
        }),
        features,
        displays: displays().iter().map(display_snapshot).collect(),
        window: window_state().map(|state| window_snapshot(&state)),
    }
}

/// The direction the first preferred language's script is written in, as CLDR has it (NSH does
/// not expose the direction it lays the surface out in).
fn text_direction(languages: &[String]) -> TextDirection {
    let first = languages
        .first()
        .and_then(|tag| dxn_intl::parse_locale(tag).ok());
    match first.map(|locale| dxn_intl::direction(&locale)) {
        Some(dxn_intl::Direction::Rtl) => TextDirection::Rtl,
        _ => TextDirection::Ltr,
    }
}

/// `day`'s English name in lowercase, as [`LocaleSnapshot::first_day_of_week`] has it.
fn weekday_name(day: Weekday) -> &'static str {
    match day.number() {
        1 => "monday",
        2 => "tuesday",
        3 => "wednesday",
        4 => "thursday",
        5 => "friday",
        6 => "saturday",
        _ => "sunday",
    }
}

fn rect(bounds: Bounds<Points>) -> PlatformRect {
    PlatformRect {
        x: bounds.origin.x.0,
        y: bounds.origin.y.0,
        width: bounds.size.width.0,
        height: bounds.size.height.0,
    }
}

fn display_snapshot(display: &Display) -> DisplaySnapshot {
    DisplaySnapshot {
        id: format!("{:016x}", display.id.0),
        name: display.name.clone(),
        bounds: rect(display.bounds),
        visible_bounds: rect(display.visible_bounds),
        primary: display.primary,
        scale_factor: display.scale_factor,
        refresh_rate: display.refresh_rate,
    }
}

fn window_snapshot(state: &WindowState) -> WindowSnapshot {
    WindowSnapshot {
        mode: match state.mode {
            NshWindowMode::Normal => WindowMode::Normal,
            NshWindowMode::Maximized => WindowMode::Maximized,
            NshWindowMode::FullScreen => WindowMode::FullScreen,
        },
        minimized: state.minimized,
        visible: state.visible,
        focused: state.focused,
        bounds: rect(state.bounds),
        content_bounds: rect(state.content_bounds),
        display: state.display.map(|id| format!("{:016x}", id.0)),
        scale_factor: state.scale_factor,
    }
}

/// Keeps the surface's platform and suspension services current from NSH's environment:
/// once the app is ready, the [`PlatformInfoService`] gets every change of the device and the
/// user's settings (and what the platform answers of web views and speech), and the
/// [`SuspensionService`] the app's start, every change of NSH's app state (active, inactive,
/// background) and of a desktop's window, every wake of the system from sleep, the user's
/// requests to quit while the app asks first, and the app's quit (`Destroying`, then the
/// storage writes what waits, then `Destroyed`). Call it in the app's root component; it stops
/// with the component.
pub fn use_platform_sync() {
    if global::<PlatformSync>().with_untracked(|sync| sync.running) {
        return;
    }
    global::<PlatformSync>().set(PlatformSync { running: true });
    cleanup(|| global::<PlatformSync>().set(PlatformSync { running: false }));
    sync_platform();
}

/// Whether this surface's platform and suspension services are kept current already (by a Rust
/// root's [`use_platform_sync`] or `AppServices.load()`), so they are not kept twice.
#[derive(Default)]
struct PlatformSync {
    running: bool,
}

/// [`use_platform_sync`] for the surface's life: what `AppServices.load()` starts, since a TSX
/// app has no root component. Call it where the engine runs Rust code (a task).
pub(crate) fn keep_platform_current() {
    if global::<PlatformSync>().with_untracked(|sync| sync.running) {
        return;
    }
    global::<PlatformSync>().set(PlatformSync { running: true });
    Owner::detached().run(sync_platform);
}

/// The effect that copies NSH's environment and app state into the services, and the app's
/// quitting.
fn sync_platform() {
    ask_features();
    let quitting = Rc::new(keep_quitting());
    let seen = Rc::new(Seen::default());
    effect(move || {
        // Reading the app's state subscribes: the effect runs again once it is ready.
        if app_state() != AppState::Ready {
            return;
        }
        let Ok(root) = app_services() else {
            return;
        };
        sync(&root, &seen, &quitting);
    });
}

/// What the effect saw last.
#[derive(Default)]
struct Seen {
    started: Cell<bool>,
    state: Cell<Option<Lifecycle>>,
    window: Cell<Option<WindowActivity>>,
    wakes: Cell<Option<u32>>,
}

fn sync(root: &ServiceProvider, seen: &Seen, quitting: &QuitWiring) {
    quitting.connect(root);
    if let Some(platform) = root.service::<PlatformInfoService>() {
        platform.update(platform_snapshot());
    }
    // Read before anything returns, so the effect runs again after each wake.
    let wakes = wake_count();
    let Some(suspension) = root.service::<SuspensionService>() else {
        return;
    };
    let (started, last_state, last_window) = (&seen.started, &seen.state, &seen.window);
    if !started.replace(true) {
        suspension.publish(SuspensionEvent::new(
            SuspensionEventKind::Started,
            SuspensionEventReason::HostStarted,
        ));
    }
    // A desktop's window: its focus, being minimized and being shown.
    let window = window_state().map(|state| WindowActivity {
        focused: state.focused,
        minimized: state.minimized,
        visible: state.visible,
    });
    if let Some(window) = window
        && last_window.replace(Some(window)) != Some(window)
    {
        suspension.publish(SuspensionEvent::window_changed(
            window,
            SuspensionEventReason::WindowStateChanged,
        ));
    }
    // The wakes since the app was ready (those before it are where it started).
    if let Some(before) = seen.wakes.replace(Some(wakes)) {
        for _ in before..wakes {
            suspension.publish(SuspensionEvent::new(
                SuspensionEventKind::Woke,
                SuspensionEventReason::SystemWoke,
            ));
        }
    }
    let state = lifecycle();
    let previous = last_state.replace(Some(state));
    if previous == Some(state) {
        return;
    }
    let publish = |kind| {
        suspension.publish(SuspensionEvent::new(
            kind,
            SuspensionEventReason::AppStateChanged,
        ));
    };
    match state {
        Lifecycle::Active => {
            if previous == Some(Lifecycle::Background) {
                publish(SuspensionEventKind::Foregrounded);
            }
            publish(SuspensionEventKind::Activated);
        }
        Lifecycle::Inactive => {
            if previous == Some(Lifecycle::Background) {
                publish(SuspensionEventKind::Foregrounded);
            }
            publish(SuspensionEventKind::Deactivated);
        }
        Lifecycle::Background => publish(SuspensionEventKind::Backgrounded),
    }
}
