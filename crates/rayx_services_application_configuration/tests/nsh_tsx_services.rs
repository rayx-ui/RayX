//! The application configuration for TSX through NSH's engine, headless:
//! `builder.addApplicationConfiguration()` (the set's class extension) registers RayX's service,
//! which the `ApplicationConfigurationService` class reaches (the settings, the layout direction,
//! reduced motion, replacing the settings with a promise, hearing changes). The class, the
//! contract a TS implementation follows (`ApplicationConfigurationServiceContract`) and
//! `addApplicationConfiguration` with its doc comment are declared.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use nsh_core::element::Registry;
use rayx_app_nsh::testing;

fn set() -> Registry {
    testing::set(rayx_services_application_configuration::register)
}

const TSX: &str = r#"import { mount, signal, Text } from "nsh";
import { ApplicationConfigurationService, RayXAppBuilder } from "test/app";

const shown = signal("waiting");
setTimeout(async () => {
  const lines = [];
  try {
    const services = await new RayXAppBuilder().addHostFakes()
      .addApplicationConfiguration((options) => options.name("Notes"))
      .start();
    const configuration = services.getRequiredService(ApplicationConfigurationService);
    lines.push(String(configuration === services.getRequiredService(ApplicationConfigurationService)));
    lines.push(`${configuration.current().name} ${configuration.layoutDirection()} ${configuration.reducedMotion()}`);

    const heard = [];
    const stop = configuration.onChange((settings) => heard.push(`${settings.layout.direction} ${settings.motion.policy}`));
    const settings = configuration.current();
    settings.layout.direction = "rtl";
    settings.motion.policy = "Reduced";
    lines.push(String(await configuration.replaceOptions(settings)));
    lines.push(String(await configuration.replaceOptions(settings)));
    lines.push(`${configuration.layoutDirection()} ${configuration.reducedMotion()}`);
    try {
      await configuration.replaceOptions({ ...settings, name: "Renamed" });
      lines.push("renamed");
    } catch (error) {
      lines.push(error.message.includes("startup only") ? "refused" : `rejected ${error.message}`);
    }
    await new Promise((resolve) => setTimeout(resolve, 20));
    stop();
    lines.push(heard.join(","));
  } catch (error) {
    lines.push(`threw ${error}`);
  }
  shown.set(`done ${lines.join("|")}`);
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn the_configuration_class_reaches_the_service() {
    assert_eq!(
        testing::run(set(), TSX),
        "done true|Notes ltr false|true|false|rtl true|refused|rtl Reduced"
    );
}

#[test]
fn the_class_the_contract_and_add_application_configuration_are_declared() {
    let ts = testing::declarations(&set());
    let add = "addApplicationConfiguration(configure?: ((options: ApplicationConfigurationOptions) => void) | null): RayXAppBuilder;";
    for declared in [
        "export declare class ApplicationConfigurationService {",
        "  replaceOptions(settings: ApplicationConfiguration): Promise<boolean>;",
        "export interface ApplicationConfigurationServiceContract {",
        "export type ApplicationConfigurationSnapshot = ",
        "export type ApplicationConfiguration = ",
        add,
    ] {
        assert!(ts.contains(declared), "no `{declared}` in\n{ts}");
    }
    let before = ts[..ts.find(add).unwrap()].trim_end();
    assert!(
        before.ends_with("*/"),
        "no doc comment on addApplicationConfiguration in\n{ts}"
    );
    let doc = &before[before.rfind("/**").unwrap()..];
    assert!(doc.contains("configuration service"), "{doc}");
    assert_eq!(set().undocumented(), Vec::<String>::new());
}

#[test]
fn without_the_set_there_is_no_add_application_configuration() {
    let ts = testing::declarations(&testing::set(|_| {}));
    assert!(!ts.contains("addApplicationConfiguration("), "{ts}");
    assert!(
        !ts.contains("class ApplicationConfigurationService"),
        "{ts}"
    );
    assert!(
        !ts.contains("class ApplicationConfigurationOptions"),
        "{ts}"
    );
}
