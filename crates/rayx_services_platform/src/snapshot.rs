//! The device and the user's settings, as NSH's environment reports them.

use nsh_core::desktop::{Display, WindowMode as NshWindowMode, WindowState};
use nsh_core::element::cell_size;
use nsh_core::element::desktop::{displays, window_state};
use nsh_core::geometry::{Bounds, Points};
use nsh_core::platform::{Idiom, Os, Weekday};
use nsh_core::prelude::{
    apple_keys, can_pick_screen_color, dark, font_scale, global, idiom, is_catalyst, is_desktop,
    keyboard_duration, keyboard_height, locale, locale_settings, os, os_version, pixel_ratio,
    platform_name, reads_active_descendants, reduce_motion, safe_area, screen_reader,
    shaders_available, spawn, web_evaluate_available, web_input_available, web_view_available,
    window,
};
use rayx_app::services::platform::{
    ColorScheme, DisplaySnapshot, FormFactor, InputCapabilities, Insets, LocaleSnapshot,
    PlatformFamily, PlatformFeatures, PlatformRect, PlatformSnapshot, TextDirection,
    ViewportSnapshot, WindowMode, WindowSnapshot,
};

/// What the platform answers asynchronously of what it can do (web views), once
/// the platform service's initialization asked: `None` until then.
#[derive(Clone, Copy, Default)]
struct AnsweredFeatures(Option<Answered>);

#[derive(Clone, Copy)]
struct Answered {
    web_views: bool,
    web_evaluate: bool,
    web_input: bool,
}

/// Asks the platform what it answers asynchronously of what it can do, once a surface.
pub(crate) fn ask_features() {
    if global::<AnsweredFeatures>().with_untracked(|answered| answered.0.is_some()) {
        return;
    }
    drop(spawn(async {
        let answered = Answered {
            web_views: web_view_available().await,
            web_evaluate: web_evaluate_available().await,
            web_input: web_input_available().await,
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
