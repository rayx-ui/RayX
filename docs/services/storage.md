# Storage

Values under keys in named buckets, behind the `KeyValueStore` contract, with JSON helpers.

## Register

| | |
| --- | --- |
| Trait | `rayx_app::services::storage::KeyValueStore` (`rayx::app::services::storage`) |
| Crate | `rayx_services_storage` (`rayx::services::storage`) |
| Feature | `storage` |
| Rust | `add_storage()`, `add_in_memory_storage()` (`StorageAppBuilderExt`) |
| TS | `addStorage()`, `addInMemoryStorage()`, and the class `KeyValueStore` |
| Requires | nothing |

```rust
use rayx_services_storage::StorageAppBuilderExt as _;

let builder = RayXAppBuilder::new().add_storage(); // NSH's storage: persistent
// or .add_in_memory_storage(): in memory, for the app's life
// TS: new RayXAppBuilder().addStorage(), or .addInMemoryStorage()
```

Each registers RayX's implementation as the app's `dyn KeyValueStore`, replacing any earlier registration of it:

- `add_storage()` registers a store on NSH's storage, a startup service that loads what the device holds when the
  app starts (see [On NSH](#on-nsh)); its values persist.
- `add_in_memory_storage()` registers an `InMemoryKeyValueStore` (not a startup service); its values live as long as
  the app; TS's `addInMemoryStorage()` registers the same.

The TS side is there once the crate's set is registered: `#[rayx::native_module]` with the `storage` feature on, or
`rayx_services_storage::register(registry)` by hand.

Nothing registers a store for the app: the services kept in one, the [input map](input-map.md) and
[persistent history](history.md), require it, and building without one fails naming `add_storage()`.

## The contract

Values are bytes. `KeyValueStore` (`KeyValueStoreContract` in TS) is called from any thread.

Constants:

- `capabilities()` (`capabilities`): a `StorageCapabilities`, whether values persist (`persistent`) and whether they
  are encrypted at rest (`secure`).

Commands (`async`, `Result<_, StorageError>`; promises in TS):

- `get(bucket, key)` (`get`): the value at `key` in `bucket`, `Option<Vec<u8>>`.
- `set(bucket, key, value)` (`set`): sets it.
- `remove(bucket, key)` (`remove`): removes it; whether there was one.
- `keys(bucket)` (`keys`): the bucket's keys, sorted.
- `clear(bucket)` (`clear`): removes every value in the bucket.

Buckets and keys are owned `String`s.

Rust helpers:

- `bucket(name)` on `dyn KeyValueStore`: a `StorageBucket` for JSON values, with `set_json`, `get_json`
  (`StorageError::MissingKey` when there is none), `try_get_json`, `remove`, `keys` and `clear`, all `async`.

## Use

```rust
use rayx::app::services::storage::KeyValueStore;

let store = services.required_service::<dyn KeyValueStore>()?;
let drafts = store.bucket("drafts");
drafts.set_json("note-1", &Draft { title: "Ideas".into() }).await?;
let draft: Option<Draft> = drafts.try_get_json("note-1").await?;
drafts.remove("note-1").await?;

let lang = store.get("prefs".into(), "lang".into()).await?; // Option<Vec<u8>>
```

The in-memory store answers at once, and so does the NSH store on its surface's engine thread where NSH's storage is
reachable (a component, an effect, a task), so a caller there may poll a call once (`now_or_never()`). Elsewhere the
NSH store answers once the engine ran the call, and another store may answer later (a TS store answers in JS's next
turn): what RayX keeps in the store awaits its calls (history's `KeyValueHistoryStore`, the input map), so it works
over any.

## On NSH

`add_storage()` keeps values in NSH's storage (`nsh/storage`: a file in the app's data directory, the browser's
`localStorage`; strings only, 5 MB in all, shared with the app's TSX) under `rayx/<bucket>/<key>`
(`rayx_services_storage::STORAGE_PREFIX`), UTF-8 as it is and other bytes in base64.

NSH's storage is reachable only where the surface's engine runs Rust code, on its engine thread, so every call of the
store reads or writes it there: at once where it is reachable, else through the surface's engine runner from any
thread (the call answers once the engine ran it, and fails once the engine is gone). A write fails when the storage is
full. A startup service keyed by `dyn KeyValueStore`, which the input map and history initialize after, has NSH load
the device's entries on the engine thread as the app starts. The store keeps no copy of them: NSH's backend keeps
every change itself, so nothing waits to be written as the app quits, and what the app saves on the engine thread as
it hears `Destroying` reaches the device.

Each provider has its own store; the entries are NSH's. NSH shares one storage's entries between every surface
(window, engine) given the same backend, each operation serialized, so the stores of the surfaces over one storage,
and the app's TSX (`nsh/storage`), read each other's writes as soon as they are made, whatever cleared or changed
them. A surface over another storage (another backend, another data directory, a copy of the first) keeps its own
entries.

## TSX

`KeyValueStore`, resolved from the provider `start()` gives, wraps whichever `dyn KeyValueStore` is registered. Its
values are text or JSON:

- `capabilities`: a property, `{ persistent, secure }`;
- `get(bucket, key)`, `set(bucket, key, value)`: the text at a key (`null` when there is none; a value that is not
  UTF-8 rejects);
- `getJson(bucket, key)`, `setJson(bucket, key, value)`: a JSON value (`null` when there is none);
- `remove(bucket, key)` (whether it was there), `keys(bucket)` (sorted) and `clear(bucket)`.

Every method returns a promise.

```tsx
// With the provider `start()` gives, in its `.then(async (services) => { … })`:
async function rememberTheme(services: ServiceProvider) {
  const storage = services.getRequiredService(KeyValueStore);
  await storage.setJson("settings", "theme", { mode: "dark" });
  const theme = await storage.getJson("settings", "theme");
}
```

## Replace it

An app may keep its values elsewhere (a keychain, a database) by registering its own `dyn KeyValueStore` in place of
`add_storage()`. RayX's [input map](input-map.md) and [persistent history](history.md) then keep their documents in
it, and the class `KeyValueStore` wraps it.

```rust
builder.add_singleton_as::<dyn KeyValueStore>(Arc::new(MyStore::new()) as Arc<dyn KeyValueStore>)
```

One that must load at the start registers itself with `add_startup_singleton_for` in place of `add_singleton_as`,
as `add_storage()` does: the trait and a startup service registered as it, which the services that use the store wait for
(`builder.add_startup_singleton_for::<dyn KeyValueStore, _>(Arc::new(KeychainStore::new()))`, `KeychainStore` implementing
`AsyncInitializable` and `StartupInitializable`; [Startup](../startup.md#implementing-a-trait)). Registered
after `add_storage()`, the app's own replaces it completely: RayX's never starts.

In TS, a class implementing `KeyValueStoreContract`, registered under `KeyValueStore`: `capabilities` a field, the
commands `async` methods. Values cross as byte arrays: `get` answers a `number[]` or `Uint8Array` (or `null`), `set`
receives a `number[]`.

```tsx
class MyStore implements KeyValueStoreContract {
  capabilities = { persistent: true, secure: false };
  values = new Map<string, number[]>();
  async get(bucket: string, key: string) { return this.values.get(`${bucket}/${key}`) ?? null; }
  async set(bucket: string, key: string, value: number[]) { this.values.set(`${bucket}/${key}`, value); }
  async remove(bucket: string, key: string) { return this.values.delete(`${bucket}/${key}`); }
  async keys(bucket: string) { // the bucket's keys, sorted
    const prefix = `${bucket}/`;
    return [...this.values.keys()].filter((k) => k.startsWith(prefix)).map((k) => k.slice(prefix.length)).sort();
  }
  async clear(bucket: string) { for (const key of await this.keys(bucket)) this.values.delete(`${bucket}/${key}`); }
}

new RayXAppBuilder()
  .addPlatform().addSuspension().addAssets().addFonts()
  .addSingletonAs(KeyValueStore, () => new MyStore())
  .start()
  .then((services) => { /* RayX's KeyValueStore class wraps MyStore */ });
```

A store that must load first is registered with `addStartupSingletonAs`, with `isInitialized()` and
`async initialize()`. For tests, `rayx_app::testing::FakeKeyValueStore` keeps values in memory, with the capabilities
it is given (`with_capabilities`).

## Not available yet

Secure storage: NSH has no keychain or keystore access.
