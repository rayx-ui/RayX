# Operations

Typed, versioned domain operations applied to registered targets: the base of [undo and redo](history.md).

## Register

| | |
| --- | --- |
| Trait | `rayx_app::services::operations::OperationService` (`rayx::app::services::operations`) |
| Crate | `rayx_services_operations` (`rayx::services::operations`) |
| Feature | `operations` |
| Rust | `add_operations()` (`OperationsAppBuilderExt`) |
| TS | `addOperations()`, and the classes `OperationService` and `OperationTarget` |
| Requires | nothing |

```rust
use rayx::prelude::*; // `OperationsAppBuilderExt`, with the `operations` feature

let builder = RayXAppBuilder::new().add_operations();
```

```tsx
new RayXAppBuilder()
  .addPlatform().addSuspension().addAssets().addFonts()
  .addOperations()
  .start()
  .then((services) => { /* resolve OperationService here */ });
```

`add_operations()` registers RayX's implementation as the app's `dyn OperationService`, replacing any earlier
registration of it. It is a plain singleton, not a startup service: it has nothing to initialize and is ready once
made. In TS, `addOperations()` is there once the crate's set is registered (`#[rayx::native_module]` with the feature
on, or `rayx_services_operations::register(registry)`).

The crate's own `diagnostics` feature keeps the latest committed operations and failure codes in the published state
(`diagnostics()` on `dyn OperationService`). The umbrella has no feature for it: an app enables it by depending on
`rayx_services_operations` from the same tag as `rayx`, with `features = ["diagnostics"]`.

## The contract

`OperationService` (TS: `OperationServiceContract`) is NSH-free and implementable in Rust or TS.

Published state:

- `operations()` (TS `operations`): an `OperationState`, the registered targets (RayX's implementation publishes them
  sorted, a TS one may in any order: `targets()` sorts them and `has_target` finds any) and, with diagnostics, the
  latest committed operations and failure codes.

Events:

- `events()` (TS `events`): every `OperationEvent`, `Committed(receipt)` (outside a transaction at once, inside one
  when it commits) or `Failed(failure)`.

Constants:

- `instance_id()` (TS `instanceId`): tells two services apart; the transactions entered on a thread are kept per
  instance.

Commands:

- `register_target(id, handle)` (TS `registerTarget`): registers a target's handler, an `OperationTargetHandle`;
  refused when the id is registered (a released id may be registered again).
- `release_target(id)` (TS `releaseTarget`): releases it, whether it was registered; operations submitted to it fail
  with `TargetReleased`.
- `begin()`, `commit(id)`, `abort(id)` (TS the same): open a transaction (its id), publish its operations' events, or
  undo them newest first.
- `submit_record(record, origin, transaction)` (TS `submitRecord`): applies an `OperationRecord` through its target,
  alone or in an open transaction; the receipt, with the inverse history undoes it with.
- `publish_remote_record(record, peer, before, after, affected)` (TS `publishRemoteRecord`): publishes what a peer
  applied as committed.

Rust helpers (on `dyn OperationService`):

- `register_handler::<T, H>(id, Arc<H>)`: registers an `OperationTargetHandler<T>` of a `DomainOperation` `T`.
- `submit(&id, operation, origin)`, `resubmit(record, origin)`: encode and submit, in the transaction entered on the
  thread if there is one; `publish_remote(&id, operation, peer, before, after, affected)`.
- `begin_transaction(mode)`: an `OperationTransaction`; `current_transaction_id()`.
- `subscribe(..)`: hears every event, isolating a subscriber that panics; `targets()`, `has_target(&id)`,
  `diagnostics()`.

**Transaction ids** are unique and increasing per service: every id it gives (`begin`, a submission outside a
transaction, a remote operation) is greater than every id it gave before, on any thread. An implementation, in Rust or
TS, must keep this: history groups the receipts of one id into one entry, and a clear discards what transactions with
smaller ids commit after it.

## Use

```rust
use rayx::app::services::operations::*;

#[derive(Clone, Serialize, Deserialize)]
struct Rename(String);

impl DomainOperation for Rename {
    const KIND: &'static str = "doc.rename";
    const VERSION: u32 = 1;
}

impl OperationTargetHandler<Rename> for Document {
    fn apply_atomically(
        &self,
        operation: &Rename,
        _origin: &OperationOrigin,
    ) -> Result<OperationApplyResult<Rename>, String> {
        let (before, revision) = self.rename(&operation.0);
        Ok(OperationApplyResult::applied(Rename(before), ["title"], revision - 1, revision))
    }
}

let operations = services.required_service::<dyn OperationService>()?;
let doc = OperationTargetId::new("doc")?;
operations.register_handler::<Rename, _>(doc.clone(), Arc::new(document)).await?;
let receipt = operations.submit(&doc, Rename("Draft".into()), OperationOrigin::LocalUser).await?;
```

- **Targets:** a handler applies the operations meant for its target atomically (one at a time per target) and leaves
  it unchanged when it fails; `validate` may refuse one first. `release_target(id).await` releases it.
