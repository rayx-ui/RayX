---
agentx:
  kind: state
  scope: knowledge/core/configuration
  status: current
  created: 2026-11-04T19:30:18+01:00
  updated: 2026-11-18T03:10:00+01:00
  topics:
    - configuration
    - options
  supersedes: []
  superseded_by: []
---
# core/configuration: state

## Purpose And Usage

Context for changes to configuration sources and options binding.

## Current Technical Shape

`crates/rayx_app/src/configuration` and `src/options` (NSH-free). On NSH, `rayx_app_nsh` reads
the bundled `appsettings.json` (`src/configuration/settings.rs`, `AppSettingsSource`,
`add_app_settings`) and gives TS its options classes (`src/configuration/options_ts.rs`,
`configuration_builder_ts.rs`). A service's options belong to its implementation's crate
(`rayx_services_<name>`), not the core; the application configuration service is a contract in
`rayx_app::services::application_configuration` with RayX's implementation in
`crates/rayx_services_application_configuration`.

## Backlinks

Code and docs: [docs/configuration.md](../../../../../docs/configuration.md); tests
`di_configuration`, `di_options` in `crates/rayx_app/tests/`, `nsh_tsx_options`,
`nsh_tsx_options_order` in `crates/rayx_app_nsh/tests/`.

Memory: [core](../state.md), [app](../../app/state.md) (the host),
[runtime](../../runtime/state.md) (application configuration).
