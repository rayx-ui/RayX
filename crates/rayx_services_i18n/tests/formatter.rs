//! The message formatter, split from the i18n service: RayX's formats messages, numbers, dates
//! and plurals as the i18n service did, and over any `dyn I18nService` (a fake here) it compiles
//! the published messages once per version, so a language change the service publishes shows
//! on the next call.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::sync::Arc;

use dxn_i18n::{Part, TranslateError};
use dxn_intl::{DateTime, DateTimeOptions, Length, NumberOptions, PluralCategory};
use futures::FutureExt as _;
use futures::executor::block_on;
use futures::future::BoxFuture;
use rayx_app::services::i18n::{
    I18nService, I18nServiceError, I18nState, MessageBundle, MessageFormatter,
};
use rayx_app::testing::add_host_fakes;
use rayx_app::{RayXAppBuilder, ServiceProvider, StateCell};
use rayx_services_i18n::{Args, I18nAppBuilderExt as _, I18nOptions, MissingMessagePolicy};

/// RayX's service and formatter with `configure`'s options and messages in `en` and `hr`.
fn rayx(
    configure: impl Fn(&mut I18nOptions) + Send + Sync + 'static,
) -> (Arc<dyn I18nService>, Arc<dyn MessageFormatter>) {
    let builder = RayXAppBuilder::new().add_i18n(move |options| {
        options.auto_load_assets(false).follow_platform(false);
        configure(options);
    });
    let services = add_host_fakes(builder).build().unwrap();
    let i18n = services.required_service::<dyn I18nService>().unwrap();
    for (language, json) in [
        (
            "en",
            r#"{
              "hello": "Hello, {$name}!",
              "cart": ".input {$n :number}\n.match $n\none {{One item}}\n* {{{$n} items}}",
              "link": "See {#b}this{/b}",
              "meeting": "Meeting at {$when :time timeStyle=short}"
            }"#,
        ),
        (
            "hr",
            r#"{ "cart": ".input {$n :number}\n.match $n\none {{{$n} artikl}}\nfew {{{$n} artikla}}\n* {{{$n} artikala}}" }"#,
        ),
    ] {
        block_on(i18n.add_resource(language.into(), None, json.into())).unwrap();
    }
    block_on(i18n.set_time_zone("Europe/Zagreb".into())).unwrap();
    let formatter = services.required_service::<dyn MessageFormatter>().unwrap();
    (i18n, formatter)
}

#[test]
fn rayx_s_formatter_formats_what_the_i18n_service_formatted() {
    let (i18n, formatter) = rayx(|_| {});
    assert_eq!(formatter.locale().to_string(), "en");
    assert_eq!(
        formatter.format("hello", &Args::new().with("name", "Ana")),
        "Hello, Ana!"
    );
    assert_eq!(formatter.t("hello"), "Hello, {$name}!");
    assert_eq!(formatter.t("missing"), "missing");
    assert!(formatter.contains("cart") && !formatter.contains("missing"));
    assert!(matches!(
        formatter.try_format("hello", &Args::new()),
        Err(TranslateError::Format { .. })
    ));
    let parts = formatter
        .format_to_parts("link", &Args::new())
        .unwrap()
        .text;
    assert!(parts.iter().any(|part| matches!(part, Part::Markup { .. })));
    assert!(formatter.format_to_parts("missing", &Args::new()).is_none());

    // Values in the user's time zone and locale.
    let local = formatter.local_time("2026-11-02T19:45:00Z").unwrap();
    assert_eq!((local.hour, local.minute), (20, 45));
    assert_eq!(
        formatter.local_time_from_millis(1_793_648_700_000).unwrap(),
        local
    );
    assert_eq!(
        formatter.format(
            "meeting",
            &Args::new().with("when", dxn_i18n::Value::DateTime(local))
        ),
        "Meeting at 8:45\u{202f}PM"
    );
    assert_eq!(
        formatter
            .format_date_time(&local, DateTimeOptions::time(Length::Short))
            .unwrap(),
        "8:45\u{202f}PM"
    );
    assert_eq!(
        formatter
            .format_number(1234.5.try_into().unwrap(), NumberOptions::currency("EUR"))
            .unwrap(),
        "€1,234.50"
    );
    assert!(formatter.local_time("2026-11-02T19:45:00").is_err());

    // The language the service picks is the formatter's.
    block_on(i18n.set_language("hr".into())).unwrap();
    assert_eq!(
        formatter.format("cart", &Args::new().with("n", 3)),
        "3 artikla"
    );
    assert_eq!(formatter.plural_category(21.into()), PluralCategory::One);
    assert_eq!(
        formatter
            .format_number(1234567.into(), NumberOptions::default())
            .unwrap(),
        "1.234.567"
    );
    let when = DateTime::new(2026, 10, 27, 15, 45, 0).unwrap();
    assert!(
        formatter
            .format_date_time(&when, DateTimeOptions::time(Length::Short))
            .unwrap()
            .starts_with("15:45")
    );
}

