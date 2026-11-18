---
agentx:
  kind: index
  scope: knowledge
  status: current
  created: 2026-11-04T19:30:18+01:00
  updated: 2026-11-18T03:10:00+01:00
  topics:
    - taxonomy
  supersedes: []
  superseded_by: []
---
# RayX memory: knowledge index

Secondary context for the RayX code repository. Current source, tests and `docs/` come first;
this tree holds rationale, rules and history that the source does not show. The taxonomy and why
it was chosen: [taxonomy decision](../decisions/governance/memory/2026-11-04-governance-memory-taxonomy.md) (current, 2026-11-04).

| Domain | Covers | Code |
| --- | --- | --- |
| [core](core/INDEX.md) | the NSH-free core: DI, configuration, options, the app builder, published values, every service's contract and its fake | `crates/rayx_app`, `crates/rayx_app_macros` |
| [core/di](core/di/INDEX.md) | services, lifetimes, scopes, async startup, validation, `#[derive(AppService)]` | `crates/rayx_app/src/di`, `src/services/app` |
| [core/configuration](core/configuration/INDEX.md) | configuration sources and .NET's options model | `crates/rayx_app/src/configuration`, `src/options` |
| [runtime](runtime/INDEX.md) | RayX's service implementations, one crate per service | `crates/rayx_services_*` |
| [app](app/INDEX.md) | the NSH host (start, scopes, binding, TS side, contracts' TS adapters) and the `rayx` umbrella | `crates/rayx_app_nsh`, `crates/rayx` |
| [components](components/INDEX.md) | the component library, RayX's theme type and the gallery app | `crates/rayx_components`, `examples/gallery` |
| [cli](cli/INDEX.md) | `rayx`, the command-line tool | `crates/rayx_cli` |
| [dependencies](dependencies/INDEX.md) | git-tag-only rule, the NSH fork pin, dXn crates | `Cargo.toml` |
| [releasing](releasing/INDEX.md) | release-plz, cargo-dist, CI, the 0.x version policy | `.github/workflows`, `release-plz.toml`, `dist-workspace.toml` |
| [governance](governance/INDEX.md) | the RayX root this repository lives in, agent instructions | `AGENTS.md`, `../AGENTS.md` |
| [governance/memory](governance/memory/INDEX.md) | how this memory tree is organized | `.agentx/` |
