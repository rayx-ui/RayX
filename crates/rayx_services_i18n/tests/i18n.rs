//! RayX's i18n service and formatter: loading the catalog's translations, following the
//! platform's languages and clock, the app's own language choice, and number, date and plural
//! formatting.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::{Arc, Mutex};

use futures::executor::block_on;
use rayx_app::di::{ServiceValidationProblem, StartupInitializationContext};
use rayx_app::services::assets::{AssetCatalog, AssetService};
use rayx_app::services::i18n::{I18nService, I18nServiceError, MessageFormatter};
use rayx_app::services::platform::{PlatformInfoService, PlatformSnapshot, TextDirection};
use rayx_app::testing::{FakeAssets, FakePlatform, add_host_fakes};
use rayx_app::{AppBuildError, RayXAppBuilder, ServiceProvider, ServiceValidationError};
use rayx_services_i18n::{Args, I18nAppBuilderExt as _, I18nOptions};
use serde_json::json;

/// The app's assets: its manifest's locale files.
fn assets() -> Arc<dyn AssetService> {
    let mut catalog = AssetCatalog::builder();
    catalog
        .add_manifest_text(
            "app",
            "files = [\"locales/en.json\", \"locales/de/translation.json\", \"locales/de/checkout.json\", \"locales/hr.json\"]",
        )
        .unwrap();
    FakeAssets::new(catalog.build())
        .file(
            "@app/locales/en.json",
            r#"{
                "hello": "Hello, {$name}!",
                "cart": ".input {$n :number}\n.match $n\none {{One item}}\n* {{{$n} items}}",
                "only_en": "English only"
            }"#,
        )
        .file("@app/locales/de/translation.json", r#"{ "hello": "Hallo, {$name}!" }"#)
        .file("@app/locales/de/checkout.json", r#"{ "title": "Kasse" }"#)
        .file(
            "@app/locales/hr.json",
            r#"{ "cart": ".input {$n :number}\n.match $n\none {{{$n} artikl}}\nfew {{{$n} artikla}}\n* {{{$n} artikala}}" }"#,
        ) as Arc<dyn AssetService>
}

/// Changes the fake platform's snapshot with `change` and publishes it.
fn modify(platform: &FakePlatform, change: impl FnOnce(&mut PlatformSnapshot)) {
    let mut snapshot = (*platform.platform().get()).clone();
    change(&mut snapshot);
    platform.publish(snapshot);
}

/// Registers a fake platform as the app's `dyn PlatformInfoService` (and as itself, for the
/// test to change).
fn add_fake_platform(services: &mut ServiceProvider) {
    let platform = FakePlatform::new(PlatformSnapshot::default());
    services.add_singleton_as::<FakePlatform>(Arc::clone(&platform));
    services.add_singleton_as::<dyn PlatformInfoService>(platform as Arc<dyn PlatformInfoService>);
}

fn start(
    configure: impl Fn(&mut I18nOptions) + Send + Sync + 'static,
    platform: bool,
) -> rayx_app::ServiceProvider {
    // Without a platform of the test's own, the host fakes' one, which the service does not
    // follow.
    let mut builder = RayXAppBuilder::new().add_i18n(move |options| {
        options.follow_platform(platform);
        configure(options);
    });
    if platform {
        add_fake_platform(&mut builder.services);
    }
    builder
        .services
        .add_singleton_as::<dyn AssetService>(assets());
    let services = add_host_fakes(builder).build().unwrap();
    services.validate_options_on_start().unwrap();
    block_on(
        services
            .startup_registry()
            .initialize(StartupInitializationContext::new(services.clone())),
    )
    .unwrap();
    services
}

/// The app's i18n service and formatter.
fn i18n(services: &ServiceProvider) -> (Arc<dyn I18nService>, Arc<dyn MessageFormatter>) {
    (
        services.required_service::<dyn I18nService>().unwrap(),
        services.required_service::<dyn MessageFormatter>().unwrap(),
    )
}

fn set_language(i18n: &Arc<dyn I18nService>, language: &str) -> Result<(), I18nServiceError> {
    block_on(i18n.set_language(language.to_owned()))
}

#[test]
fn loads_the_catalogs_translations_by_language_and_namespace() {
    let services = start(|_| {}, false);
    let (i18n, formatter) = i18n(&services);
    let mut languages = i18n.available_languages();
    languages.sort();
    assert_eq!(languages, ["de", "en", "hr"]);
    assert_eq!(i18n.language(), "en");
    assert_eq!(
        formatter.format("hello", &Args::new().with("name", "Ana")),
        "Hello, Ana!"
    );
    set_language(&i18n, "de").unwrap();
    assert_eq!(
        formatter.format("hello", &Args::new().with("name", "Ana")),
        "Hallo, Ana!"
    );
    // Another namespace prefixes its ids; missing messages fall back to the default language.
    assert_eq!(formatter.t("checkout.title"), "Kasse");
    assert_eq!(formatter.t("only_en"), "English only");
    assert_eq!(formatter.t("nowhere"), "nowhere");
}

#[test]
fn plurals_follow_the_language() {
    let services = start(|_| {}, false);
    let (i18n, formatter) = i18n(&services);
    assert_eq!(
        formatter.format("cart", &Args::new().with("n", 1)),
        "One item"
    );
    set_language(&i18n, "hr").unwrap();
    assert_eq!(
        formatter.format("cart", &Args::new().with("n", 3)),
        "3 artikla"
    );
    assert_eq!(
        formatter.format("cart", &Args::new().with("n", 5)),
        "5 artikala"
    );
    assert_eq!(
        formatter.plural_category(21.into()),
        dxn_intl::PluralCategory::One
    );
}

#[test]
fn follows_the_platforms_languages_and_clock_until_the_app_picks() {
    let services = start(|_| {}, true);
    let (i18n, formatter) = i18n(&services);
    let platform = services.required_service::<FakePlatform>().unwrap();
    let changes = Arc::new(Mutex::new(Vec::new()));
    let seen = Arc::clone(&changes);
    let _subscription = i18n
        .i18n()
        .subscribe(move |state| seen.lock().unwrap().push(state.locale.clone()));

    modify(&platform, |snapshot| {
        snapshot.locale.languages = vec!["fr-FR".into(), "de-AT".into()];
        snapshot.locale.hour_cycle = "h23".into();
    });
    // French is not there: the next language, with the user's region and clock.
    assert_eq!(i18n.locale(), "de-AT-u-hc-h23");
    assert_eq!(formatter.locale().to_string(), "de-AT-u-hc-h23");
    assert_eq!(formatter.t("hello"), "Hallo, {$name}!");
    let when = dxn_intl::DateTime::new(2026, 10, 27, 15, 45, 0).unwrap();
    assert_eq!(
        formatter
            .format_date_time(
                &when,
                dxn_intl::DateTimeOptions::time(dxn_intl::Length::Short)
            )
            .unwrap(),
        "15:45"
    );
    assert_eq!(changes.lock().unwrap().last().unwrap(), "de-AT-u-hc-h23");

    set_language(&i18n, "hr").unwrap();
    modify(&platform, |snapshot| {
        snapshot.locale.languages = vec!["en-GB".into()]
    });
    assert_eq!(i18n.language(), "hr");
    assert_eq!(i18n.chosen_language().as_deref(), Some("hr"));
    block_on(i18n.follow_platform()).unwrap();
    assert_eq!(i18n.locale(), "en-GB-u-hc-h23");
    assert_eq!(i18n.chosen_language(), None);
}

#[test]
fn numbers_and_dates_format_for_the_locale() {
    let services = start(|_| {}, false);
    let (i18n, formatter) = i18n(&services);
    set_language(&i18n, "de-CH").unwrap();
    assert_eq!(
        formatter
            .format_number(1234567.into(), dxn_intl::NumberOptions::default())
            .unwrap(),
        "1'234'567"
    );
    assert_eq!(
        formatter
            .format_number(5.into(), dxn_intl::NumberOptions::currency("CHF"))
            .unwrap(),
        "CHF\u{a0}5.00"
    );
    set_language(&i18n, "ar").unwrap();
    assert_eq!(i18n.direction(), TextDirection::Rtl);
}

#[test]
fn instants_show_in_the_user_s_time_zone() {
    let services = start(|_| {}, true);
    let (i18n, formatter) = i18n(&services);
    let platform = services.required_service::<FakePlatform>().unwrap();
    assert_eq!(i18n.time_zone(), "UTC", "before the platform says");
    let heard = Arc::new(Mutex::new(0));
    let count = Arc::clone(&heard);
    let _subscription = i18n.i18n().subscribe(move |_| *count.lock().unwrap() += 1);

    // The zone follows the platform, even where the app picked the language: formatted
    // instants change with it.
    set_language(&i18n, "hr").unwrap();
    let before = *heard.lock().unwrap();
    modify(&platform, |snapshot| {
        snapshot.locale.time_zone = "Europe/Zagreb".into()
    });
    assert_eq!(i18n.time_zone(), "Europe/Zagreb");
    assert_eq!(*heard.lock().unwrap(), before + 1);
    let local = formatter.local_time("2026-11-02T09:30:00.000Z").unwrap();
    assert_eq!((local.day, local.hour, local.minute), (2, 10, 30));
    assert_eq!(
        formatter.local_time_from_millis(1_793_611_800_000).unwrap(),
        local
    );
    assert_eq!(
        formatter
            .format_date_time(
                &local,
                dxn_intl::DateTimeOptions::time(dxn_intl::Length::Short)
            )
            .unwrap(),
        "10:30"
    );

    // The app may set one itself; an unknown zone and a wall time without offset are refused.
    block_on(i18n.set_time_zone("Asia/Tokyo".into())).unwrap();
    assert_eq!(
        formatter.local_time("2026-11-02T09:30:00Z").unwrap().hour,
        18
    );
    assert!(block_on(i18n.set_time_zone("Europe/Atlantis".into())).is_err());
    assert!(formatter.local_time("2026-11-02T09:30:00").is_err());
}

#[test]
fn options_bind_from_configuration_and_resources_add_at_run_time() {
    let mut builder = RayXAppBuilder::new().add_i18n(|options| {
        options.auto_load_assets(false);
    });
    builder.configuration.add_value(json!({
        "RayX": { "Services": { "I18N": { "default_language": "hr", "missing_message": "empty" } } }
    }));
    let services = add_host_fakes(builder).build().unwrap();
    let (i18n, formatter) = i18n(&services);
    assert_eq!(i18n.language(), "hr");
    assert_eq!(formatter.t("missing"), "");
    let add = |namespace: Option<&str>, json: &str| {
        block_on(i18n.add_resource("hr".into(), namespace.map(Into::into), json.into()))
    };
    add(Some("shop"), r#"{ "buy": "Kupi" }"#).unwrap();
    assert_eq!(formatter.t("shop.buy"), "Kupi");
    assert!(add(None, r#"{ "ok": "U redu", "bad": "{$x" }"#).is_err());
    assert_eq!(formatter.t("ok"), "U redu");
    assert!(matches!(
        set_language(&i18n, "not a tag"),
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
    let services = add_host_fakes(builder).build().unwrap();
    assert!(services.validate_options_on_start().is_err());
}

#[test]
fn building_without_an_asset_or_platform_service_fails_with_their_hints() {
    let builder = RayXAppBuilder::new().add_i18n(|_| {});
    let Err(AppBuildError::Services(ServiceValidationError { problems })) = builder.build() else {
        panic!("built without an asset or a platform service");
    };
    for (service, hint) in [
        ("AssetService", "add_assets(..)"),
        ("PlatformInfoService", "add_platform()"),
    ] {
        assert!(
            problems.iter().any(|problem| matches!(
                problem,
                ServiceValidationProblem::Missing { needed, by, hint: told }
                    if needed.contains(service)
                        && by.as_deref().is_some_and(|by| by.contains("I18nService"))
                        && told.contains(hint)
            )),
            "no {service} hint in {problems:?}"
        );
    }
}
