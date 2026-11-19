//! The umbrella crate: one dependency and one prelude reach the core, the services the features
//! enable (`rayx::services::<name>`, their builder extensions in the prelude), the NSH layer and
//! the component library. Its groups run under the default features (the four services the host
//! requires) and under every feature.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use rayx::prelude::*;

#[cfg(all(
    feature = "platform",
    feature = "suspension",
    feature = "assets",
    feature = "fonts"
))]
#[test]
fn the_defaults_bring_the_four_services_the_host_requires() {
    use rayx::app::services::assets::AssetService;
    use rayx::app::services::fonts::FontService;
    use rayx::app::services::platform::PlatformInfoService;
    use rayx::app::services::suspension::SuspensionService;

    // Their builder extensions are in the prelude.
    let builder = RayXAppBuilder::new()
        .add_platform()
        .add_suspension()
        .add_assets(|_| {})
        .add_fonts(|_| {});
    let services = builder.build().unwrap();
    assert!(services.service::<dyn PlatformInfoService>().is_some());
    assert!(services.service::<dyn SuspensionService>().is_some());
    assert!(services.service::<dyn AssetService>().is_some());
    assert!(services.service::<dyn FontService>().is_some());
    // Their crates are `rayx::services::<name>`, their TS classes there.
    let _: [fn(&mut Registry); 4] = [
        rayx::services::platform::register,
        rayx::services::suspension::register,
        rayx::services::assets::register,
        rayx::services::fonts::register,
    ];
    let _: Option<&rayx::services::platform::PlatformInfoService> = None;
    let _: Option<&rayx::services::suspension::SuspensionService> = None;
    let _: Option<&rayx::services::assets::AssetService> = None;
    let _: Option<&rayx::services::fonts::FontService> = None;
}

#[cfg(all(feature = "operations", feature = "history", feature = "i18n"))]
#[test]
fn the_prelude_builds_an_app_with_the_services() {
    use futures::executor::block_on;

    let builder = RayXAppBuilder::new()
        .add_operations()
        .add_history()
        .add_i18n(|options| {
            options
                .auto_load_assets(false)
                .follow_platform(false)
                .default_language("hr");
        });
    // The platform and asset services `add_i18n` requires.
    let services = rayx::app::testing::add_host_fakes(builder).build().unwrap();
    let i18n = services.required_service::<dyn I18nService>().unwrap();
    block_on(i18n.add_resource("hr".into(), None, r#"{ "hi": "Bok, {$name}!" }"#.into())).unwrap();
    let formatter = services.required_service::<dyn MessageFormatter>().unwrap();
    assert_eq!(
        formatter.format("hi", &Args::new().with("name", "Ana")),
        "Bok, Ana!"
    );
    // `tr!` is the id outside an engine.
    assert_eq!(tr!("hi", name = "Ana"), "hi");
    assert!(
        services
            .service::<dyn rayx::app::services::history::HistoryService>()
            .is_some()
    );
    assert!(
        services
            .service::<dyn rayx::app::services::operations::OperationService>()
            .is_some()
    );
}

#[test]
fn the_crates_are_reachable_by_name() {
    let _: fn() -> rayx::app::RayXAppBuilder = rayx::app::RayXAppBuilder::new;
    let locale = rayx::intl::parse_locale("de-AT").unwrap();
    assert_eq!(locale.to_string(), "de-AT");
    let message = rayx::i18n::Message::parse("Hello").unwrap();
    assert_eq!(message.format(&locale, &rayx::i18n::Args::new()), "Hello");
    // The TSX classes, and a Rust root's start.
    let _: Option<&rayx::app_nsh::ts::ServiceProvider> = None;
    let _: fn(RayXAppBuilder) = StartAppBuilderExt::start;
    // The screen's (or the surface root's) provider, for Rust components.
    let _: fn() -> Result<ServiceProvider, AppError> = services;
    // The component library and its theme: the default document (tokens named by their DTCG
    // paths), RayX's roles and their reads.
    assert!(
        rayx::components::theme::DOCUMENT.contains("\"platform-density-size-default-lvl5-height\"")
    );
    let role = rayx::components::theme::roles::SPACING_DEFAULT_LVL5_HEIGHT;
    assert_eq!(role.name(), "spacing-default-lvl5-height");
    assert!(
        rayx::components::theme::roles::all()
            .iter()
            .any(|r| r.name == role.name() && r.required)
    );
    let _: fn(nsh_core::element::Role) -> nsh_core::color::Hsla = rayx::components::theme::value;
    let _register = rayx::components::register;
}

#[cfg(all(
    feature = "application_configuration",
    feature = "history",
    feature = "i18n",
    feature = "input_map",
    feature = "navigation",
    feature = "operations",
    feature = "selection",
    feature = "storage",
    feature = "theme"
))]
#[test]
fn every_other_service_crate_is_reachable_under_its_feature() {
    let _: [fn(&mut Registry); 9] = [
        rayx::services::application_configuration::register,
        rayx::services::history::register,
        rayx::services::i18n::register,
        rayx::services::input_map::register,
        rayx::services::navigation::register,
        rayx::services::operations::register,
        rayx::services::selection::register,
        rayx::services::storage::register,
        rayx::services::theme::register,
    ];
    // Their builder extensions are in the prelude.
    let _ = RayXAppBuilder::new()
        .add_application_configuration(|_| {})
        .add_input_map(|_| {})
        .add_navigation(|_| {})
        .add_selection()
        .add_storage()
        .add_operations()
        .add_history()
        .add_theme(|_| {});
    #[allow(unused_imports)]
    use rayx::prelude::{navigation_region, navigation_view};
    // `tr!` comes with `i18n`, and its checked form (which reads an app's messages when it
    // builds, so it is only named here).
    assert_eq!(tr!("hi"), "hi");
    #[allow(unused_imports)]
    use rayx::checked::tr as checked_tr;
}
