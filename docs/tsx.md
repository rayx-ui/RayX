# TSX

A TSX app reaches RayX through classes. Register the ones it uses in the app's UI set (its `#[native_module]`), beside
`AppServices`:

```rust
#[native_module("my-app")]
fn native(registry: &mut Registry) {
    set_app(build_app);
    registry
        .class::<AppServices>()
        .class::<ServiceScope>()
        .class::<I18n>()
        .class::<Navigation>()
        .class::<Storage>();
}
```

## The app and its scopes

| Class | What it does |
| --- | --- |
| `AppServices` | `AppServices.load()` starts the app once and resolves when every startup service is ready; `app.scope()` makes a screen's scope |
| `ServiceScope` | A screen's scope: `child()` for a nested one, `dispose()` in the screen's cleanup. Rust bridges take it as `Instance<ServiceScope>` |

```tsx
const app = await AppServices.load();
const scope = app.scope(); // on a screen's mount
scope.dispose();           // in its cleanup
```

See [Startup](startup.md) and [Dependency injection](dependency-injection.md).

## The services

Each is made with `X.of(app)`, and its `onChange` hears the service's changes.

| Class | What it does |
| --- | --- |
| `I18n` | `t(id, args)`, `has(id)`, `formatDate(instant, options)` in the user's time zone, `formatNumber(value, options)`, `timeZone()`, `locale()`, `languages()`, `direction()`, `setLanguage(language)` ([I18n](services/i18n.md#tsx)) |
| `Navigation` | The URL region: `navigate(uri)`, `back()`, `forward()`, `current()` |
| `NavigationRegion` | A named region: `NavigationRegion.of(app, name)`, `navigate`, `back`, `forward`, `goTo`, `close`, `pages()` ([Navigation](navigation.md)) |
| `Storage` | Text and JSON values by bucket and key, in NSH's storage: `get`, `set`, `getJson`, `setJson`, `remove`, `keys`, `clear` |
| `AppConfiguration` | The app's settings: `current()`, `layoutDirection()`, `reducedMotion()` |
| `Assets` | Asset URLs: `url(path)`, `urlForScale(path, scale)` |
| `Fonts` | Font aliases: `family(alias)`, `aliases()` |
| `Platform` | The device and the app's lifecycle: `current()`, `onChange`, `lifecycle()`, `onLifecycle`, `askBeforeQuit(listener)`, `quit()` ([Suspension](services/suspension.md#tsx)) |

```tsx
const i18n = I18n.of(app);
title.set(i18n.t("welcome", { name: "Ada" }));
i18n.onChange(() => title.set(i18n.t("welcome", { name: "Ada" })));

const storage = Storage.of(app);
storage.setJson("settings", "theme", { mode: "dark" });
```

## View models

A view-model class exposes `Observable`s and `ReactiveCommand`s; see [View models](view-models.md#binding-in-tsx).
