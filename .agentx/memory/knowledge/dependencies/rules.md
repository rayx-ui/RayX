---
agentx:
  kind: rules
  scope: knowledge/dependencies
  status: current
  created: 2026-11-04T19:30:18+01:00
  updated: 2026-11-04T19:30:18+01:00
  topics:
    - git-tags
    - nsh-fork
    - dxn
  supersedes: []
  superseded_by: []
---
# dependencies: rules


- Git tags only: never path, branch or commit dependencies.
- Move the `[patch.crates-io]` NSH tag and the CLI's default pin together.
- Never push to upstream `longbridge/nsh`, never a `v*` tag there.
- GPUI, gpux and RayX.Old's crates never enter the dependency tree.
