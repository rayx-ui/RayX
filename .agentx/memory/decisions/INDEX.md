---
agentx:
  kind: index
  scope: decisions
  status: current
  created: 2026-11-04T19:30:18+01:00
  updated: 2026-11-19T13:30:00+01:00
  topics:
    - decisions
  supersedes: []
  superseded_by: []
---
# RayX memory: decisions index

Repository decisions, the second tier of evidence after source. Path:
`decisions/<domain>/<subdomain>/YYYY-MM-DD-<domain>-<subdomain>-<topic>.md`. A decision drafted
from an open epic's design sits at its final path with `promoted_from: <epic-id>` and says so in
its status line; AgentX promotes it when the epic closes.

The CLI's standing decisions live in the repository docs, not here:
[docs/cli/decisions.md](../../../docs/cli/decisions.md) and the decision log D1 to D15 in
[docs/cli/architecture/rayx-nsh-proposal.md](../../../docs/cli/architecture/rayx-nsh-proposal.md).

| Decision | Domain | Status |
| --- | --- | --- |
| [Memory taxonomy](governance/memory/2026-11-04-governance-memory-taxonomy.md) | governance/memory | current (2026-11-04) |
| [One app builder API in Rust and TypeScript](app/tsx/2026-11-15-app-tsx-builder-parity.md) | app/tsx | superseded (2026-11-18) by the next row |
| [One app builder extended by crates, contracts callable from any thread](app/tsx/2026-11-17-app-tsx-contracts-any-thread-builder-extensions.md) | app/tsx | current (2026-11-17), from `service-crates` |
| [RayX's crates: core, NSH host, a crate per service, umbrella features](runtime/service-crates/2026-11-16-runtime-service-crates-crate-structure.md) | runtime/service-crates | current (2026-11-16), from `service-crates` |
| [RayX's roles are an API list in `roles.json`; the role map is design data](components/roles/2026-11-19-components-roles-role-list-and-map.md) | components/roles | current (2026-11-19), from `theme-roles` |
| [One NSH tag across RayX and RayX.Dtcg](dependencies/nsh-pin/2026-11-19-dependencies-nsh-pin-one-tag.md) | dependencies/nsh-pin | current (2026-11-19), from `theme-roles` |

Cross-repository: components read roles only, RayX.Root's
`.agentx/memory/decisions/workspace/2026-11-19-theme-roles-only.md` (from `theme-roles`,
superseding its 2026-11-16 theme documents decision).
