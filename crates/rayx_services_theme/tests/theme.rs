//! RayX's theme service without an engine (the platform a `FakePlatform`): `add_theme`
//! registers it as the app's `dyn ThemeService`, replacing any other, requires a platform
//! service, binds its options from `RayX:Services:Theme` and `configure`, refuses invalid ones at
//! startup naming the entry, and starts. Outside an engine there is no NSH to pick on: the
//! commands validate and set the light/dark choice, and the snapshot lists no modifiers.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::{Arc, Mutex};

use futures::executor::block_on;
use rayx_app::di::{ServiceValidationProblem, StartupInitializationContext};
use rayx_app::services::platform::{ColorScheme, PlatformInfoService, PlatformSnapshot};
use rayx_app::services::theme::{ColorModeChoice, ThemeError, ThemeService, ThemeSnapshot};
use rayx_app::testing::{FakePlatform, FakeTheme, add_host_fakes};
use rayx_app::{AppBuildError, RayXAppBuilder, ServiceProvider, ServiceValidationError};
use rayx_services_theme::{
    ColorModeOptions, THEME_CONFIG_PATH, ThemeAppBuilderExt as _, ThemeOptions,
};
use serde_json::json;

/// Builds `builder` with the host's fakes (a platform at `scheme`, which the test changes) and
/// runs its startup; the provider and the platform, or startup's error.
fn start(
    builder: RayXAppBuilder,
    scheme: ColorScheme,
) -> (ServiceProvider, Arc<FakePlatform>, Result<(), String>) {
    let mut builder = builder;
    let platform = FakePlatform::new(PlatformSnapshot {
        color_scheme: scheme,
        ..PlatformSnapshot::default()
    });
    builder
        .services
        .add_singleton_as::<dyn PlatformInfoService>(
            Arc::clone(&platform) as Arc<dyn PlatformInfoService>
        );
    let services = add_host_fakes(builder).build().unwrap();
    let started = block_on(
        services
            .startup_registry()
            .initialize(StartupInitializationContext::new(services.clone())),
    )
    .map_err(|error| error.to_string());
    (services, platform, started)
}

fn theme(services: &ServiceProvider) -> Arc<dyn ThemeService> {
    services.required_service::<dyn ThemeService>().unwrap()
}

#[test]
fn add_theme_registers_rayx_s_service_replacing_any_before() {
    let earlier = FakeTheme::new(ThemeSnapshot {
        revision: 7,
        ..ThemeSnapshot::default()
    });
    let mut builder = RayXAppBuilder::new();
    builder
        .services
        .add_singleton_as::<dyn ThemeService>(Arc::clone(&earlier) as Arc<dyn ThemeService>);
    let (services, _, started) = start(builder.add_theme(|_| {}), ColorScheme::Light);
    started.unwrap();
    let rayx = theme(&services);
    assert_eq!(
        rayx.snapshot().revision,
        0,
        "the earlier service is still registered"
    );
    assert!(Arc::ptr_eq(&rayx, &theme(&services)));

    // The app's own registration after it replaces RayX's, startup service included.
    let mine = FakeTheme::new(ThemeSnapshot {
        revision: 9,
        ..ThemeSnapshot::default()
    });
    let mut builder = RayXAppBuilder::new().add_theme(|options| {
        options.color_mode_modifier("", "light", "dark");
    });
    builder
        .services
        .add_singleton_as::<dyn ThemeService>(Arc::clone(&mine) as Arc<dyn ThemeService>);
    let (services, _, started) = start(builder, ColorScheme::Light);
    started.unwrap();
    assert_eq!(theme(&services).snapshot().revision, 9);
}

#[test]
fn building_without_a_platform_service_fails_with_its_hint() {
    let builder = RayXAppBuilder::new().add_theme(|_| {});
    let Err(AppBuildError::Services(ServiceValidationError { problems })) = builder.build() else {
        panic!("built without a platform service");
    };
    assert!(
        problems.iter().any(|problem| matches!(
            problem,
            ServiceValidationProblem::Missing { needed, by, hint }
                if needed.contains("PlatformInfoService")
                    && by.as_deref().is_some_and(|by| by.contains("ThemeService"))
                    && hint.contains("add_platform()")
        )),
        "no platform hint in {problems:?}"
    );
}

#[test]
fn options_bind_from_configuration_then_configure() {
    assert_eq!(THEME_CONFIG_PATH, "RayX:Services:Theme");
    let defaults = ThemeOptions::default();
    assert_eq!(
        defaults.color_mode,
        ColorModeOptions {
            modifier: "colorMode".into(),
            light: "light".into(),
            dark: "dark".into(),
            choice: ColorModeChoice::System,
        }
    );
    assert!(defaults.contexts.is_empty());

    let mut builder = RayXAppBuilder::new().add_theme(|options| {
        options.context("density", "compact");
    });
    builder.configuration.add_value(json!({
        "RayX": { "Services": { "Theme": {
            "colorMode": { "modifier": "mode", "light": "day", "dark": "night", "choice": "Dark" },
            "contexts": { "accent": "cyan", "density": "comfortable" }
        } } }
    }));
    let (services, _, started) = start(builder, ColorScheme::Light);
    started.unwrap();
    let options = services.options::<ThemeOptions>().unwrap().cloned();
    assert_eq!(options.color_mode.modifier, "mode");
    assert_eq!(options.color_mode.light, "day");
    assert_eq!(options.color_mode.dark, "night");
    assert_eq!(options.color_mode.choice, ColorModeChoice::Dark);
    assert_eq!(options.contexts["accent"], "cyan");
    // `configure` comes after configuration.
    assert_eq!(options.contexts["density"], "compact");
    // The service starts at the bound choice.
    assert_eq!(theme(&services).color_mode(), ColorModeChoice::Dark);
}

