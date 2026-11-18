//! Options written in TS, through NSH's engine, headless: the defaults, a bound configuration
//! section, configure callbacks in order (from `addOptions` and `configureOptions`), a failed
//! validation throwing when resolved, `validateOnStart` failing `start()` with its message, and
//! the declarations typing them.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod headless;

const TSX: &str = r#"import { mount, signal, Text } from "nsh";
import { OptionsToken, RayXAppBuilder } from "test/app";

const Feed = new OptionsToken("Feed", { title: "News", pageSize: 10, tags: ["all"], retry: { limit: 1, delay: 5 } });
const Plain = new OptionsToken("Plain", { on: true });
const Checked = new OptionsToken("Checked", { pageSize: 0 });
const Unbound = new OptionsToken("Unbound", { name: "none" });
const Mutated = new OptionsToken("Mutated", { on: false });

const shown = signal("waiting");
setTimeout(async () => {
  const lines = [];
  const thrown = (f) => { try { f(); return "no throw"; } catch (e) { return e.message; } };
  try {
    const services = await new RayXAppBuilder().addHostFakes()
      .configureAppConfiguration((c) => {
        c.addValue({ Feed: { pageSize: 25, retry: { limit: 3 } } });
      })
      .addOptions(Feed, (options) => options
        .bind("Feed")
        .configure((value) => { value.order = [value.pageSize]; return value; })
        .validate((value) => value.pageSize > 0, "pageSize must be positive"))
      .configureOptions(Feed, (value) => ({ ...value, order: [...value.order, "second"] }))
      .addOptions(Plain)
      .addOptions(Checked, (options) => options
        .validate((value) => value.pageSize > 0, "pageSize must be positive")
        .validate((value) => value.pageSize < 100, "pageSize is at most 99")
        .validate(() => false, "always"))
      .addOptions(Mutated, (options) => options.configure((value) => { value.on = true; }))
      .start();

    // The defaults, then the bound section (key by key), then the callbacks in order.
    lines.push(JSON.stringify(services.getOptions(Feed).value));
    lines.push(JSON.stringify(services.getOptions(Plain).value));
    // Nothing registered: the defaults.
    lines.push(JSON.stringify(services.getOptions(Unbound).value));
    // A scope resolves the same options.
    lines.push(String(services.createScope().getOptions(Feed).value.pageSize));
    // A failed validation throws when resolved.
    lines.push(thrown(() => services.getOptions(Checked)));
    // A callback gives the configured value.
    lines.push(thrown(() => services.getOptions(Mutated)));
  } catch (error) {
    lines.push(`threw ${error}`);
  }
  shown.set(`done ${lines.join("|")}`);
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn ts_options_are_the_defaults_the_bound_section_then_the_callbacks() {
    assert_eq!(
        headless::run(headless::set(|_| {}), TSX),
        r#"done {"title":"News","pageSize":25,"tags":["all"],"retry":{"limit":3,"delay":5},"order":[25,"second"]}|{"on":true}|{"name":"none"}|25|options `Checked` failed validation: pageSize must be positive; always|options `Mutated`: configure gave no value: it returns the configured value (the one it got, changed, or a new one)"#
    );
}

const ON_START: &str = r#"import { mount, signal, Text } from "nsh";
import { OptionsToken, RayXAppBuilder } from "test/app";

const Feed = new OptionsToken("Feed", { pageSize: 0 });
let configured = 0;
const shown = signal("waiting");
new RayXAppBuilder().addHostFakes()
  .addOptions(Feed, (options) => options
    .configure((value) => { configured++; return value; })
    .validate((value) => value.pageSize > 0, "pageSize must be positive")
    .validateOnStart())
  .start()
  .then(
    () => shown.set("done started"),
    (error) => shown.set(`done ${configured} ${error.message}`),
  );
mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn validate_on_start_fails_start_with_the_message() {
    assert_eq!(
        headless::run(headless::set(|_| {}), ON_START),
        "done 1 the app did not start: one or more startup services failed: options `Feed`: \
         failed validation: pageSize must be positive"
    );
    let valid = ON_START.replace("{ pageSize: 0 }", "{ pageSize: 1 }");
    assert_eq!(headless::run(headless::set(|_| {}), &valid), "done started");
}

#[test]
fn the_declarations_type_the_options() {
    let set = headless::set(|_| {});
    let ts = headless::declarations(&set);
    for declared in [
        "export declare class OptionsToken<T> {",
        "  constructor(name: string, defaults: T);",
        "  get name(): string;",
        "export declare class OptionsBuilder<T> {",
        "  bind(path: string): this;",
        "  configure(configure: (value: T) => T): this;",
        "  validate(validate: (value: T) => boolean, message: string): this;",
        "  validateOnStart(): this;",
        "  addOptions<T>(token: OptionsToken<T>, configure?: (options: OptionsBuilder<T>) => void): this;",
        "  configureOptions<T>(token: OptionsToken<T>, configure: (value: T) => T): this;",
        "  getOptions<T>(token: OptionsToken<T>): { value: T };",
    ] {
        assert!(ts.contains(declared), "no `{declared}` in\n{ts}");
    }
    assert_eq!(set.undocumented(), Vec::<String>::new());
}
