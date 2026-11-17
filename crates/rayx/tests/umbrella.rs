//! The umbrella crate: one dependency and one prelude reach the core, the services, the NSH
//! layer and the component library.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use futures::executor::block_on;
use rayx::prelude::*;

#[test]
fn the_prelude_builds_an_app_with_the_services() {
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
    // The component library and its theme: the document, its typed keys and modifiers.
    assert!(rayx::components::theme::DOCUMENT.contains("\"spacing-size-default-lvl5-height\""));
    let key = rayx::components::theme::keys::platform::density::size::default::lvl5::HEIGHT;
    assert_eq!(key.name(), "spacing-size-default-lvl5-height");
    assert_eq!(
        rayx::components::theme::modifiers::Accent::DEFAULT,
        rayx::components::theme::modifiers::Accent::Cyan
    );
    let _register = rayx::components::register;
}