#[test]
fn invalid_options_fail_startup_naming_the_entry() {
    for (configure, entry) in [
        (
            Box::new(|options: &mut ThemeOptions| {
                options.color_mode_modifier("", "light", "dark");
            }) as Box<dyn Fn(&mut ThemeOptions) + Send + Sync>,
            "colorMode.modifier",
        ),
        (
            Box::new(|options: &mut ThemeOptions| {
                options.color_mode_modifier("colorMode", "", "dark");
            }),
            "colorMode.light",
        ),
        (
            Box::new(|options: &mut ThemeOptions| {
                options.color_mode_modifier("colorMode", "light", " ");
            }),
            "colorMode.dark",
        ),
        (
            Box::new(|options: &mut ThemeOptions| {
                options.context("", "cyan");
            }),
            "contexts",
        ),
        (
            Box::new(|options: &mut ThemeOptions| {
                options.context("accent", "");
            }),
            "contexts.accent",
        ),
        (
            Box::new(|options: &mut ThemeOptions| {
                options.context("colorMode", "dark");
            }),
            "contexts.colorMode",
        ),
    ] {
        let builder = RayXAppBuilder::new().add_theme(configure);
        let (_, _, started) = start(builder, ColorScheme::Light);
        let error = started.expect_err(entry);
        assert!(
            error.contains(&format!("`{entry}`")),
            "`{entry}` not named in: {error}"
        );
    }

    // The light/dark modifier's entry is refused whatever the modifier is called.
    let builder = RayXAppBuilder::new().add_theme(|options| {
        options
            .color_mode_modifier("mode", "day", "night")
            .context("mode", "night");
    });
    let (_, _, started) = start(builder, ColorScheme::Light);
    let error = started.unwrap_err();
    assert!(
        error.contains("`contexts.mode`") && error.contains("colorMode.choice"),
        "{error}"
    );
}

#[test]
fn outside_an_engine_the_service_starts_with_nothing_to_pick() {
    let builder = RayXAppBuilder::new().add_theme(|options| {
        options.context("accent", "cyan");
    });
    let (services, _, started) = start(builder, ColorScheme::Dark);
    started.unwrap();
    let theme = theme(&services);
    let snapshot = theme.snapshot();
    assert!(snapshot.modifiers.is_empty(), "{snapshot:?}");
    assert_eq!(snapshot.color_mode, ColorModeChoice::System);
    assert_eq!(theme.selected("colorMode"), None);
    // No document: every pick of a modifier is refused, and changes nothing.
    assert_eq!(
        block_on(theme.set_context("accent".into(), "cyan".into())),
        Err(ThemeError::UnknownModifier {
            modifier: "accent".into()
        })
    );
    assert_eq!(theme.snapshot(), snapshot);
}

#[test]
fn the_color_mode_maps_the_platform_s_scheme_and_the_choice() {
    let options = ColorModeOptions::default();
    let platform = FakePlatform::new(PlatformSnapshot::default());
    for (scheme, system) in [(ColorScheme::Light, "light"), (ColorScheme::Dark, "dark")] {
        platform.modify(|snapshot| snapshot.color_scheme = scheme);
        let scheme = platform.platform().get().color_scheme;
        assert_eq!(options.context_for(ColorModeChoice::System, scheme), system);
        assert_eq!(options.context_for(ColorModeChoice::Light, scheme), "light");
        assert_eq!(options.context_for(ColorModeChoice::Dark, scheme), "dark");
    }
    assert_eq!(options.choice_of("light"), Some(ColorModeChoice::Light));
    assert_eq!(options.choice_of("dark"), Some(ColorModeChoice::Dark));
    assert_eq!(options.choice_of("sepia"), None);
}

#[test]
fn the_choice_follows_the_commands_whatever_the_scheme_does() {
    let (services, platform, started) =
        start(RayXAppBuilder::new().add_theme(|_| {}), ColorScheme::Light);
    started.unwrap();
    let theme = theme(&services);
    let heard = Arc::new(Mutex::new(Vec::new()));
    let heard_by = Arc::clone(&heard);
    let _subscription = theme.theme().subscribe(move |snapshot| {
        heard_by
            .lock()
            .unwrap()
            .push((snapshot.color_mode, snapshot.revision));
    });

    // The scheme changing publishes nothing: the choice is `System`, and outside an engine
    // there is no modifier to move.
    platform.modify(|snapshot| snapshot.color_scheme = ColorScheme::Dark);
    platform.modify(|snapshot| snapshot.color_scheme = ColorScheme::Light);
    assert_eq!(theme.color_mode(), ColorModeChoice::System);
    assert!(heard.lock().unwrap().is_empty());

    assert!(block_on(theme.set_color_mode(ColorModeChoice::Dark)).unwrap());
    assert!(!block_on(theme.set_color_mode(ColorModeChoice::Dark)).unwrap());
    platform.modify(|snapshot| snapshot.color_scheme = ColorScheme::Light);
    assert_eq!(theme.color_mode(), ColorModeChoice::Dark);
    assert!(block_on(theme.set_color_mode(ColorModeChoice::System)).unwrap());
    assert_eq!(
        *heard.lock().unwrap(),
        [(ColorModeChoice::Dark, 1), (ColorModeChoice::System, 2)]
    );

    // The light/dark modifier takes only its light and dark contexts.
    assert_eq!(
        block_on(theme.set_context("colorMode".into(), "sepia".into())),
        Err(ThemeError::UnknownContext {
            modifier: "colorMode".into(),
            context: "sepia".into()
        })
    );
    assert_eq!(theme.color_mode(), ColorModeChoice::System);
    assert_eq!(heard.lock().unwrap().len(), 2);
}
