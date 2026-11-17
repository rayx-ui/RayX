# Storage

Values under keys in named buckets, behind the `KeyValueStore` contract, with JSON helpers.

## Register

```rust
use rayx_services_storage::StorageAppBuilderExt;

builder.add_storage()            // NSH's storage: persistent
builder.add_in_memory_storage()  // in memory, for the app's life
```

The contract is `rayx_app::services::storage::KeyValueStore`; RayX's implementations are `rayx_services_storage`'s,
each registered as the app's `dyn KeyValueStore`, replacing any other (`builder.addStorage()` in TS, once its set is
registered with `rayx_services_storage::register(registry)`). An app may register its own `dyn KeyValueStore`, in
Rust or in TS (a class implementing `KeyValueStoreContract`, registered under `KeyValueStore`).

Nothing registers a store for the app: the services kept in one, the [input map](input-map.md) and
[persistent history](history.md), require it, and building without one fails naming `add_storage()`.

## Use

```rust
let store = services.required_service::<dyn KeyValueStore>()?;
let drafts = store.bucket("drafts");
drafts.set_json("note-1", &Draft { title: "Ideas".into() }).await?;
let draft: Option<Draft> = drafts.try_get_json("note-1").await?;
drafts.remove("note-1").await?;
```

- Values are bytes. The contract's commands `get`, `set`, `remove`, `keys` and `clear` are `async` and take owned
  `String` buckets and keys (`store.get("prefs".into(), "lang".into()).await`), from any thread; its constant
  `capabilities()` says whether values persist and whether they are encrypted.
- `bucket(name)` adds `set_json`, `get_json`, `try_get_json`, `remove`, `keys` and `clear`, all `async`.
- RayX's stores answer at once (from memory, or the NSH store's mirror once it loaded), so a synchronous caller may
  poll a call once (`now_or_never()`); history's `KeyValueHistoryStore` does, and fails on a store that leaves a call
  pending.

## On NSH

`add_storage()` keeps values in NSH's storage (`nsh/storage`: a file in the app's data directory, the browser's
`localStorage`; strings only, 5 MB in all, shared with the app's TSX) under `rayx/<bucket>/<key>`
(`rayx_services_storage::STORAGE_PREFIX`), UTF-8 as it is and other bytes in base64.

NSH's storage is reachable only while the engine runs Rust code, so the store mirrors its entries, loaded on the
engine thread when the app starts (a startup service keyed by `dyn KeyValueStore`, which the input map and history
initialize after; a call before that loads it through the surface's engine runner):

- reads come from the mirror, on any thread;
- a write where the storage is reachable goes through at once, and fails when the storage is full;
- a write elsewhere (another thread) waits in the mirror while the engine runner writes it on the engine thread;
- as the app quits, the store writes what still waits: it hears the app's `dyn SuspensionService` publish
  `Destroying` and `Destroyed`, so what the app saves as it hears `Destroying` reaches the device.

## TSX

`KeyValueStore`, resolved from the provider `start()` gives (`dyn KeyValueStore` in Rust):

```tsx
const storage = services.getRequiredService(KeyValueStore);
await storage.setJson("settings", "theme", { mode: "dark" });
const theme = await storage.getJson("settings", "theme");
```

`get`, `set`, `getJson`, `setJson`, `remove`, `keys` and `clear` take a bucket and a key (`keys` and `clear` a bucket
only) and return promises; `capabilities` is `{ persistent, secure }`.

## Not available yet

Secure storage: NSH has no keychain or keystore access.
