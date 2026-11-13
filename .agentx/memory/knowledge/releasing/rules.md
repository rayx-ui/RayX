---
agentx:
  kind: rules
  scope: knowledge/releasing
  status: current
  created: 2026-11-04T19:30:18+01:00
  updated: 2026-11-13T18:20:12+01:00
  topics:
    - release-plz
    - migration-footers
    - cargo-dist
    - ci
  supersedes: []
  superseded_by: []
---
# releasing: rules


- Conventional Commits drive the version; breaking commits carry a `Migration:` footer.
- Write `Migration:` footers for apps in general (`Migration: apps: …`). Name no consumer app, by its current or any former name, anywhere in the repository: code, docs, memory, changelog or commit messages. An old footer that names an app (92beb14) is history, not a pattern to follow.
- Never edit `CHANGELOG.md` in feature commits; never move or delete tags.
- Pushing and tagging need the owner's go-ahead.
