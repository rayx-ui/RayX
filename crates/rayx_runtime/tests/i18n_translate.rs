//! `dxn_i18n::t!` and `args!` with the i18n service: it is a `Translate`, as a `Translator` is,
//! and formats in the language it has now.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::Arc;

use rayx_runtime::services::i18n::{I18nOptions, I18nService, Translate, Value, args};

fn service() -> I18nService {
    let mut options = I18nOptions::default();
    options.auto_load_assets(false);
    let service = I18nService::new(options).unwrap();
    service
        .add_resource(
            "en",
            None,
            r#"{
              "hello": "Hello, {$name}!",
              "items": ".input {$count :number}\n.match $count\none {{One item}}\n* {{{$count} items}}"
            }"#,
        )
        .unwrap();
    service
        .add_resource("hr", None, r#"{ "hello": "Bok, {$name}!" }"#)
        .unwrap();
    service
}

#[test]
fn t_formats_with_the_service_a_reference_or_an_arc() {
    let i18n = service();
    let name = "Ana";
    assert_eq!(dxn_i18n::t!(i18n, "hello", name), "Hello, Ana!");
    assert_eq!(dxn_i18n::t!(&i18n, "items", count = 1200), "1,200 items");
    let shared = Arc::new(i18n.clone());
    assert_eq!(dxn_i18n::t!(shared, "items", count = 1), "One item");
    assert_eq!(dxn_i18n::t!(i18n, "missing"), "missing");
}

#[test]
fn translate_is_format_in_the_language_the_service_has_now() {
    let i18n = service();
    let args = args!(name = "Ana", count = 3);
    assert_eq!(args.get("count"), Some(&Value::from(3)));
    assert_eq!(i18n.translate("hello", &args), i18n.format("hello", &args));

    i18n.set_language("hr").unwrap();
    assert_eq!(Translate::translate(&i18n, "hello", &args), "Bok, Ana!");
}
