---
agentx:
  kind: rules
  scope: knowledge/runtime
  status: current
  created: 2026-11-04T19:30:18+01:00
  updated: 2026-11-18T03:10:00+01:00
  topics:
    - rayx_services
    - services
  supersedes: []
  superseded_by: []
---
# runtime: rules

- No NSH in the core: a service's contract and data types go in `rayx_app::services::<name>`;
  everything NSH-bound goes in its service crate (or, when every service crate builds on it, the
  host `rayx_app_nsh`).
- A service is a contract in the core plus a crate `crates/rayx_services_<name>`. The crate
  depends on `rayx_app`, `rayx_app_nsh`, `rayx_app_macros` and `nsh-core` as it needs, and never
  on another service crate (`crates/rayx/tests/crate_layout.rs`).
- A service that needs another resolves it as `dyn Trait` and declares it with `require_for`
  and a hint; it never registers it.
- A new service crate gets its umbrella feature (named after the service, implying no other
  feature), its `register` call in `rayx::__private::register`, a doc in `docs/services/`, a line
  in `docs/services.md` and its own test groups.
- Keep every `builder.add<Name>(..)` name and signature in TS when moving a service; a builder
  method exists only when its crate's set is registered.
