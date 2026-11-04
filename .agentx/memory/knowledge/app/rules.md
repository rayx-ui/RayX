---
agentx:
  kind: rules
  scope: knowledge/app
  status: current
  created: 2026-11-04T19:30:18+01:00
  updated: 2026-11-04T19:30:18+01:00
  topics:
    - rayx_app
    - nsh
    - tsx
    - rayx
  supersedes: []
  superseded_by: []
---
# app: rules


- Read NSH from `.references/nsh-main/`; never edit it, build against it or link it.
- What RayX needs from NSH goes into the fork first (see [dependencies](../dependencies/rules.md)).
