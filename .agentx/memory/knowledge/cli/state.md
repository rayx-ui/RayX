---
agentx:
  kind: state
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
# cli: state

## Purpose And Usage

Route work on the command-line tool.

## Current Technical Shape

Modules `cli`, `host`, `project` (pins), `setup` (doctor, WSL and Lima guests), `nsh`
(pin, store, install, run), `app`, `new`, `serve`, `fmt`, `update`. Moved here from
`rayx-ui/RayX.CLI` on 2026-11-02.

## Backlinks

Code and docs: [crates/rayx_cli/README.md](../../../../crates/rayx_cli/README.md),
[docs/cli/decisions.md](../../../../docs/cli/decisions.md),
[docs/cli/projects.md](../../../../docs/cli/projects.md),
[docs/cli/architecture/rayx-nsh-proposal.md](../../../../docs/cli/architecture/rayx-nsh-proposal.md);
tests `crates/rayx_cli/tests/<feature>.rs`, `#[ignore]`d `proof_*` groups.

Memory: [dependencies](../dependencies/state.md), [releasing](../releasing/state.md).
