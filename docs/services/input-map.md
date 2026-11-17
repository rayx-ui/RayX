# Input map

Which action a key sequence or pointer gesture runs in a context, behind the `InputMapService` contract.

## Register

```rust
use rayx_services_input_map::InputMapAppBuilderExt;

builder.add_input_map(|options| {
    options.bind_key("editor", "cmd-s", "file::save");
})
```

The contract is `rayx_app::services::input_map::InputMapService`; RayX's implementation is
`rayx_services_input_map`'s, registered as the app's `dyn InputMapService`, replacing any other (`builder.addInputMap(..)`
in TS, once its set is registered with `rayx_services_input_map::register(registry)`). An app may register its own
`dyn InputMapService`, in Rust or in TS (a class implementing `InputMapServiceContract`, registered under
`InputMapService`).

`add_input_map` requires a [key-value store](storage.md) (`add_storage()` from `rayx_services_storage`, or the app's
own `dyn KeyValueStore`), where the user's bindings live, and registers none; the input map initializes after it.

## Configure

Options bind from `RayX:Services:InputMap`, then the closure changes them:

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

The service publishes the three documents and the user's revision as `InputMapState` (`input_map()`, a state cell).

## Use

```rust
let input = services.required_service::<dyn InputMapService>()?;
let action = input.resolve(&["editor", "document"], &binding); // innermost context first, then ""
let shortcuts = input.bindings_for("file::save");
```

- `resolve` and `bindings_for` are functions of the state (`InputMapState::resolve`, `InputMapState::bindings_for`),
  synchronous whatever the implementation.
- `apply_user_document(expected_revision, document).await` validates and stores the user's document, then switches to
  the next revision; `load().await` reads it again; `input_map().subscribe(..)` hears the changes.
- The document model (`InputMapDocument`, entries, keyboard and pointer bindings, modifiers) validates key sequences
  itself (`valid_key_sequence`). The service resolves bindings itself, so any UI layer can feed it events.

In TS, the `InputMapService` class has `resolve(contexts, binding)`, `bindingsFor(action)`, `current()`,
`revision()`, `applyUserDocument(expectedRevision, document)` and `load()` (promises) and `onChange(listener)`.

## Key sequences

A key sequence is keystrokes separated by spaces, each its modifiers and its key joined by `-` (`ctrl-k ctrl-c`,
`cmd-shift-p`, `ctrl--` for the minus key). Spellings of the same keys are one binding: `normalize_key_sequence` puts
the modifiers in the order `ctrl`, `alt`, `shift`, `cmd`, `fn` and the key in lowercase, `control` is `ctrl`, and
`cmd`, `super`, `win` and `platform` are the platform's key (Command on Apple platforms, the Windows or Super key
elsewhere), as GPUI reads them. So `shift-cmd-P` resolves what `cmd-shift-p` binds, a user's `unbind` of `super-shift-p`
removes it, and one document that binds both spellings in one context has a duplicate binding.
