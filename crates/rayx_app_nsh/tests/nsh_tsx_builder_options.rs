//! Options callbacks of TS `RayXAppBuilder` methods through NSH's engine, headless: the options
//! object (`configureAppConfiguration`'s `ConfigurationBuilder`) has Rust's methods in camelCase
//! and is valid during its callback only, and a callback that throws makes its builder call
//! throw. (The service crates' options have this group in their crates.)

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod headless;

fn set() -> nsh_core::element::Registry {
    headless::set(|_| {})
}

const THROWING: &str = r#"import { mount, signal, Text } from "nsh";
import { RayXAppBuilder } from "test/app";

const shown = signal("waiting");
setTimeout(async () => {
  const lines = [];
  const thrown = (f) => { try { f(); return "no throw"; } catch (e) { return e.message; } };
  const builder = new RayXAppBuilder().addHostFakes();
  lines.push(thrown(() => builder.configureAppConfiguration(() => { throw new Error("no sources"); })));
  // An options object is valid during its callback only.
  let kept;
  builder.configureAppConfiguration((configuration) => { kept = configuration; });
  lines.push(thrown(() => kept.addValue({ Feed: { Label: "late" } })).includes("disposed") ? "disposed" : "kept");
  // The builder still works after a callback threw.
  try {
    await builder.defaultInitializationTimeout(1000).start();
    lines.push("started");
  } catch (error) {
    lines.push(`rejected ${error.message}`);
  }
  shown.set(`done ${lines.join("|")}`);
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn a_throwing_callback_makes_its_builder_call_throw_with_its_message() {
    assert_eq!(
        headless::run(set(), THROWING),
        "done no sources|disposed|started"
    );
}

#[test]
fn the_options_classes_are_declared_with_rust_s_methods() {
    let ts = headless::declarations(&set());
    for declared in [
        "export declare class ConfigurationBuilder {",
        "  addJson(json: string): ConfigurationBuilder;",
    ] {
        assert!(ts.contains(declared), "no `{declared}` in\n{ts}");
    }
}
