# Navigation

Routes matched from URIs, named regions with their own journals, and one location for the whole app. How regions
show as NSH navigation stacks is in [Navigation regions](../navigation.md).

## Register

```rust
builder.add_navigation(|options| {
    options.add_route(RouteDefinition::new("/"));
    options.add_route(RouteDefinition::new("/projects/{id}"));
    options.add_route(RouteDefinition::new("/settings"));
})
```

`NavigationOptions` bind from `RayX:Services:Navigation`, then the closure changes them; routes that do not build fail
startup. The options include the default route, the default history mode, the root region's name, the journal's size,
the URL region (`url_region`), the location's prefix (`base_path`) and the regions the location may carry in its query.

## Routes

`RouteMap` matches URIs against patterns (`/projects/{id}`; literal segments win over parameters) and gives a `Route`
with its path, segments, parameters, query and fragment. External URIs and unknown routes have their own statuses.

## Use

```rust
let navigation = services.required_service::<NavigationService>()?;
navigation.navigate_to("/projects/12")?;
navigation.back()?;
let panel = navigation.get_or_create_region("panel")?;
```

- **Regions:** named regions keep their own journals (push, replace, reset, none; back and forward):
  `back_in_region`, `forward_in_region`, `go_to_entry(region, index)`, `close_region(region)`.
- **Subscribers:** `on_navigated_from`, `on_navigated_to`, `subscribe_route_changed`, `subscribe_location_changed`,
  `subscribe_region_closed`.

## The location

The location is the URL region's route (the root region by default) behind `base_path`, with every other region's
route in its query: `/app/projects/12?panel=%2Fsettings`.

- After every navigation in any region, the service shows the location through the app's
  `NavigationLocationSynchronizer` (from DI, `with_navigation_location_synchronizer`): a new navigation as a new
  history entry; back, forward and refresh in place. On NSH it goes through NSH's own `location()` and `navigate()`;
  the default synchronizer does nothing.
- `apply_location` takes a location the platform moved to (its back and forward, a typed address): each region goes
  to the nearest journal entry with its route or to a new one, and the query regions it no longer names close.
- Startup goes to the synchronizer's `location()` (the deep link), else to the default route.

## TSX

```tsx
const navigation = Navigation.of(app);
navigation.navigate("/projects/12");
navigation.onChange(({ region, from, to }) => render(to));
```

`NavigationRegion.of(app, name)` drives a named region; see [Navigation regions](../navigation.md#from-tsx).
