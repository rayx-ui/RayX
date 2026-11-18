---
agentx:
  kind: knowledge
  scope: knowledge/app
  status: current
  created: 2026-11-04T19:30:18+01:00
  updated: 2026-11-18T03:10:00+01:00
  topics:
    - rayx_app_nsh
    - nsh
    - tsx
    - contracts
    - rayx
  supersedes: []
  superseded_by: []
---
# app: knowledge

- NSH's source is readable at `.references/nsh-main/` (git-ignored copy); its `AGENTS.md` and `docs/dev/` explain NSH's internals.
- TSX apps compose with the same builder as Rust apps, and TS services share the Rust container: [decision](../../decisions/app/tsx/2026-11-15-app-tsx-builder-parity.md) (2026-11-15, current; implemented by the epic `ts-app-builder`). Its rule that Rust reaches TS services from the engine thread only, and the host's own TS builder methods per service, are replaced by the epic `service-crates`: [successor draft](../../decisions/app/tsx/2026-11-17-app-tsx-contracts-any-thread-builder-extensions.md) (2026-11-17, current, promoted when that epic closes).

- The host's TS container asks whether a TS factory may run on the spot with NSH's public
  `Callback::can_call_now()` (`crates/rayx_app_nsh/src/di/container_ts.rs`, since `nshx-0.5.0-5`,
  fork #269), which replaced the `#[doc(hidden)]` `JsHandle::handle().in_call()` it read before.
- Open follow-up from `ts-app-builder` (2026-11-16): the TS wrapper objects of Rust services are
  per TS `ServiceProvider` (`===` within one provider, not across scopes), unlike TS singletons
  (one object per root).
