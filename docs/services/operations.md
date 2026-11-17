# Operations

Typed, versioned domain operations applied to registered targets: the base of [undo and redo](history.md).

## Register

```rust
use rayx_services_operations::OperationsAppBuilderExt;

builder.add_operations()
```

The contract is `dyn OperationService` (`rayx_app::services::operations`): its state (the registered targets), its
events, its commands and data types, NSH-free and implementable in Rust or TS. RayX's implementation is
`rayx_services_operations`' (`builder.addOperations()` in TS, once its set is registered with
`rayx_services_operations::register(registry)`), registered as the app's `dyn OperationService`, replacing any other
(an app's own, registered after it, wins). It has nothing to initialize. Its `diagnostics` feature keeps the latest
committed operations and failure codes in the published state (`diagnostics()` on `dyn OperationService`).

## Use

- **Targets:** register an `OperationTargetHandler<T>` per `OperationTargetId` with
  `register_handler::<T, _>(id, handler).await`; it applies the operations meant for that target, atomically. The
  contract's `register_target(id, handle)` takes an `OperationTargetHandle`: the handler as a Rust object, which a TS
  implementation of the service calls through its class (`OperationTarget.of(handle).apply(record, origin)`), never a
  closure. `release_target(id).await` releases it; operations submitted to it fail with `TargetReleased`.
- **Submissions:** `submit(&id, operation, origin).await` encodes the operation as an `OperationRecord` and submits it
  (`submit_record(record, origin, transaction)` in the contract); the receipt has its inverse, which history uses to
  undo it.
- **Transactions:** `begin_transaction(mode).await` opens a transaction on the thread (join, new or reject when one
  is open), over the commands `begin`, `commit(id)` and `abort(id)`: the operations submitted on the thread meanwhile
  are in it, their events are published when it commits, and an abort (or dropping it) undoes them, newest first.
- **Events:** `subscribe(..)` (or `events()`) hears every `OperationEvent`, committed or failed; a subscriber that
  panics fails neither the others nor the operation.
- **Remote operations:** `publish_remote(&id, operation, peer, before, after, affected).await` publishes what a peer
  applied as committed.
- **The instance:** `instance_id()`, a constant, tells two services apart; a thread's open transactions are kept per
  instance.

In TS, `services.getRequiredService(OperationService)` wraps whichever `dyn OperationService` is registered:
`registerTarget(id, target)`, `releaseTarget`, `begin`, `commit`, `abort`, `submitRecord` and `publishRemoteRecord`
return promises, `targets` and `instanceId` are read at once, and `onEvent` hears every event.

`rayx_app::testing::FakeOperations` is a fake of the contract, for testing what consumes operations.
