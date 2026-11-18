---
agentx:
  kind: decision
  scope: decisions/app/tsx
  status: current
  created: 2026-11-18T03:10:00+01:00
  updated: 2026-11-18T12:30:00+01:00
  topics:
    - tsx
    - app-builder
    - contracts
    - threads
    - class-extensions
  supersedes:
    - .agentx/memory/decisions/app/tsx/2026-11-15-app-tsx-builder-parity.md
  superseded_by: []
  promoted_from: service-crates
---
# Decision: one app builder in Rust and TS, extended by crates, with contracts callable from any thread

**Date:** 2026-11-17, decided with the owner. **Status:** current; promoted from the epic `service-crates`
(`.agentx/tasks/service-crates/design.json`: `d-any-thread`, with `d-contract-kinds`,
`d-published-values`, `d-contract-tokens`, `d-service-crates`, `d-umbrella`), which implemented it
and closed on 2026-11-18. It supersedes the [builder parity decision](2026-11-15-app-tsx-builder-parity.md).

## Context

The [TS builder parity decision](2026-11-15-app-tsx-builder-parity.md) gave TS and Rust one
builder and one container, but Rust could reach a TS service only from the engine thread
(`ContractError::WrongThread` elsewhere), contracts had only async and posted members, and every
TS builder method lived on the host's `#[class] RayXAppBuilder`, since the host cannot depend on
the crates that build on it. Splitting the services into crates (the crate structure decision)
needs each crate to add its own builder method, and every service to be a contract an app can
implement in Rust or TS.

## Decision

Everything the parity decision settled stays, except where this says otherwise: the same builder
on both sides (`RayXAppBuilder`, camelCase twins, `await builder.start()`), Rust's `.start()`
without `set_app`, resolution under Rust's names (`getService`, `getRequiredService`,
`createScope`, `dispose`, a service's class as its own token, typed `ServiceKey<T>`), one container
with one startup graph (TS `AsyncInitializable`/`StartupInitializable` mirroring Rust's), and a
TSX app's minimal Rust crate with `#[rayx::native_module("spec")]`.

- **Contracts are callable from any thread** (`d-any-thread`). A TS implementation's adapter
  starts a command as an engine task on the engine thread, else queues it on the surface's
  dispatch queue that an engine task drains, and awaits a oneshot, so the future is `Send`.
  Once the engine is gone every queued or later call ends in `ContractError::EngineGone`;
  `WrongThread` is gone. A Rust implementation is called directly. The host's `EngineRunner`
  (one per surface) gives Rust implementations the same queue for NSH's engine-thread APIs. TS
  services are still made and resolved on the engine thread; only calls cross threads.
- **Three member kinds** (`d-contract-kinds`, `d-published-values`): commands (`async fn` or
  posted `fn`), published values (`#[state] -> &StateCell<T>`, `#[events] -> &EventStream<E>`,
  the core's types) and constants (`#[constant]`). A TS implementation publishes through
  `ContractState`/`ContractEvents`; the adapter links them, and reads constants, once when it is
  made, and never calls JS for a read afterwards, so render paths read synchronously.
- **A service is implementable in TS.** A contract may take a RayX TS class as its token
  (`d-contract-tokens`): every TS registration under that class (`addSingletonAs(KeyValueStore,
  () => new MyStore())`) implements the contract, typed by its interface
  (`#[contract(ts = "KeyValueStoreContract")]`), and `getRequiredService(KeyValueStore)` wraps
  whichever `dyn Trait` is registered, so TS holds one class per service. A contract without a
  class keeps its generated `<trait>Token()` function.
- **Builder methods come from crates** (`d-service-crates`, `d-umbrella`). Each service crate adds
  its TS method as an NSH class extension of the host's `RayXAppBuilder`
  (`#[class_extension(RayXAppBuilder)]`, `nshx-0.5.0-6`), keeping the method's name, parameters
  and types; the host's class keeps only the core's methods. `#[app_extension]` makes an app
  crate's Rust extension a builder method the same way (`builder.addWeather()`). A method exists,
  in JS and in `.nsh/types`, only when its crate's set is registered: `#[rayx::native_module]`
  registers the sets of the umbrella's enabled features.

## Alternatives considered

- **Contracts on the engine thread only** (the parity decision): background Rust code could not
  use a TS implementation, so a service could not be replaced by TS everywhere it is used.
- **Exported functions applied with `builder.apply(addI18n, …)`**, or **a builder owned by the
  umbrella with `#[cfg]`'d methods**: set aside by the owner; app crates could not extend the
  builder the way Rust's extension traits do.
- **An async `t()` in the i18n contract**: one empty frame at startup and after each language
  change; the formatter stays Rust and synchronous instead (`d-i18n-split`).

## Trade-offs accepted

- A call into a TS implementation from another thread costs an estimated 10 to 50 µs more when
  the engine is idle, or waits for its current turn; all TS work shares the UI thread, so the
  docs and the `rayx` skill say to batch such calls or keep heavy work in Rust.
- Off the engine thread, the first read of a scoped or transient TS contract made there waits for
  the engine to link it; a factory running off the engine thread therefore never reads a TS
  contract's state, events or constants.
- Breaking 0.x changes with `Migration: apps:` footers.

## Supersedes

[One app builder API in Rust and TypeScript](2026-11-15-app-tsx-builder-parity.md) (2026-11-15):
its engine-thread-only rule for contracts and its host-owned TS builder methods; the rest is
carried over above.
