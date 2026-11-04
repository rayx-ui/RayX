---
agentx:
  kind: rules
  scope: knowledge/runtime
  status: current
  created: 2026-11-04T19:30:18+01:00
  updated: 2026-11-04T19:30:18+01:00
  topics:
    - rayx_runtime
    - services
  supersedes: []
  superseded_by: []
---
# runtime: rules


- No NSH dependency in `rayx_runtime`; a platform need becomes a trait implemented in `rayx_app`.
- A new service gets a doc in `docs/services/`, a line in `docs/services.md` and its own test group.
