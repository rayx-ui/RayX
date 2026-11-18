//! TS options run every configure step before any validation, as Rust's options pipeline does:
//! a `configureOptions` added after `addOptions`, or a `.configure` after `.validate` in one
//! builder, is validated, both when the options are resolved and when `validateOnStart` checks
//! them in the startup run. Through NSH's engine, headless.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod headless;

const RESOLVED: &str = r#"import { mount, signal, Text } from "nsh";
import { OptionsToken, RayXAppBuilder } from "test/app";

const Later = new OptionsToken("Later", { pageSize: 10 });
const After = new OptionsToken("After", { pageSize: 10 });
const Fixed = new OptionsToken("Fixed", { pageSize: 0 });

const shown = signal("waiting");
setTimeout(async () => {
  const lines = [];
  const options = (f) => { try { return JSON.stringify(f().value); } catch (e) { return e.message; } };
  try {
    const services = await new RayXAppBuilder().addHostFakes()
      // A later `configureOptions` makes the value invalid.
      .addOptions(Later, (o) => o.validate((v) => v.pageSize > 0, "pageSize must be positive"))
      .configureOptions(Later, (v) => ({ ...v, pageSize: 0 }))
      // A `.configure` after `.validate` in one builder makes it invalid.
      .addOptions(After, (o) => o
        .validate((v) => v.pageSize > 0, "pageSize must be positive")
        .configure((v) => ({ ...v, pageSize: -1 })))
      // A later `configureOptions` makes invalid defaults valid.
      .addOptions(Fixed, (o) => o.validate((v) => v.pageSize > 0, "pageSize must be positive"))
      .configureOptions(Fixed, (v) => ({ ...v, pageSize: 5 }))
      .start();
    lines.push(options(() => services.getOptions(Later)));
    lines.push(options(() => services.getOptions(After)));
    lines.push(options(() => services.getOptions(Fixed)));
  } catch (error) {
    lines.push(`threw ${error}`);
  }
  shown.set(`done ${lines.join("|")}`);
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn resolved_options_validate_the_value_after_every_configure_step() {
    assert_eq!(
        headless::run(headless::set(|_| {}), RESOLVED),
        "done options `Later` failed validation: pageSize must be positive\
         |options `After` failed validation: pageSize must be positive\
         |{\"pageSize\":5}"
    );
}

/// An app whose builder chain is `chain`, showing whether it started.
fn on_start(chain: &str) -> String {
    const APP: &str = r#"import { mount, signal, Text } from "nsh";
import { OptionsToken, RayXAppBuilder } from "test/app";

const Feed = new OptionsToken("Feed", { pageSize: 10 });
const shown = signal("waiting");
new RayXAppBuilder().addHostFakes()
  CHAIN
  .start()
  .then(
    () => shown.set("done started"),
    (error) => shown.set(`done ${error.message}`),
  );
mount(() => <Text>{shown()}</Text>);
"#;
    headless::run(headless::set(|_| {}), &APP.replace("CHAIN", chain))
}

const FAILED_ON_START: &str = "done the app did not start: one or more startup services failed: \
                               options `Feed`: failed validation: pageSize must be positive";

#[test]
fn validate_on_start_validates_a_later_configure_options() {
    assert_eq!(
        on_start(
            r#".addOptions(Feed, (o) => o.validate((v) => v.pageSize > 0, "pageSize must be positive").validateOnStart())
  .configureOptions(Feed, (v) => ({ ...v, pageSize: 0 }))"#
        ),
        FAILED_ON_START
    );
}

#[test]
fn validate_on_start_validates_a_configure_after_validate() {
    assert_eq!(
        on_start(
            r#".addOptions(Feed, (o) => o
    .validate((v) => v.pageSize > 0, "pageSize must be positive")
    .configure((v) => ({ ...v, pageSize: 0 }))
    .validateOnStart())"#
        ),
        FAILED_ON_START
    );
}

#[test]
fn validate_on_start_passes_when_a_later_configure_makes_the_value_valid() {
    assert_eq!(
        on_start(
            r#".addOptions(Feed, (o) => o.validate((v) => v.pageSize > 20, "pageSize must be above 20").validateOnStart())
  .configureOptions(Feed, (v) => ({ ...v, pageSize: 25 }))"#
        ),
        "done started"
    );
}
