# Input map

Which action a key sequence or pointer gesture runs in a context.

## Register

```rust
builder.add_input_map(|options| {
    options.bind_key("editor", "cmd-s", "file::save");
})
```

`add_input_map` registers the [storage](storage.md) too, where the user's bindings live.

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

## Use

```rust
let input = services.required_service::<InputMapService>()?;
let action = input.resolve(&["editor", "document"], &binding); // innermost context first, then ""
let shortcuts = input.bindings_for("file::save");
```

- `apply_user_document(expected_revision, document)` validates and stores the user's document, then switches to the
  next revision; `subscribe` hears the changes.
- The document model (`InputMapDocument`, entries, keyboard and pointer bindings, modifiers) validates key sequences
  itself (`valid_key_sequence`). The service resolves bindings itself, so any UI layer can feed it events.

## Key sequences

A key sequence is keystrokes separated by spaces, each its modifiers and its key joined by `-` (`ctrl-k ctrl-c`,
`cmd-shift-p`, `ctrl--` for the minus key). Spellings of the same keys are one binding: `normalize_key_sequence` puts
the modifiers in the order `ctrl`, `alt`, `shift`, `cmd`, `fn` and the key in lowercase, `control` is `ctrl`, and
`cmd`, `super`, `win` and `platform` are the platform's key (Command on Apple platforms, the Windows or Super key
elsewhere), as GPUI reads them. So `shift-cmd-P` resolves what `cmd-shift-p` binds, a user's `unbind` of `super-shift-p`
removes it, and one document that binds both spellings in one context has a duplicate binding.
