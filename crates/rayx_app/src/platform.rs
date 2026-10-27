//! What RayX's services need from NSH: reading the app's files, registering fonts, and the
//! device's state and the app's lifecycle copied into the platform and suspension services.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

use futures::FutureExt as _;
use futures::future::LocalBoxFuture;
use nsh_core::platform::{AppState as NshAppState, Idiom, Os};
use nsh_core::prelude::{
    dark, effect, fetch, first_day_of_week, font_scale, hour_cycle, idiom, is_desktop,
    keyboard_height, languages, locale, measurement_system, os, os_version, pixel_ratio,
    reduce_motion, register_font, safe_area, screen_reader, temperature_unit, time_zone, window,
};
use rayx_core::{AppBuilder, ServiceProvider};
use rayx_runtime::services::assets::AssetReader;
use rayx_runtime::services::fonts::FontRegistrar;
use rayx_runtime::services::platform::{
    ColorScheme, FormFactor, InputCapabilities, Insets, LocaleSnapshot, PlatformFamily,
    PlatformInfoService, PlatformSnapshot, TextDirection, ViewportSnapshot,
};
use rayx_runtime::services::suspension::{
    SuspensionEvent, SuspensionEventKind, SuspensionEventReason, SuspensionService,
};

use crate::{AppState, app_services, app_state};

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
/// and `dyn FontRegistrar`, unless others are registered. Every surface's builder starts with
/// them ([`start_app`](crate::start_app)).
pub trait NshServicesAppBuilderExt: AppBuilder {
    fn add_nsh_services(mut self) -> Self {
        let services = self.services_mut();
        services.try_add_service_factory_as::<dyn AssetReader>(|_| Arc::new(NshAssetReader));
        services.try_add_service_factory_as::<dyn FontRegistrar>(|_| Arc::new(NshFontRegistrar));
        self
    }
}

impl<T> NshServicesAppBuilderExt for T where T: AppBuilder {}

/// The device and the user's settings, as NSH's environment reports them now. Reading it in
/// an effect subscribes to every part.
pub fn platform_snapshot() -> PlatformSnapshot {
    let form_factor = match idiom() {
        Idiom::Phone => FormFactor::Phone,
        Idiom::Pad => FormFactor::Tablet,
        Idiom::Tv => FormFactor::Tv,
        Idiom::Mac | Idiom::Desktop => FormFactor::Desktop,
    };
    let viewport = window();
    let insets = safe_area();
    let languages: Vec<String> = languages()
        .iter()
        .map(|tag| tag.as_str().to_owned())
        .collect();
    let first_language = languages.first().map(String::as_str).unwrap_or("en");
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
            text_direction: text_direction(first_language),
            languages,
            time_zone: time_zone().as_str().to_owned(),
            hour_cycle: hour_cycle().name().to_owned(),
            first_day_of_week: format!("{:?}", first_day_of_week()).to_lowercase(),
            measurement_system: measurement_system().name().to_owned(),
            temperature_unit: temperature_unit().name().to_owned(),
        },
    }
}

/// Right to left for the languages written so (Arabic, Hebrew, Persian, Urdu, …).
fn text_direction(language: &str) -> TextDirection {
    const RTL: [&str; 9] = ["ar", "he", "iw", "fa", "ur", "ps", "sd", "yi", "dv"];
    let primary = language.split(['-', '_']).next().unwrap_or(language);
    if RTL.contains(&primary.to_ascii_lowercase().as_str()) {
        TextDirection::Rtl
    } else {
        TextDirection::Ltr
    }
}

/// Keeps the surface's platform and suspension services current from NSH's environment:
/// once the app is ready, the [`PlatformInfoService`] gets every change of the device and the
/// user's settings, and the [`SuspensionService`] the app's start and every change of NSH's
/// app state (active, inactive, background). Call it in the app's root component; it stops
/// with the component.
pub fn use_platform_sync() {
    let started = Rc::new(Cell::new(false));
    let last_state = Rc::new(Cell::new(None::<NshAppState>));
    effect(move || {
        // Reading the app's state subscribes: the effect runs again once it is ready.
        if app_state() != AppState::Ready {
            return;
        }
        let Ok(root) = app_services() else {
            return;
        };
        sync(&root, &started, &last_state);
    });
}

fn sync(root: &ServiceProvider, started: &Cell<bool>, last_state: &Cell<Option<NshAppState>>) {
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
    let state = nsh_core::prelude::app_state();
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
        NshAppState::Active => {
            if previous == Some(NshAppState::Background) {
                publish(SuspensionEventKind::Foregrounded);
            }
            publish(SuspensionEventKind::Activated);
        }
        NshAppState::Inactive => {
            if previous == Some(NshAppState::Background) {
                publish(SuspensionEventKind::Foregrounded);
            }
            publish(SuspensionEventKind::Deactivated);
        }
        NshAppState::Background => publish(SuspensionEventKind::Backgrounded),
    }
}
