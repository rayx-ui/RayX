//! `dxn_i18n::t!` and `args!` with the message formatter: it is a `Translate`, as a `Translator`
//! is, and formats in the language its i18n service has now.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::Arc;

use futures::executor::block_on;
use rayx_app::RayXAppBuilder;
use rayx_app::services::i18n::{I18nService, MessageFormatter};
use rayx_app::testing::add_host_fakes;
use rayx_services_i18n::{I18nAppBuilderExt as _, Translate, Value, args};

fn services() -> (Arc<dyn I18nService>, Arc<dyn MessageFormatter>) {
    let builder = RayXAppBuilder::new().add_i18n(|options| {
        options.auto_load_assets(false).follow_platform(false);
    });
    let services = add_host_fakes(builder).build().unwrap();
    let i18n = services.required_service::<dyn I18nService>().unwrap();
    block_on(
        i18n.add_resource(
            "en".into(),
            None,
            r#"{
          "hello": "Hello, {$name}!",
          "items": ".input {$count :number}\n.match $count\none {{One item}}\n* {{{$count} items}}"
        }"#
            .into(),
        ),
    )
    .unwrap();
    block_on(i18n.add_resource("hr".into(), None, r#"{ "hello": "Bok, {$name}!" }"#.into()))
        .unwrap();
    let formatter = services.required_service::<dyn MessageFormatter>().unwrap();
    (i18n, formatter)
}

#[test]
fn t_formats_with_the_formatter_a_reference_or_an_arc() {
    let (_, formatter) = services();
    let name = "Ana";
    assert_eq!(dxn_i18n::t!(formatter, "hello", name), "Hello, Ana!");
    assert_eq!(
        dxn_i18n::t!(&*formatter, "items", count = 1200),
        "1,200 items"
    );
    assert_eq!(dxn_i18n::t!(&formatter, "items", count = 1), "One item");
    assert_eq!(dxn_i18n::t!(formatter, "missing"), "missing");
}

#[test]
fn translate_is_format_in_the_language_the_service_has_now() {
    let (i18n, formatter) = services();
    let args = args!(name = "Ana", count = 3);
    assert_eq!(args.get("count"), Some(&Value::from(3)));
    assert_eq!(
        formatter.translate("hello", &args),
        formatter.format("hello", &args)
    );

    block_on(i18n.set_language("hr".into())).unwrap();
    assert_eq!(
        Translate::translate(&formatter, "hello", &args),
        "Bok, Ana!"
    );
}
