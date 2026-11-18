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
  closure; a handle TS does not take during the call (its `registerTarget` throws first) is released with the call.
  `release_target(id).await` releases it; operations submitted to it fail with `TargetReleased`.
- **Submissions:** `submit(&id, operation, origin).await` encodes the operation as an `OperationRecord` and submits it
  (`submit_record(record, origin, transaction)` in the contract); the receipt has its inverse, which history uses to
  undo it.
- **Transactions:** `begin_transaction(mode).await` opens a transaction, over the commands `begin`, `commit(id)` and
  `abort(id)`: its events are published when it commits, and an abort (or dropping it) undoes its operations, newest
  first. It is carried explicitly: `transaction.submit(&id, operation, origin).await` submits in it from any task or
  thread, so it may be held across `.await`s and closed on another thread. `transaction.enter(|| ..)` makes it the
  thread's transaction for a synchronous body only: there `submit` on the service (and the code calling it) submits in
  it and `begin_transaction(Join)` joins it (`Reject` refuses, `New` begins another); the body cannot `.await`, so it
  never spans another task's work, and the futures it makes carry the id. A submission into a transaction that is not
  open (never begun, committed or aborted) fails with `TransactionClosed`, applying nothing.
- **Events:** `subscribe(..)` (or `events()`) hears every `OperationEvent`, committed or failed; a subscriber that
  panics fails neither the others nor the operation.
- **Remote operations:** `publish_remote(&id, operation, peer, before, after, affected).await` publishes what a peer
  applied as committed.
- **Transaction ids** are unique and increasing per service: every id it gives (`begin`, a submission outside a
  transaction, a remote operation) is greater than every id it gave before, on any thread. An implementation, in Rust
  or TS, must keep this: history groups the receipts of one id into one entry, and a clear discards what transactions
  with smaller ids commit after it.
- **The instance:** `instance_id()`, a constant, tells two services apart; the transactions entered on a thread are
  kept per instance.

In TS, `services.getRequiredService(OperationService)` wraps whichever `dyn OperationService` is registered:
`registerTarget(id, target)`, `releaseTarget`, `begin`, `commit`, `abort`, `submitRecord` and `publishRemoteRecord`
return promises, `targets` and `instanceId` are read at once, and `onEvent` hears every event.

`rayx_app::testing::FakeOperations` is a fake of the contract, for testing what consumes operations.
