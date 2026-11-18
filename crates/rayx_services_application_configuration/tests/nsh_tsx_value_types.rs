//! The application configuration's TS class declares the settings it gives and takes as the
//! `ApplicationConfiguration` they are (the JSON of Rust's), not `unknown`: `current()`,
//! `onChange`'s listener and `replaceOptions`; `merge` takes an object of settings.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use rayx_app_nsh::testing;

/// The declaration of the class `name` in `ts`, up to its closing brace.
fn class<'a>(ts: &'a str, name: &str) -> &'a str {
    ts.split(&format!("export declare class {name} {{"))
        .nth(1)
        .and_then(|rest| rest.split("\n}\n").next())
        .unwrap_or_else(|| panic!("no class {name} in\n{ts}"))
}

#[test]
fn the_settings_are_declared_as_application_configuration() {
    let ts = testing::declarations(&testing::set(
        rayx_services_application_configuration::register,
    ));
    let declared = class(&ts, "ApplicationConfigurationService");
    for line in [
        "  current(): ApplicationConfiguration;",
        "  replaceOptions(settings: ApplicationConfiguration): Promise<boolean>;",
        "  onChange(listener: (settings: ApplicationConfiguration) => void): () => void;",
    ] {
        assert!(declared.contains(line), "no `{line}` in\n{declared}");
    }
    let declared = class(&ts, "ApplicationConfigurationOptions");
    let line = "  merge(settings: Record<string, unknown>): ApplicationConfigurationOptions;";
    assert!(declared.contains(line), "no `{line}` in\n{declared}");
    assert!(
        !ts.contains("(): unknown"),
        "a member still gives `unknown` in\n{ts}"
    );
}
