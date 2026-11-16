---
agentx:
  kind: knowledge
  scope: knowledge/components
  status: current
  created: 2026-11-15T03:00:00+01:00
  updated: 2026-11-16T21:00:00+01:00
  topics:
    - rayx_components
    - theme
    - button
    - gallery
  supersedes: []
  superseded_by: []
---
# components: knowledge

- Since `nshx-0.5.0-4` (NSH #264) RayX's theme is an NSH theme document (`theme.json`) and code
  reads typed keys; the roles are the document's `"roles"` section, by token name (the
  generator's `ROLE_TOKENS`). In a Rust view, what a `theme_scope()` holds must be built where
  the scope places it (a component's view): a view built before is outside the scope (found
  2026-11-16 in `theme_modifiers_view`).
- The document's modifier contexts lie over each other (a later modifier wins), so each context
  holds only its own collections' tokens; `theme_codegen.py` stops on a token that changes with
  another collection's modifier.
- A slot TSX fills does not mount when placed straight into a nested component's children
  (`nsh_base::button().child(slot.place(..))`); placed in a view of the outer component, it does,
  and so do Rust fills (found 2026-11-15 on `nshx-0.5.0-1`). Components place slots in a box of
  their own.
- NSH reads `leading-*` and `tracking-*` tokens as multiples and ems; RayX's line heights and
  letter spacing are points, so they are `spacing` tokens and TSX applies them through `var()`
  (`leading-(--spacing-…)`), verified in `theme_modifiers_tsx`.
- `font_weight` and `outline` take values, not bindings: Button reads its weight and ring width
  when made (the export has one value for every size and density).
- `LocalEngine` runs only when pumped (`engine.pump()`); the threaded `Engine` runs by itself.
- A platform sends a disabled view no presses, but a press dispatched to it still reaches its
  listener: Button drops presses while `disabled`.
- `nsh check/test/run` build the app's crate in their own workspace (`.nsh/rust`), where the root
  `Cargo.toml`'s `[patch.crates-io]` does not apply: without `examples/gallery/.cargo/config.toml`
  (the patch file `rayx new` writes) nsh resolved crates.io's upstream `nsh-core` 0.5.0 and
  `rayx_components` failed on the fork's theme API (2026-11-15: then theme types; since
  `nshx-0.5.0-4`, theme documents and typed keys).
- A theme token named as one of Tailwind's sizes keeps Tailwind's class: `shadow-sm` … `shadow-xl`
  are Tailwind's shadows, `blur-sm` … `blur-xl` Tailwind's blurs (nsh warns of the blurs when it
  bundles). RayX's are reached through `var()`: `shadow-(--shadow-md)`, `blur-(--blur-md)`.
- The Windows development host exposes each `testID` as a UI Automation `AutomationId`, so the
  gallery can be driven (Invoke, Scroll) and captured (`PrintWindow`) without screen control.
