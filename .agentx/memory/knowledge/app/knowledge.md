---
agentx:
  kind: knowledge
  scope: knowledge/app
  status: current
  created: 2026-11-04T19:30:18+01:00
  updated: 2026-11-15T18:45:00+01:00
  topics:
    - rayx_app
    - nsh
    - tsx
    - rayx
  supersedes: []
  superseded_by: []
---
# app: knowledge

- NSH's source is readable at `.references/nsh-main/` (git-ignored copy); its `AGENTS.md` and `docs/dev/` explain NSH's internals.
- TSX apps compose with the same builder as Rust apps, and TS services share the Rust container: [decision](../../decisions/app/tsx/2026-11-15-app-tsx-builder-parity.md) (2026-11-15, current; implemented by the epic `ts-app-builder`).

- Open follow-ups from `ts-app-builder` (2026-11-16): `rayx_app::ts` reads whether a TS factory may
  run on the spot from NSH's `#[doc(hidden)]` `JsHandle::handle().in_call()` (`ts/container.rs`), to
  be made public API in the fork; the TS wrapper objects of Rust services are per TS
  `ServiceProvider` (`===` within one provider, not across scopes), unlike TS singletons (one object
  per root).
