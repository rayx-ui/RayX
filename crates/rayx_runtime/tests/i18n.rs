//! The i18n service: loading the catalog's translations, following the platform's languages
//! and clock, the app's own language choice, and number, date and plural formatting.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::{Arc, Mutex};

use futures::executor::block_on;
use rayx_core::RayXAppBuilder;
use rayx_core::di::StartupInitializationContext;
use rayx_runtime::services::assets::{AssetReader, MemoryAssetReader};
use rayx_runtime::services::i18n::*;
use rayx_runtime::services::platform::{PlatformAppBuilderExt as _, PlatformInfoService};
use serde_json::json;

fn reader() -> MemoryAssetReader {
    MemoryAssetReader::new()
        .with(
            "asset:/@app/assets.toml",
            "files = [\"locales/en.json\", \"locales/de/translation.json\", \"locales/de/checkout.json\", \"locales/hr.json\"]"
                .as_bytes(),
        )
        .with(
            "asset:/@app/locales/en.json",
            r#"{
                "hello": "Hello, {$name}!",
                "cart": ".input {$n :number}\n.match $n\none {{One item}}\n* {{{$n} items}}",
                "only_en": "English only"
            }"#
            .as_bytes(),
        )
        .with("asset:/@app/locales/de/translation.json", r#"{ "hello": "Hallo, {$name}!" }"#.as_bytes())
        .with("asset:/@app/locales/de/checkout.json", r#"{ "title": "Kasse" }"#.as_bytes())
        .with(
            "asset:/@app/locales/hr.json",
            r#"{ "cart": ".input {$n :number}\n.match $n\none {{{$n} artikl}}\nfew {{{$n} artikla}}\n* {{{$n} artikala}}" }"#
                .as_bytes(),
        )
}

fn start(
    configure: impl Fn(&mut I18nOptions) + Send + Sync + 'static,
    platform: bool,
) -> rayx_core::ServiceProvider {
    let mut builder = RayXAppBuilder::new().add_i18n(configure);
    if platform {
        builder = builder.add_platform();
    }
    builder.configuration.add_value(json!({
        "RayX": { "Services": { "Assets": { "packages": ["app"] } } }
    }));
    builder
        .services
        .add_singleton_as::<dyn AssetReader>(Arc::new(reader()) as Arc<dyn AssetReader>);
    let services = builder.build().unwrap();
    services.validate_options_on_start().unwrap();
    block_on(
        services
            .startup_registry()
            .initialize(StartupInitializationContext::new(services.clone())),
    )
    .unwrap();
    services
}

#[test]
fn loads_the_catalogs_translations_by_language_and_namespace() {
    let services = start(|_| {}, false);
    let i18n = services.required_service::<I18nService>().unwrap();
    let mut languages = i18n.available_languages();
    languages.sort();
    assert_eq!(languages, ["de", "en", "hr"]);
    assert_eq!(i18n.language(), "en");
    assert_eq!(
        i18n.format("hello", &Args::new().with("name", "Ana")),
        "Hello, Ana!"
    );
    i18n.set_language("de").unwrap();
    assert_eq!(
        i18n.format("hello", &Args::new().with("name", "Ana")),
        "Hallo, Ana!"
    );
    // Another namespace prefixes its ids; missing messages fall back to the default language.
    assert_eq!(i18n.t("checkout.title"), "Kasse");
    assert_eq!(i18n.t("only_en"), "English only");
    assert_eq!(i18n.t("nowhere"), "nowhere");
}

#[test]
fn plurals_follow_the_language() {
    let services = start(|_| {}, false);
    let i18n = services.required_service::<I18nService>().unwrap();
    assert_eq!(i18n.format("cart", &Args::new().with("n", 1)), "One item");
    i18n.set_language("hr").unwrap();
    assert_eq!(i18n.format("cart", &Args::new().with("n", 3)), "3 artikla");
    assert_eq!(i18n.format("cart", &Args::new().with("n", 5)), "5 artikala");
    assert_eq!(i18n.plural_category(21), dxn_intl::PluralCategory::One);
}

#[test]
fn follows_the_platforms_languages_and_clock_until_the_app_picks() {
    let services = start(|_| {}, true);
    let i18n = services.required_service::<I18nService>().unwrap();
    let platform = services.required_service::<PlatformInfoService>().unwrap();
    let changes = Arc::new(Mutex::new(Vec::new()));
    let seen = Arc::clone(&changes);
    let _subscription =
        i18n.subscribe(move |event| seen.lock().unwrap().push(event.locale.to_string()));

    platform.modify(|snapshot| {
        snapshot.locale.languages = vec!["fr-FR".into(), "de-AT".into()];
        snapshot.locale.hour_cycle = "h23".into();
    });
    // French is not there: the next language, with the user's region and clock.
    assert_eq!(i18n.locale().to_string(), "de-AT-u-hc-h23");
    assert_eq!(i18n.t("hello"), "Hallo, {$name}!");
    let when = dxn_intl::DateTime::new(2026, 10, 27, 15, 45, 0).unwrap();
    assert_eq!(
        i18n.format_date_time(
            &when,
            dxn_intl::DateTimeOptions::time(dxn_intl::Length::Short)
        )
        .unwrap(),
        "15:45"
    );
    assert_eq!(changes.lock().unwrap().last().unwrap(), "de-AT-u-hc-h23");

    i18n.set_language("hr").unwrap();
    platform.modify(|snapshot| snapshot.locale.languages = vec!["en-GB".into()]);
    assert_eq!(i18n.language(), "hr");
    i18n.follow_platform_again(&platform.current().locale);
    assert_eq!(i18n.locale().to_string(), "en-GB-u-hc-h23");
}

#[test]
fn numbers_and_dates_format_for_the_locale() {
    let services = start(|_| {}, false);
    let i18n = services.required_service::<I18nService>().unwrap();
    i18n.set_language("de-CH").unwrap();
    assert_eq!(
        i18n.format_number(1234567, dxn_intl::NumberOptions::default())
            .unwrap(),
        "1'234'567"
    );
    assert_eq!(
        i18n.format_number(5, dxn_intl::NumberOptions::currency("CHF"))
            .unwrap(),
        "CHF\u{a0}5.00"
    );
    i18n.set_language("ar").unwrap();
    assert_eq!(i18n.direction(), dxn_intl::Direction::Rtl);
}

#[test]
fn options_bind_from_configuration_and_resources_add_at_run_time() {
    let mut builder = RayXAppBuilder::new().add_i18n(|options| {
        options.auto_load_assets(false);
    });
    builder.configuration.add_value(json!({
        "RayX": { "Services": { "I18N": { "default_language": "hr", "missing_message": "empty" } } }
    }));
    let services = builder.build().unwrap();
    let i18n = services.required_service::<I18nService>().unwrap();
    assert_eq!(i18n.language(), "hr");
    assert_eq!(i18n.t("missing"), "");
    i18n.add_resource("hr", Some("shop"), r#"{ "buy": "Kupi" }"#)
        .unwrap();
    assert_eq!(i18n.t("shop.buy"), "Kupi");
    assert!(
        i18n.add_resource("hr", None, r#"{ "ok": "U redu", "bad": "{$x" }"#)
            .is_err()
    );
    assert_eq!(i18n.t("ok"), "U redu");
    assert!(matches!(
        i18n.set_language("not a tag"),
        Err(I18nServiceError::InvalidLanguage(_))
    ));
}

#[test]
fn invalid_language_options_fail_startup() {
    let builder = RayXAppBuilder::new().add_i18n(|options| {
        options
            .default_language("not a tag")
            .auto_load_assets(false);
    });
    let services = builder.build().unwrap();
    assert!(services.validate_options_on_start().is_err());
}
