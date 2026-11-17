# Navigation

Routes matched from URIs, named regions with their own journals, and one location for the whole app. How regions
show as NSH navigation stacks is in [Navigation regions](../navigation.md).

## Register

```rust
use rayx_services_navigation::{NavigationAppBuilderExt, RouteDefinition};

builder.add_navigation(|options| {
    options.add_route(RouteDefinition::new("/"));
    options.add_route(RouteDefinition::new("/projects/{id}"));
    options.add_route(RouteDefinition::new("/settings"));
})
```

The contract is `dyn NavigationService` (`rayx_app::services::navigation`): its state, events, commands and data types,
NSH-free and implementable in Rust or TS. RayX's implementation is `rayx_services_navigation`'s (`builder.addNavigation(..)`
in TS, once its set is registered with `rayx_services_navigation::register(registry)`), registered as the app's
`dyn NavigationService`, replacing any other (an app's own, registered after it, wins). It is a startup service: when
the app starts it goes to the address the app started at (a deep link), else to the default route.

`NavigationOptions` bind from `RayX:Services:Navigation`, then the closure changes them; routes that do not build fail
startup. The options include the default route, the default history mode, the root region's name, the journal's size,
the URL region (`url_region`), the location's prefix (`base_path`) and the regions the location may carry in its query.

## Routes

`RouteMap` (in `rayx_services_navigation`) matches URIs against patterns (`/projects/{id}`; literal segments win over
parameters) and gives a `Route` with its path, segments, parameters, query and fragment. External URIs and unknown
routes have their own statuses.

## Use

```rust
let navigation = services.required_service::<dyn NavigationService>()?;
navigation.navigate_to("/projects/12").await?;
navigation.back(navigation.root_region_name()).await?;
navigation.ensure_region("panel".into()).await?;
let current = navigation.current(); // the root region's route, from the state
```

The commands are `async` (a TS implementation answers on a later turn); RayX's answer at once, from any thread.

- **State** (`navigation()`, a `StateCell<NavigationState>`): every region with its journal and current route, the
  root region's name and the location. `current()`, `current_in_region(region)`, `journal(region)`, `region(name)`,
  `region_names()` and `location()` read it synchronously, on the state and on `dyn NavigationService`.
- **Commands:** `navigate(request)` (with `navigate_to(uri)` and `navigate_in_region(region, args)` over it), and per
  region `back`, `forward`, `refresh`, `go_to_entry(region, index)`, `ensure_region`, `close_region` and
  `remove_region`; `apply_location(location)`.
- **Constants:** `root_region_name()`, `url_region()` and `default_history_mode()`.
- **Events** (`events()`, `NavigationEvent`): for each successful navigation `NavigatedFrom` (when the region had a
  route), `NavigatedTo`, `RouteChanged` and `LocationChanged`, in that order, after the state changed; `RegionClosed`
  when a region closes. `on_navigated_from`, `on_navigated_to`, `on_route_changed`, `on_location_changed` and
  `on_region_closed` on `dyn NavigationService` listen to one kind each, until their subscription is dropped.

## The location

The location is the URL region's route (the root region by default) behind `base_path`, with every other region's
route in its query: `/app/projects/12?panel=%2Fsettings`.

- After every navigation in any region, RayX's service shows the location as NSH's app address
  (`set_app_url_with`): a new navigation as a new history entry; back, forward and refresh in place. It takes the
  navigation to the address at start once the app is ready, and follows the platform's moves of it and the deep links
  that open the app while it runs ([the location on NSH](../navigation.md#the-location-on-nsh)). How it shows and
  reads the location is private to the crate.
- `apply_location` takes a location the platform moved to (its back and forward, a typed address): each region goes
  to the nearest journal entry with its route or to a new one, and the query regions it no longer names close.
- Startup goes to the deep link, else to the default route. NSH answers the address at start only once the engine
  runs, so the navigation starts at its default route and moves there once the app is ready.

## TSX

`NavigationService`, resolved from the provider `start()` gives (it is the contract's token, over whichever
`dyn NavigationService` is registered):

```tsx
const navigation = services.getRequiredService(NavigationService);
navigation.navigate("/projects/12");
navigation.onChange(({ region, from, to }) => render(to));
```

`navigation.region(name)` drives a named region; see [Navigation regions](../navigation.md#from-tsx). A navigation
written in TS implements `NavigationServiceContract` and is registered under the class:
`addSingletonAs(NavigationService, () => new MyNavigation())`.
