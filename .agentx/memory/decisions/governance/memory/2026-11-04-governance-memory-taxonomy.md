---
agentx:
  kind: decision
  scope: decisions/governance/memory
  status: current
  created: 2026-11-04T19:30:18+01:00
  updated: 2026-11-04T19:30:18+01:00
  topics:
    - taxonomy
    - agentx
  supersedes: []
  superseded_by: []
---
# Decision: RayX's memory taxonomy

**Date:** 2026-11-04. **Status:** current.

## Context

RayX had no AgentX memory. The repository is a Cargo workspace of six crates sharing one version
(`AGENTS.md`, `docs/architecture.md`), with docs per feature in `docs/`, a CLI with its own
decision record (`docs/cli/decisions.md`), and release automation (release-plz, cargo-dist). It is a
child project of the RayX root (`..`), which is not a repository and whose `AGENTS.md` covers what
spans the code (`RayX/`) and the design side (`RayX.Design/`, with its own AgentX memory).

## Decision

Domains follow the crates and the repository's own cross-cutting rules:

- `core` (`rayx_core`, with `rayx_app_macros`), subdomains `di` and `configuration`: the two
  documented halves of the core (`docs/dependency-injection.md`, `docs/startup.md`,
  `docs/configuration.md`).
- `runtime` (`rayx_runtime`): the app services, one doc each under `docs/services/`. No
  subdomains yet; a service gets one when it gathers memory of its own.
- `app` (`rayx_app`, with the umbrella `rayx`): the only NSH-facing layer.
- `cli` (`rayx_cli`): its standing decisions stay in `docs/cli/decisions.md`; memory here only
  adds what those docs do not hold.
- `dependencies`: the git-tag-only rule and the NSH fork pin cut across every crate and the CLI.
- `releasing`: release-plz, cargo-dist, CI and the 0.x policy.
- `governance`, subdomain `memory`: the relationship to the RayX root and the rules of this tree.

## Excluded

- UI components and theme: RayX has none in code yet (`../AGENTS.md`).
- Design tokens and Figma: owned by `RayX.Design/` and the root's `rayx-figma` skill.
- NSH internals: `.references/nsh-main/` is a read-only copy; NSH knowledge enters only as what
  RayX depends on (`dependencies`).
- RayX.Old, RayX.CLI and other archived projects: reference only, never dependencies.

## Alternatives rejected

- One domain per service in `runtime`: twelve near-empty folders; `docs/services/*.md` already
  routes by service.
- Folding `dependencies` and `releasing` into `governance`: they are build and delivery systems
  with their own files and tests, not agent process.
- Sharing RayX.Design's memory tree: the root keeps the two projects separate, each with its own
  rules and life.
