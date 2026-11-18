---
agentx:
  kind: index
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
# runtime

RayX's service implementations: one crate per service, `crates/rayx_services_<name>`, each
implementing a contract of the core (`rayx_app::services::<name>`). The crate `rayx_runtime` that
held them until the epic `service-crates` is gone; the domain keeps its name.

- [state.md](state.md): how it works now, with code and doc anchors
- [knowledge.md](knowledge.md): durable facts
- [rules.md](rules.md): apply-by-default guidance
- [hypotheses.md](hypotheses.md): unconfirmed ideas
