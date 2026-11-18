---
agentx:
  kind: knowledge
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
# runtime: knowledge

- The services were ported from RayX.Old (`../_archive/RayX.Old`), which is reference only and never a dependency.
- They lived in one crate, `rayx_runtime`, until the epic `service-crates` split them into a contract in the core and a crate each (2026-11-17). `rayx_runtime` stays in the CLI's library guard because it is also RayX.Old's crate name.
