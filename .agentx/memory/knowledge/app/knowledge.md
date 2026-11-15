---
agentx:
  kind: knowledge
  scope: knowledge/app
  status: current
  created: 2026-11-04T19:30:18+01:00
  updated: 2026-11-15T18:45:00+01:00
  topics:
    - rayx_app
    - nsh
    - tsx
    - rayx
  supersedes: []
  superseded_by: []
---
# app: knowledge

- NSH's source is readable at `.references/nsh-main/` (git-ignored copy); its `AGENTS.md` and `docs/dev/` explain NSH's internals.
- TSX apps compose with the same builder as Rust apps, and TS services share the Rust container: [decision](../../decisions/app/tsx/2026-11-15-app-tsx-builder-parity.md) (2026-11-15, current; implemented by the epic `ts-app-builder`).
