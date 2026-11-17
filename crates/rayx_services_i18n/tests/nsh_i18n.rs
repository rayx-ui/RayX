//! The i18n service for TSX through NSH's engine, headless: the `I18nService` class formats messages,
//! instants in the user's time zone and numbers, changes the language and tells TSX about it;
//! over a TS implementation of the contract it formats the messages that implementation
//! publishes, through RayX's formatter.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod messages;

use nsh_core::element::Registry;
use nsh_core::prelude::*;
use rayx_app_nsh::RayXAppBuilder;
use rayx_app_nsh::testing;
use rayx_services_i18n::I18nAppBuilderExt as _;

fn build_app(builder: RayXAppBuilder) -> RayXAppBuilder {
    messages::add_messages(
        builder,
        &[
            (
                "locales/en.json",
                r#"{ "hello": "Hello, {$name}!", "cart": ".input {$n :number}\n.match $n\none {{One item}}\n* {{{$n} items}}", "meeting": "Meeting at {$when :time timeStyle=short}" }"#,
            ),
            (
                "locales/hr.json",
                r#"{ "cart": ".input {$n :number}\n.match $n\none {{{$n} artikl}}\nfew {{{$n} artikla}}\n* {{{$n} artikala}}" }"#,
            ),
        ],
    )
    .add_i18n(|options| {
        options.follow_platform(false);
    })
}

/// The app's Rust side: its composition root, for the TSX builder.
#[module]
struct Probe;

#[module]
impl Probe {
    /// Adds the app's services to `builder`.
    pub(crate) fn add_app(
        builder: Instance<rayx_app_nsh::ts::RayXAppBuilder>,
    ) -> Result<Instance<rayx_app_nsh::ts::RayXAppBuilder>, String> {
        builder
            .with_mut(|builder| builder.extend(build_app))
            .map_err(|error| error.to_string())?;
        Ok(builder)
    }
}

fn set() -> Registry {
    testing::set(|set| {
        rayx_services_i18n::register(set);
        set.module::<Probe>();
    })
}

const TSX: &str = r#"import { mount, signal, Text } from "nsh";
import { I18nService, Probe, RayXAppBuilder } from "test/app";

const shown = signal("waiting");
setTimeout(async () => {
  const lines = [];
  const services = await Probe.addApp(new RayXAppBuilder()).addHostFakes().start();
  const i18n = services.getRequiredService(I18nService);
  await i18n.setTimeZone("Europe/Zagreb");
  lines.push(i18n.t("hello", { name: "Ana" }));
  lines.push(i18n.t("cart", { n: 1 }));
  lines.push(i18n.has("cart") && !i18n.has("nope") ? "has" : "lacks");
  lines.push(i18n.languages().sort().join(","));
  // Instants (a Date's `toISOString()`) are dates in the user's zone; a Date itself throws.
  const meeting = new Date(Date.UTC(2026, 10, 2, 19, 45));
  lines.push(i18n.t("meeting", { when: meeting.toISOString() }));
  try {
    i18n.t("meeting", { when: meeting });
    lines.push("no throw");
  } catch (error) {
    lines.push(String(error).includes("toISOString") ? "throws" : `threw ${error}`);
  }
  lines.push(i18n.formatDate(meeting.toISOString(), { length: "long", time: "minute", hourCycle: "h23" }));
  lines.push(i18n.formatDate(meeting.toISOString(), { date: false, time: "minute", hourCycle: "h23" }));
  lines.push(i18n.formatNumber(0.256, { style: "percent" }));
  lines.push(i18n.formatNumber(1234.5, { style: "currency", currency: "EUR" }));
  lines.push(i18n.timeZone());
  i18n.onChange((locale) => {
    lines.push(`changed ${locale} ${i18n.direction()}`);
    lines.push(i18n.t("cart", { n: 3 }));
    shown.set(`done ${lines.join("|")}`);
  });
  await i18n.setLanguage("hr");
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn nsh_i18n_gives_tsx_messages_and_language_changes() {
    assert_eq!(
        testing::run(set(), TSX),
        "done Hello, Ana!|One item|has|en,hr|Meeting at 8:45\u{202f}PM|throws|November 2, 2026 at 20:45|20:45|26%|€1,234.50|Europe/Zagreb|changed hr ltr|3 artikla"
    );
}

const TS_IMPLEMENTATION: &str = r#"import { mount, signal, Text } from "nsh";
import { ContractState, I18nService, RayXAppBuilder } from "test/app";

/// An i18n service written in TS: its messages come from wherever it likes.
class RemoteI18n {
  i18n = new ContractState({
    language: "en", locale: "en", default_language: "en", time_zone: "UTC", version: 1,
    available_languages: ["en", "hr"],
    messages: [
      { language: "en", messages: { hello: "Hello from TS, {$name}!" } },
      { language: "hr", messages: { hello: "Bok iz TS-a, {$name}!" } },
    ],
  });
  async setLanguage(language) {
    const now = this.i18n.value;
    this.i18n.publish({ ...now, language, locale: language, chosen_language: language, version: now.version + 1 });
  }
  async followPlatform() {}
  async loadMessages() {}
  async addResource() {}
  async setTimeZone() {}
}

const shown = signal("waiting");
setTimeout(async () => {
  const lines = [];
  try {
    const services = await new RayXAppBuilder().addHostFakes()
      .addSingletonAs(I18nService, () => new RemoteI18n())
      .addMessageFormatter()
      .start();
    const i18n = services.getRequiredService(I18nService);
    lines.push(i18n.t("hello", { name: "Ana" }));
    // The call resolves once the TS implementation took it.
    await i18n.setLanguage("hr");
    lines.push(`${i18n.locale()} ${i18n.t("hello", { name: "Ana" })}`);
  } catch (error) {
    lines.push(`threw ${error}`);
  }
  shown.set(`done ${lines.join("|")}`);
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn nsh_i18n_formats_what_a_ts_implementation_publishes() {
    assert_eq!(
        testing::run(set(), TS_IMPLEMENTATION),
        "done Hello from TS, Ana!|hr Bok iz TS-a, Ana!"
    );
}