- **Transactions:** `begin_transaction(mode).await?` opens one, over `begin`, `commit(id)` and `abort(id)`: its events
  are published when it commits, and an abort (or dropping it) undoes its operations, newest first. It is carried
  explicitly: `transaction.submit(&id, operation, origin).await` submits in it from any task or thread, so it may be
  held across `.await`s and closed on another thread. `transaction.enter(|| ..)` makes it the thread's transaction for
  a synchronous body only: there `submit` on the service (and the code calling it) submits in it and
  `begin_transaction(Join)` joins it (`Reject` refuses with `NestedTransactionRejected`, `New` begins another); the
  body cannot `.await`, so it never spans another task's work, and the futures it makes carry the id. A submission into
  a transaction that is not open (never begun, committed or aborted) fails with `TransactionClosed`, applying nothing.

```rust
let transaction = operations.begin_transaction(OperationTransactionMode::Join).await?;
transaction.submit(&doc, Rename("Final".into()), OperationOrigin::LocalUser).await?;
transaction.commit().await?;
```

- **Events:** `operations.subscribe(|event| ..)` (or `events().subscribe(..)`) hears every event; a subscriber that
  panics fails neither the others nor the operation.
- **Remote operations:** `publish_remote(&id, operation, peer, before, after, affected).await` publishes what a peer
  applied as committed; the target must be registered.

## On NSH

RayX's implementation needs nothing from NSH: its commands answer at once, on any thread. It applies each record
through its target's handler under a per-target gate, keeps an open transaction's receipts until it commits or aborts,
and publishes the registered targets as its state.

## TSX

`OperationService`, resolved from the provider `start()` gives, wraps whichever `dyn OperationService` is registered:

- `instanceId` and `targets` (getters), read at once;
- `registerTarget(id, target)` (an `OperationTarget`), `releaseTarget(id)`, `begin()`, `commit(id)`, `abort(id)`,
  `submitRecord(record, origin, transactionId?)` and `publishRemoteRecord(record, peerId, beforeRevision,
  afterRevision, affectedIds)`: promises that reject with the service's error;
- `onEvent(listener)`: hears every event (`{ Committed: receipt }` or `{ Failed: failure }`); it returns a function
  that stops it.

`OperationTarget` is a target's handler, a Rust object: Rust code hands one to TS with `OperationTarget::new(handle)`,
and a TS implementation of the service takes the handle it is given with `OperationTarget.of(handle)` (once) and
applies records with `apply(record, origin)` (synchronous; it throws when the target refused or failed it).

```tsx
// With the provider `start()` gives, in its `.then(async (services) => { … })`:
async function submit(services: ServiceProvider, record: OperationRecord) {
  const operations = services.getRequiredService(OperationService);
  const stop = operations.onEvent((event) => {
    if ("Committed" in event) console.log(event.Committed.target_id);
  });
  const id = await operations.begin();
  await operations.submitRecord(record, "LocalUser", id);
  await operations.commit(id);
}
```

Records, origins and receipts cross as their JSON (snake_case fields; a record's `payload` is the operation's JSON as
bytes).

## Replace it

Register the app's own `dyn OperationService` instead of calling `add_operations()`. RayX's history then undoes and
redoes through it.

```rust
builder.add_singleton_as::<dyn OperationService>(
    Arc::new(MyOperations::new()) as Arc<dyn OperationService>,
)
```

In TS, a class implementing `OperationServiceContract`, registered under the class `OperationService`
([contracts](../tsx.md#contracts)):

```tsx
class MyOperations implements OperationServiceContract {
  operations = new ContractState<OperationState>({ targets: [], recent_transactions: [], recent_failure_codes: [] });
  events = new ContractEvents<OperationEvent>();
  instanceId = 77;
  targets = new Map<string, OperationTarget>();
  next = 0;
  async registerTarget(targetId: string, handle: OperationTargetHandle) {
    this.targets.set(targetId, OperationTarget.of(handle));
    this.operations.publish({ targets: [...this.targets.keys()].sort(), recent_transactions: [], recent_failure_codes: [] });
  }
  async releaseTarget(targetId: string) { /* ... */ return this.targets.delete(targetId); }
  async begin() { return ++this.next; }
  async commit(transactionId: number) { /* publish its receipts */ }
  async abort(transactionId: number) { /* undo them */ }
  async submitRecord(record: OperationRecord, origin: OperationOrigin, transactionId: number | null) {
    const target = this.targets.get(record.target_id);
    if (!target) throw new Error(`no target ${record.target_id}`);
    const result = target.apply(record, origin);
    const receipt: OperationReceipt = {
      transaction_id: transactionId ?? ++this.next, target_id: record.target_id, forward: record, origin, ...result,
    };
    this.events.publish({ Committed: receipt });
    return receipt;
  }
  async publishRemoteRecord(
    record: OperationRecord, peerId: string, beforeRevision: number, afterRevision: number, affectedIds: string[],
  ): Promise<OperationReceipt> {
    throw new Error("no peers"); // ... apply a peer's record
  }
}

new RayXAppBuilder().addSingletonAs(OperationService, () => new MyOperations());
```

It must keep the transaction-id rule above. A handle TS does not take during `registerTarget` (it throws first) is
released with the call.

`rayx_app::testing::FakeOperations` is a fake of the contract, for testing what consumes operations.
