# Input map

Which action a key sequence or pointer gesture runs in a context, behind the `InputMapService` contract.

## Register

| | |
| --- | --- |
| Trait | `rayx_app::services::input_map::InputMapService` (`rayx::app::services::input_map`) |
| Crate | `rayx_services_input_map` (`rayx::services::input_map`) |
| Feature | `input_map` |
| Rust | `add_input_map(configure)` (`InputMapAppBuilderExt`) |
| TS | `addInputMap((options) => ..)`, and the class `InputMapService` |
| Requires | `dyn KeyValueStore` (`add_storage()`) |

```rust
use rayx_services_input_map::InputMapAppBuilderExt as _;
use rayx_services_storage::StorageAppBuilderExt as _;

let builder = RayXAppBuilder::new().add_storage().add_input_map(|options| {
    options.bind_key("editor", "cmd-s", "file::save");
});
```

```tsx
new RayXAppBuilder()
  .addPlatform().addSuspension().addAssets().addFonts()
  .addStorage()
  .addInputMap((options) => options.bindKey("editor", "cmd-s", "file::save"))
  .start()
  .then((services) => { /* resolve InputMapService here */ });
```

`add_input_map(..)` registers RayX's implementation as the app's `dyn InputMapService`, replacing any earlier
registration of it. It is a startup service that reads the user's bindings from the app's `dyn KeyValueStore` when
the app starts, after the store initializes. It requires a [key-value store](storage.md) (`add_storage()`, or the
app's own `dyn KeyValueStore`) and registers none: building without one fails naming `add_storage()`. TS's
`bindKey(context, sequence, action)` returns the options, so calls chain.

The TS side is there once the crate's set is registered: `#[rayx::native_module]` with the `input_map` feature on, or
`rayx_services_input_map::register(registry)` by hand.

## Configure

`InputMapOptions` bind from `RayX:Services:InputMap` (`INPUT_MAP_OPTIONS_SECTION`), then the closure changes them;
an invalid or duplicate binding fails startup. They hold the framework's bindings (`framework`) and the
application's (`application`, which `bind_key(context, sequence, action)` adds to):

```json
{ "RayX": { "Services": { "InputMap": { "framework": [
  { "context": "", "binding": { "kind": "keyboard", "value": "cmd-q" }, "action": "app::quit" }
] } } } }
```

## Layers

1. The framework's bindings.
2. The application's bindings (`InputMapOptions`, from code and configuration).
3. The user's document, kept in the app's `KeyValueStore`. It goes over the other two, and its `unbind` entries remove
   a lower layer's binding.

## The contract

`InputMapService` (`InputMapServiceContract` in TS).

Published state:

- `input_map()` (`inputMap`): a `StateCell<InputMapState>`, the three documents (`framework`, `application`, `user`)
  and the user's document's revision (`revision`).

Commands (`async`, `Result<InputMapDocument, InputMapApplyError>`; promises in TS):

- `load()` (`load`): reads the user's document again from where the implementation keeps it and makes it current.
- `apply_user_document(expected_revision, candidate)` (`applyUserDocument`): when the user's document is at
  `expected_revision`, the candidate gets the next revision, is validated and kept, and only then becomes current. A
  failure (`RevisionConflict`, an invalid binding, the store's error) leaves the current document as it was.

Rust helpers on `dyn InputMapService`:

- `resolve(contexts, binding)`: the `ResolvedInputAction` (action, arguments, context, layer) the binding runs in the
  innermost of `contexts` that binds it, then `""`; `None` when none does or the user unbinds it.
- `bindings_for(action)`: the entries that run `action` in any context, for showing shortcuts.
- `user_document()`, `revision()`, `framework_document()` and `application_document()`.

`resolve` and `bindings_for` are functions of the state (`InputMapState::resolve`, `InputMapState::bindings_for`), so
they are synchronous whatever the implementation.

## Use

```rust
use rayx::app::services::input_map::{InputBinding, InputMapDocument, InputMapEntry, InputMapService, InputMapSource};

let input = services.required_service::<dyn InputMapService>()?;
let binding = InputBinding::Keyboard("cmd-s".into());
let action = input.resolve(&["editor", "document"], &binding); // innermost context first, then ""
let shortcuts = input.bindings_for("file::save");

let document = InputMapDocument::new(
    InputMapSource::User,
    input.revision(),
    vec![InputMapEntry::keyboard("editor", "cmd-shift-s", "file::save", None)],
);
let applied = input.apply_user_document(input.revision(), document).await?;
let _subscription = input.input_map().subscribe(|state| show(&state.user));
```

The document model (`InputMapDocument`, entries, keyboard and pointer bindings, modifiers) validates key sequences
itself (`valid_key_sequence`). The service resolves bindings itself, so any UI layer can feed it events.

## On NSH

RayX's implementation keeps the user's document as JSON in the app's `dyn KeyValueStore`, in the bucket
`rayx.input-map` under the key `user-document` (`INPUT_MAP_STORAGE_BUCKET`, `INPUT_MAP_USER_DOCUMENT_KEY`): with
`add_storage()`, in NSH's storage. It reads it when the app starts and writes it on every `apply_user_document`
before it becomes current; one load or apply follows another. It holds no NSH state of its own: the components that
see keystrokes and gestures ask it what they run.

## TSX

`InputMapService`, resolved from the provider `start()` gives, wraps whichever `dyn InputMapService` is registered:

- `current()`: the state (`{ framework, application, user, revision }`), and `revision()`;
- `resolve(contexts, binding)`: the `ResolvedInputAction`, `null` when nothing runs;
- `bindingsFor(action)`: the entries that run `action`;
- `load()` and `applyUserDocument(expectedRevision, candidate)`: promises that resolve with the user's document and
  reject with the service's error;
- `onChange((state) => ..)`: every change of the state, until the function it returns is called.

```tsx
// With the provider `start()` gives, in its `.then(async (services) => { … })`:
async function rebindSave(services: ServiceProvider) {
  const inputMap = services.getRequiredService(InputMapService);
  const resolved = inputMap.resolve(["editor"], { kind: "keyboard", value: "cmd-s" });
  const user = inputMap.current().user ?? (await inputMap.load()); // the user's document
  await inputMap.applyUserDocument(inputMap.revision(), {
    ...user,
    entries: [{ context: "editor", binding: { kind: "keyboard", value: "cmd-shift-s" }, action: "file::save" }],
  });
}
```

## Key sequences

A key sequence is keystrokes separated by spaces, each its modifiers and its key joined by `-` (`ctrl-k ctrl-c`,
`cmd-shift-p`, `ctrl--` for the minus key). Spellings of the same keys are one binding: `normalize_key_sequence` puts
the modifiers in the order `ctrl`, `alt`, `shift`, `cmd`, `fn` and the key in lowercase, `control` is `ctrl`, and
`cmd`, `super`, `win` and `platform` are the platform's key (Command on Apple platforms, the Windows or Super key
elsewhere). So `shift-cmd-P` resolves what `cmd-shift-p` binds, a user's `unbind` of `super-shift-p` removes it, and
one document that binds both spellings in one context has a duplicate binding.

## Replace it

An app may register its own `dyn InputMapService` in place of `add_input_map(..)` (it then needs no key-value store);
the class `InputMapService` wraps it. To keep RayX's input map but the user's document elsewhere, register the app's
own `dyn KeyValueStore` instead ([storage](storage.md#replace-it)).

```rust
builder.add_singleton_as::<dyn InputMapService>(Arc::new(MyInputMap::new()) as Arc<dyn InputMapService>)
```

One that must load at the start registers itself with `add_startup_singleton_for` in place of `add_singleton_as`,
as `add_input_map(..)` does: the trait and a startup service registered as it
(`builder.add_startup_singleton_for::<dyn InputMapService, _>(Arc::new(MyInputMap::new()))`, `MyInputMap` implementing
`AsyncInitializable` and `StartupInitializable`; [Startup](../startup.md#implementing-a-trait)). Registered
after `add_input_map(..)`, the app's own replaces it completely: RayX's never starts.

In TS, a class implementing `InputMapServiceContract`, registered under `InputMapService`: its state a
`ContractState`, the commands `async` methods. Values cross as the JSON of the Rust types (snake_case fields).

```tsx
declare function readTheUsersBindings(): Promise<InputMapEntry[]>; // the app's own

const document = (source: InputMapDocument["source"], revision: number, entries: InputMapEntry[]): InputMapDocument =>
  ({ schema_version: 1, source, revision, entries });

class MyInputMap implements InputMapServiceContract {
  inputMap = new ContractState<InputMapState>({});
  loaded = false;
  isInitialized() { return this.loaded; }
  async initialize() {
    this.inputMap.publish({ user: document("user", 0, await readTheUsersBindings()), revision: 0 });
    this.loaded = true;
  }
  async load() { return this.inputMap.value.user ?? document("user", 0, []); }
  async applyUserDocument(expectedRevision: number, candidate: InputMapDocument) {
    if (expectedRevision !== this.inputMap.value.revision) throw new Error("revision conflict");
    const user = { ...candidate, revision: expectedRevision + 1 };
    this.inputMap.publish({ ...this.inputMap.value, user, revision: user.revision });
    return user;
  }
}

new RayXAppBuilder()
  .addPlatform().addSuspension().addAssets().addFonts()
  .addStartupSingletonAs(InputMapService, () => new MyInputMap())
  .start()
  .then((services) => { /* RayX's InputMapService class wraps MyInputMap */ });
```

For tests, `rayx_app::testing::FakeInputMap` publishes the state a test gives it (`publish`) and records the loads
(`loads()`) and the documents applied (`applied()`).
