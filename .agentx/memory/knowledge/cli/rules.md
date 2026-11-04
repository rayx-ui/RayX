---
agentx:
  kind: rules
  scope: knowledge/cli
  status: current
  created: 2026-11-04T19:30:18+01:00
  updated: 2026-11-04T19:30:18+01:00
  topics:
    - rayx_cli
    - rayx-cli
    - nsh-pin
  supersedes: []
  superseded_by: []
---
# cli: rules


- The CLI's rules are in `AGENTS.md` ("The CLI"): its own lints, no RayX or NSH crate, no
  build-time project, one runner, never the `nsh` on `PATH`.
- A new command, flag or default is documented in the README and `docs/cli/projects.md` in the
  same change (`readme_*` tests check them).
