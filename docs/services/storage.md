# Storage

Values under keys in named buckets, behind the `KeyValueStore` trait, with JSON helpers.

## Register

```rust
builder.add_storage()
```

`add_storage()` registers an `InMemoryKeyValueStore` unless a store is registered already; `with_key_value_store(store)`
sets one. On NSH a persistent store is registered first (below), so `add_storage()` keeps it.

## Use

```rust
let store = services.required_service::<dyn KeyValueStore>()?;
let drafts = store.bucket("drafts");
drafts.set_json("note-1", &Draft { title: "Ideas".into() })?;
let draft: Option<Draft> = drafts.try_get_json("note-1")?;
drafts.remove("note-1")?;
```

- Values are bytes; `bucket(name)` adds `set_json`, `get_json`, `try_get_json`, `remove`, `keys` and `clear`.
- The store's methods (`get`, `set`, `remove`, `keys`, `clear`) are synchronous; `get_async` and `set_async` default
  to them, and buckets have `set_json_async` and `try_get_json_async`.

## On NSH

`rayx_app`'s `NshKeyValueStore` keeps values in NSH's storage (`nsh/storage`: a file in the app's data directory, the
browser's `localStorage`; strings only, 5 MB in all, shared with the app's TSX) under `rayx/<bucket>/<key>`, UTF-8 as
it is and other bytes in base64.

NSH's storage is reachable only while the engine runs Rust code, so the store mirrors its entries (loaded at startup on
the engine thread):

- reads come from the mirror, on any thread;
- a write where the storage is reachable goes through at once, and fails when the storage is full;
- a write elsewhere (a TSX class's synchronous call, another thread) waits until a task on the engine thread writes it.

## TSX

```tsx
const storage = Storage.of(app);
storage.setJson("settings", "theme", { mode: "dark" });
const theme = storage.getJson("settings", "theme");
```

`get`, `set`, `getJson`, `setJson`, `remove`, `keys` and `clear` take a bucket and a key.

## Not available yet

Secure storage: NSH has no keychain or keystore access.
