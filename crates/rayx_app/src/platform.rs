//! What RayX's services need from NSH: reading the app's files, registering fonts, and the
//! device's state and the app's lifecycle copied into the platform and suspension services.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

use futures::FutureExt as _;
use futures::future::LocalBoxFuture;
use nsh_core::desktop::{Display, WindowMode as NshWindowMode, WindowState};
use nsh_core::element::desktop::{displays, window_state};
use nsh_core::geometry::{Bounds, Points};
use nsh_core::platform::{Idiom, Lifecycle, Os, Weekday};
use nsh_core::prelude::{
    Owner, cleanup, dark, effect, fetch, font_scale, global, idiom, is_desktop, keyboard_height,
    lifecycle, locale, locale_settings, os, os_version, pixel_ratio, reduce_motion, register_font,
    safe_area, screen_reader, untracked, window,
};
use rayx_core::{AppBuilder, ServiceProvider};
use rayx_runtime::services::assets::AssetReader;
use rayx_runtime::services::fonts::FontRegistrar;
use rayx_runtime::services::platform::{
    ColorScheme, DisplaySnapshot, FormFactor, InputCapabilities, Insets, LocaleSnapshot,
    PlatformFamily, PlatformInfoService, PlatformRect, PlatformSnapshot, TextDirection,
    ViewportSnapshot, WindowMode, WindowSnapshot,
};
use rayx_runtime::services::storage::KeyValueStore;
use rayx_runtime::services::suspension::{
    SuspensionEvent, SuspensionEventKind, SuspensionEventReason, SuspensionService, WindowActivity,
};

use rayx_runtime::services::navigation::NavigationLocationSynchronizer;

use crate::{AppState, NshKeyValueStore, NshLocationSynchronizer, app_services, app_state};

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
/// their family names.
#[derive(Clone, Copy, Debug, Default)]
pub struct NshFontRegistrar;

impl FontRegistrar for NshFontRegistrar {
    fn register(&self, data: Vec<u8>) -> LocalBoxFuture<'static, Result<String, String>> {
        async move { register_font(data).await }.boxed_local()
    }
}

/// Registers NSH's [`NshAssetReader`] and [`NshFontRegistrar`] as the app's `dyn AssetReader`
/// and `dyn FontRegistrar`, its [`NshKeyValueStore`] (a startup service, which loads on the
/// engine thread) as its `dyn KeyValueStore` and its [`NshLocationSynchronizer`] as its
/// `dyn NavigationLocationSynchronizer`, unless others are registered. Every surface's builder
/// starts with them ([`start_app`](crate::start_app)), so on NSH `add_storage()` keeps NSH's
/// persistent store and `add_navigation()` shows its location as NSH's.
pub trait NshServicesAppBuilderExt: AppBuilder {
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
    PlatformSnapshot {
        family: match os() {
            Os::Ios => PlatformFamily::Ios,
            Os::Android => PlatformFamily::Android,
            Os::MacOs => PlatformFamily::MacOS,
            Os::Windows => PlatformFamily::Windows,
            Os::Linux => PlatformFamily::Linux,
            Os::Web => PlatformFamily::Web,
        },
        os_version: os_version(),
        form_factor,
        desktop: is_desktop(),
        input: InputCapabilities::for_form_factor(if is_desktop() {
            FormFactor::Desktop
        } else {
            form_factor
        }),
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
        color_scheme: if dark() {
            ColorScheme::Dark
        } else {
            ColorScheme::Light
        },
        font_scale: font_scale(),
        reduced_motion: reduce_motion(),
        screen_reader: screen_reader(),
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
/// user's settings, and the [`SuspensionService`] the app's start and every change of NSH's
/// app state (active, inactive, background). Call it in the app's root component; it stops
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

/// The effect that copies NSH's environment and app state into the services.
fn sync_platform() {
    let started = Rc::new(Cell::new(false));
    let last_state = Rc::new(Cell::new(None::<Lifecycle>));
    let last_window = Rc::new(Cell::new(None::<WindowActivity>));
    effect(move || {
        // Reading the app's state subscribes: the effect runs again once it is ready.
        if app_state() != AppState::Ready {
            return;
        }
        let Ok(root) = app_services() else {
            return;
        };
        sync(&root, &started, &last_state, &last_window);
    });
}

fn sync(
    root: &ServiceProvider,
    started: &Cell<bool>,
    last_state: &Cell<Option<Lifecycle>>,
    last_window: &Cell<Option<WindowActivity>>,
) {
    if let Some(platform) = root.service::<PlatformInfoService>() {
        platform.update(platform_snapshot());
    }
    let Some(suspension) = root.service::<SuspensionService>() else {
        return;
    };
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