#[test]
fn the_formatter_takes_its_policies_from_the_i18n_options() {
    let (_, formatter) = rayx(|options| {
        options
            .missing_message(MissingMessagePolicy::Empty)
            .bidi_isolation(true);
    });
    assert_eq!(formatter.t("missing"), "");
    assert_eq!(
        formatter.format("hello", &Args::new().with("name", "Ana")),
        "Hello, \u{2068}Ana\u{2069}!"
    );
}

/// An i18n service of the test's own: it publishes the state the test gives it and changes
/// nothing itself.
#[derive(Default)]
struct FakeI18n {
    i18n: StateCell<I18nState>,
}

impl I18nService for FakeI18n {
    fn i18n(&self) -> &StateCell<I18nState> {
        &self.i18n
    }

    fn set_language(&self, _: String) -> BoxFuture<'_, Result<(), I18nServiceError>> {
        futures::future::ready(Ok(())).boxed()
    }

    fn follow_platform(&self) -> BoxFuture<'_, Result<(), I18nServiceError>> {
        futures::future::ready(Ok(())).boxed()
    }

    fn load_messages(&self) -> BoxFuture<'_, Result<(), I18nServiceError>> {
        futures::future::ready(Ok(())).boxed()
    }

    fn add_resource(
        &self,
        _: String,
        _: Option<String>,
        _: String,
    ) -> BoxFuture<'_, Result<(), I18nServiceError>> {
        futures::future::ready(Ok(())).boxed()
    }

    fn set_time_zone(&self, _: String) -> BoxFuture<'_, Result<(), I18nServiceError>> {
        futures::future::ready(Ok(())).boxed()
    }
}

fn bundle(language: &str, hello: &str) -> MessageBundle {
    MessageBundle {
        language: language.into(),
        messages: BTreeMap::from([("hello".to_owned(), hello.to_owned())]),
    }
}

/// The fake's state in `locale`, at `version`, with `hello` in English and Croatian.
fn state(locale: &str, version: u64, english: &str) -> I18nState {
    I18nState {
        language: locale.into(),
        locale: locale.into(),
        default_language: "en".into(),
        available_languages: vec!["en".into(), "hr".into()],
        time_zone: "UTC".into(),
        messages: vec![bundle("en", english), bundle("hr", "Bok")],
        version,
        ..I18nState::default()
    }
}

fn over_fake() -> (Arc<FakeI18n>, ServiceProvider) {
    let fake = Arc::new(FakeI18n::default());
    fake.i18n.publish(state("en", 1, "Hello"));
    let mut builder = RayXAppBuilder::new();
    builder
        .services
        .add_singleton_as::<dyn I18nService>(Arc::clone(&fake) as Arc<dyn I18nService>);
    let services = builder.add_message_formatter().build().unwrap();
    (fake, services)
}

#[test]
fn the_formatter_over_another_i18n_service_follows_a_published_change_on_the_next_call() {
    let (fake, services) = over_fake();
    let formatter = services.required_service::<dyn MessageFormatter>().unwrap();
    assert_eq!(formatter.t("hello"), "Hello");

    // A language change the service publishes, under a new version, shows on the next call.
    fake.i18n.publish(state("hr", 2, "Hello"));
    assert_eq!(formatter.t("hello"), "Bok");
    assert_eq!(formatter.locale().to_string(), "hr");

    // The messages are compiled once per version: the same version keeps them...
    fake.i18n.publish(state("en", 2, "Hi"));
    assert_eq!(formatter.t("hello"), "Bok");
    // ...and the next one drops them.
    fake.i18n.publish(state("en", 3, "Hi"));
    assert_eq!(formatter.t("hello"), "Hi");
}

#[test]
fn the_formatter_alone_requires_an_i18n_service() {
    let error = RayXAppBuilder::new()
        .add_message_formatter()
        .build()
        .err()
        .unwrap()
        .to_string();
    assert!(error.contains("MessageFormatter"), "{error}");
    assert!(error.contains("add_i18n(..)"), "{error}");
}
