---
agentx:
  kind: index
  scope: knowledge
  status: current
  created: 2026-11-04T19:30:18+01:00
  updated: 2026-11-04T19:30:18+01:00
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
| [core](core/INDEX.md) | the pure-Rust core: DI, configuration, options, the app builder | `crates/rayx_core`, `crates/rayx_app_macros` |
| [core/di](core/di/INDEX.md) | services, lifetimes, scopes, async startup, `#[derive(AppService)]` | `crates/rayx_core/src/di`, `src/services` |
| [core/configuration](core/configuration/INDEX.md) | configuration sources and .NET's options model | `crates/rayx_core/src/configuration`, `src/options` |
| [runtime](runtime/INDEX.md) | the app services and the platform traits `rayx_app` implements | `crates/rayx_runtime` |
| [app](app/INDEX.md) | the NSH layer, TSX classes, and the `rayx` umbrella | `crates/rayx_app`, `crates/rayx` |
| [cli](cli/INDEX.md) | `rayx`, the command-line tool | `crates/rayx_cli` |
| [dependencies](dependencies/INDEX.md) | git-tag-only rule, the NSH fork pin, dXn crates | `Cargo.toml` |
| [releasing](releasing/INDEX.md) | release-plz, cargo-dist, CI, the 0.x version policy | `.github/workflows`, `release-plz.toml`, `dist-workspace.toml` |
| [governance](governance/INDEX.md) | the RayX root this repository lives in, agent instructions | `AGENTS.md`, `../AGENTS.md` |
| [governance/memory](governance/memory/INDEX.md) | how this memory tree is organized | `.agentx/` |
