---
agentx:
  kind: state
  scope: knowledge/governance
  status: current
  created: 2026-11-04T19:30:18+01:00
  updated: 2026-11-04T19:30:18+01:00
  topics:
    - rayx-root
    - agents-md
  supersedes: []
  superseded_by: []
---
# governance: state

## Purpose And Usage

Orient agents to the repository's place and instructions.

## Current Technical Shape

This repository (`RayX/`) is a child project of the RayX root `..`
(`D:\Work\projects\RayX`), which is not a repository. The root's `AGENTS.md` covers what spans
the code (`RayX/`), the design side (`RayX.Design/`, its own project with its own AgentX memory),
the shared skills in `../.claude/skills/` and `_archive/`. Inside `RayX/`, its own `AGENTS.md`
governs.

## Backlinks

Code and docs: `AGENTS.md`, `CLAUDE.md` (imports `AGENTS.md`), `../AGENTS.md`,
[docs/INDEX.md](../../../../docs/INDEX.md).

Memory: [memory](memory/state.md).
