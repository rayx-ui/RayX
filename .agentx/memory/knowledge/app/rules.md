---
agentx:
  kind: rules
  scope: knowledge/app
  status: current
  created: 2026-11-04T19:30:18+01:00
  updated: 2026-11-18T03:10:00+01:00
  topics:
    - rayx_app_nsh
    - nsh
    - tsx
    - contracts
    - rayx
  supersedes: []
  superseded_by: []
---
# app: rules

- Read NSH from `.references/nsh-main/`; never edit it, build against it or link it.
- What RayX needs from NSH goes into the fork first (see [dependencies](../dependencies/rules.md)).
- `rayx_app_nsh` never depends on a service crate; its `services/` holds only what service
  crates build on, and its TS `RayXAppBuilder` only the core's methods. A service's TS builder
  method is its crate's class extension.
- Never pre-register a service in the host; what the start needs is a `require` with a hint.
- Never block the engine thread on a call that needs it. Off the engine thread, reach NSH through
  `EngineRunner`; a factory running off the engine thread resolves a TS contract but does not
  read its state, events or constants.
- `#[rayx::native_module]` stays an attribute: NSH's CLI reads the set's specifier from its line.
