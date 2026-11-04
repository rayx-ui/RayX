---
agentx:
  kind: rules
  scope: knowledge/releasing
  status: current
  created: 2026-11-04T19:30:18+01:00
  updated: 2026-11-04T19:30:18+01:00
  topics:
    - release-plz
    - cargo-dist
    - ci
  supersedes: []
  superseded_by: []
---
# releasing: rules


- Conventional Commits drive the version; breaking commits carry a `Migration:` footer.
- Never edit `CHANGELOG.md` in feature commits; never move or delete tags.
- Pushing and tagging need the owner's go-ahead.
