# Operations

Typed, versioned domain operations applied to registered targets: the base of [undo and redo](history.md).

## Register

```rust
builder.add_operations()
```

`add_operations()` registers `OperationService` as a startup singleton; `with_operation_service(service)` replaces it.
The `diagnostics` feature adds diagnostics.

## Use

- **Targets:** register an `OperationTargetHandler<T>` per `OperationTargetId`; it applies the operations meant for
  that target.
- **Transactions:** `DomainOperation`s apply atomically in transactions, nested with a join, new or reject policy.
- **Receipts:** every applied operation returns a receipt with its inverse operations, which history uses to undo it.
- **Events:** `OperationEvent`s tell subscribers what was applied.
