# Configuration and options

## Configuration

The app builder's `configuration` collects sources:

- values in memory, a JSON tree or JSON text;
- a `ConfigurationSource` for anything else (a file, a server), which may load asynchronously;
- on NSH, `add_app_settings(package)`: the app's bundled `assets/appsettings.json`.

Later sources' values go over earlier ones'. The built `Configuration` is a singleton, read by path
(`"Api:Retry:Limit"`) and by section; `set` and `reload` change it and notify its listeners.

```rust
fn build_app(builder: RayXAppBuilder) -> RayXAppBuilder {
    builder
        .add_app_settings("my-app")
        .configure_app_configuration(|configuration| {
            configuration.add_json(OVERRIDES);
        })
}
```

## Options

Options follow .NET's options model: register a type on the services, bind it to a configuration section, and read it
by type.

```rust
#[derive(Default, Serialize, Deserialize)]
struct ApiOptions {
    base_url: String,
    retry_limit: u32,
}

let mut app = RayXAppBuilder::new();
app.configuration.add_json(SETTINGS); // { "Api": { "baseUrl": "https://api.example.com", "retryLimit": 3 } }
app.services
    .add_options::<ApiOptions>()
    .bind("Api")                                                          // the section over the defaults
    .configure(|options| options.base_url.push('/'))                      // configure steps, in order
    .post_configure(|options| options.retry_limit = options.retry_limit.min(5)) // then post-configure
    .validate(|options| !options.base_url.is_empty(), "the API needs a base URL") // then validation
    .validate_on_start();                                                 // fail startup, not the first use
let services = app.build()?;
let options = services.options::<ApiOptions>()?;
```

The same with the builder's chained form:

```rust
builder.add_options::<ApiOptions>(|options| {
    options.bind("Api").validate_on_start();
})
```

### Binding

- Options start from `T::default()`; options nobody registered are just that.
- `bind` needs `Serialize` and `Deserialize`. Keys match ignoring case, `_` and `-`, so `retryLimit` binds
  `retry_limit`.
- A list is replaced as a whole (.NET appends to it), and a `null` binds nothing.

### Reading

| Accessor | .NET | Made |
| --- | --- | --- |
| `services.options::<T>()` | `IOptions<T>` | once |
| `services.options_snapshot::<T>()` | `IOptionsSnapshot<T>` | once per scope |
| `services.options_monitor::<T>()` | `IOptionsMonitor<T>` | again when its configuration changes; `on_change` hears it |

### More steps

- Named options: `add_named_options::<T>(name)`, `configure_named`, `configure_all`, `post_configure_all`.
- Steps that take a service: `configure_with::<D>`, `post_configure_with::<D>`, `validate_with::<D>`.
- Startup makes the options registered with `validate_on_start` before it initializes any startup service; an invalid
  one fails startup.

## The services' options

Every RayX service reads its options from the section `RayX:Services:<Name>` (the app's own settings from
`RayX:Application`), so `appsettings.json` can configure them; see [App services](services.md).
