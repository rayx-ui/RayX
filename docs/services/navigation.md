# Navigation

Routes matched from URIs, named regions with their own journals, and one location for the whole app. How regions
show as NSH navigation stacks is in [Navigation regions](../navigation.md).

## Register

| | |
| --- | --- |
| Trait | `rayx_app::services::navigation::NavigationService` (`rayx::app::services::navigation`) |
| Crate | `rayx_services_navigation` (`rayx::services::navigation`) |
| Feature | `navigation` |
| Rust | `add_navigation(configure)` (`NavigationAppBuilderExt`) |
| TS | `addNavigation((options) => ..)`, and the classes `NavigationService` and `NavigationRegion` |
| Requires | nothing |

```rust
use rayx_services_navigation::{NavigationAppBuilderExt as _, RouteDefinition};

let builder = RayXAppBuilder::new().add_navigation(|options| {
    options.add_route(RouteDefinition::new("/"));
    options.add_route(RouteDefinition::new("/projects/{id}"));
    options.add_route(RouteDefinition::new("/settings"));
});
```

```tsx
new RayXAppBuilder()
  .addPlatform().addSuspension().addAssets().addFonts()
  .addNavigation((options) => options.addRoute("/").addRoute("/projects/{id}", { component: "Project" }))
  .start()
  .then((services) => { /* the app's services, navigation among them */ });
```

`add_navigation(..)` registers RayX's implementation as the app's `dyn NavigationService`, replacing any earlier
registration of it. It is a startup service: when the app starts it goes to the address the app started at (a deep
link), else to the default route. Call it once; more steps go through `configure_options::<NavigationOptions>(..)`.
TS's `addRoute(pattern, { component, prewarm })` returns the options, so calls chain.

The TS side is there once the crate's set is registered: `#[rayx::native_module]` with the `navigation` feature on,
or `rayx_services_navigation::register(registry)` by hand.

## Configure

`NavigationOptions` bind from `RayX:Services:Navigation` (`NAVIGATION_OPTIONS_SECTION`), then the closure changes
them; routes that do not build fail startup.

```json
{ "RayX": { "Services": { "Navigation": {
  "defaultRoute": "/projects", "basePath": "/app", "queryRegions": ["panel"]
} } } }
```

- `routes`: `RouteDefinition`s (a pattern, the component it shows, whether its screen is made early);
- `default_route` (`/`), `default_history_mode` (`Push`), `root_region_name` (`root`) and `max_journal_entries` (40);
- `url_region` (the root region by default), `base_path` (the location's prefix, none by default) and `query_regions`
  (the regions the location carries in its query, so a location read at startup reaches them before they exist);
- `base_uri` (`app://`), `synchronize_location` (`true`), and the routes for unknown, external and component-less
  URIs (`not_found_route`, `external_route`, `component_not_found_route`).

## Routes

`RouteMap` (in `rayx_services_navigation`) matches URIs against patterns (`/projects/{id}`; literal segments win over
parameters) and gives a `Route` with its path, segments, parameters, query and fragment. External URIs and unknown
routes have their own statuses (`SkippedExternalRoute`, `FailedNotFound`).

## The contract

`NavigationService` (`NavigationServiceContract` in TS). A command naming a region that does not exist makes it
(except `go_to_entry`, which needs its journal).

Published state:

- `navigation()` (`navigation`): a `StateCell<NavigationState>`, every region with its journal and current route, the
  root region's name and the location.

Events:

- `events()` (`events`): `NavigationEvent`s. For each successful navigation `NavigatedFrom` (when the region had a
  route), `NavigatedTo`, `RouteChanged` and `LocationChanged`, in that order, after the state changed; `RegionClosed`
  when a region closes.

Constants:

- `root_region_name()` (`rootRegionName`): where `navigate_to(uri)` navigates.
- `url_region()` (`urlRegion`): the region whose route is the location's path.
- `default_history_mode()` (`defaultHistoryMode`): how `navigate_to(uri)` changes the root region's journal.

Commands (`async`, `Result<_, NavigationError>`; promises in TS):

- `navigate(request)` (`navigate`): navigates `request.region` to `request.args.uri`; a `NavigationResult` with the
  direction, status (`Success`, `FailedNotFound`, `SkippedSameRoute`, ...) and route.
- `back(region)`, `forward(region)` (`back`, `forward`): through the region's journal (`SkippedNoHistory` at its end).
- `refresh(region)` (`refresh`): the region's current route again.
- `go_to_entry(region, index)` (`goToEntry`): the journal's entry `index`.
- `ensure_region(region)` (`ensureRegion`): makes the region when it does not exist.
- `close_region(region)` (`closeRegion`): empties its journal (not the URL region's); whether it was open.
- `remove_region(region)` (`removeRegion`): removes it (not the root region); whether it existed.
- `apply_location(location)` (`applyLocation`): goes where a location the platform moved to says (below).

Rust helpers on `dyn NavigationService`:

- `navigate_to(uri)`: the root region, with the default history mode; `navigate_in_region(region, args)`.
- `current()`, `current_in_region(region)`, `journal(region)`, `region(name)`, `region_names()` and `location()`: read
  the state synchronously (`NavigationState` has the same, and `pages(region)`).
- `on_navigated_from`, `on_navigated_to`, `on_route_changed`, `on_location_changed` and `on_region_closed`: listen to
  one kind of event each, until their subscription is dropped.

## Use

```rust
use rayx::app::services::navigation::NavigationService;

let navigation = services.required_service::<dyn NavigationService>()?;
navigation.navigate_to("/projects/12").await?;
navigation.back(navigation.root_region_name()).await?;
navigation.ensure_region("panel".into()).await?;
let current = navigation.current(); // the root region's route, from the state
let _subscription = navigation.on_route_changed(|changed| println!("{}", changed.to.path));
```

A TS implementation answers on a later turn; RayX's answers at once, from any thread.

## On NSH

RayX's implementation routes the app's URIs, shows the location as NSH's app address and renders regions as NSH
navigation stacks: `navigation_region(name, render)` (a stack of the region's journal up to its current entry) and
`navigation_view(name, render)` (its current route alone), both over whichever `dyn NavigationService` is
registered ([Navigation regions](../navigation.md)).

The location is the URL region's route (the root region by default) behind `base_path`, with every other region's
route in its query: `/app/projects/12?panel=%2Fsettings`.

- After every navigation in any region, RayX's service shows the location as NSH's app address
  (`set_app_url_with`): a new navigation as a new history entry; back, forward and refresh in place. It takes the
  navigation to the address at start once the app is ready, and follows the platform's moves of it and the deep links
  that open the app while it runs ([the location on NSH](../navigation.md#the-location-on-nsh)). How it shows and
  reads the location is private to the crate; `location_of_address(address, base_path)` is the location an address
  holds.
- `apply_location` takes a location the platform moved to (its back and forward, a typed address): each region goes
  to the nearest journal entry with its route or to a new one, and the query regions it no longer names close.
- Startup goes to the deep link, else to the default route. NSH answers the address at start only once the engine
  runs, so the navigation starts at its default route and moves there once the app is ready.

## TSX

`NavigationService`, resolved from the provider `start()` gives, wraps whichever `dyn NavigationService` is
registered:

- `navigate(uri)`, `back()`, `forward()`, `refresh()`: the root region; each a promise of the `NavigationResult`
  (`{ region, direction, status, route }`), rejected with the service's error;
- `current()`: the root region's current route and its status (a `RoutingData`, `{ status, route }`), `null` before
  the first navigation;
- `region(name)`: the `NavigationRegion` `name`, made when missing;
- `onChange(({ region, from, to }) => ..)`: every route change (a `RouteChangedEvent`, `from` `null` for the region's
  first), until the function it returns is called.

A route is a `NavigationRoute` in TS, the JSON of Rust's `Route` (`uri`, `path`, `params`, `query_params`,
`passed_parameters`, `component`, `pattern`, …); `status` a `RoutingStatus` (`"Success"`, `"FailedNotFound"`, …) and
`direction` a `NavigationDirection` (`"New"`, `"Back"`, `"Forward"`, `"Refresh"`).

`NavigationRegion` has `navigate(uri, replace?)`, `back()`, `forward()`, `refresh()`, `goTo(index)`, `close()` (each a
promise; `close` of whether the region was open), `pages()` (its journal up to the current entry, bottom to top: a
`NavigationRoute[]`) and `onChange((pages) => ..)`; see
[Navigation regions](../navigation.md#from-tsx).

The classes' commands are promises, over RayX's implementation and a TS one alike; their reads (`current()`,
`pages()`) are synchronous. Over a TS implementation, which makes a region later, `region(name)` starts making it and
every command of the region waits for that one call, those issued at once included (and rejects with why it could
not); a command after a failed call tries again.

```tsx
declare function render(route: NavigationRoute): void; // the app's own

// With the provider `start()` gives, in its `.then(async (services) => { … })`:
async function openProject(services: ServiceProvider) {
  const navigation = services.getRequiredService(NavigationService);
  await navigation.navigate("/projects/12");
  const stop = navigation.onChange(({ region, from, to }) => render(to));
  const panel = navigation.region("panel");
  await panel.navigate("/settings");
}
```

## Replace it

An app may register its own `dyn NavigationService` in place of `add_navigation(..)`. RayX's
[selection](selection.md) then clears on its route changes, and the classes and `navigation_region` follow it.

```rust
builder.add_singleton_as::<dyn NavigationService>(Arc::new(MyNavigation::new()) as Arc<dyn NavigationService>)
```

One that must initialize at the start registers itself with `add_startup_singleton_for` in place of `add_singleton_as`,
as `add_navigation(..)` does: the trait and a startup service registered as it
(`builder.add_startup_singleton_for::<dyn NavigationService, _>(Arc::new(MyNavigation::new()))`, `MyNavigation` implementing
`AsyncInitializable` and `StartupInitializable`; [Startup](../startup.md#implementing-a-trait)). Registered
after `add_navigation(..)`, the app's own replaces it completely: RayX's never starts.

In TS, a class implementing `NavigationServiceContract`, registered under `NavigationService`: its state a
`ContractState`, its events a `ContractEvents`, the constants fields, the commands `async` methods. Values cross as
the JSON of the Rust types (snake_case fields: `root_region`, `history_mode`).

```tsx
// The app's own: the JSON of a `Route` (uri, base_uri, path, segments, params, query, ...).
declare function routeOf(uri: string): NavigationRoute;

class MyNavigation implements NavigationServiceContract {
  navigation = new ContractState<NavigationState>({ root_region: "main", regions: {}, location: "/" });
  events = new ContractEvents<NavigationEvent>();
  rootRegionName = "main";
  urlRegion = "main";
  defaultHistoryMode = "Push" as const;
  async navigate(request: NavigationRequest): Promise<NavigationResult> {
    this.navigation.publish({ ...this.navigation.value, location: request.args.uri });
    this.events.publish({ kind: "RouteChanged", region: request.region, from: null, to: routeOf(request.args.uri) });
    return { region: request.region, direction: "New", status: "Success", route: routeOf(request.args.uri) };
  }
  async back(region: string): Promise<NavigationResult> { throw new Error("no history"); }
  async forward(region: string): Promise<NavigationResult> { throw new Error("no history"); }
  async refresh(region: string): Promise<NavigationResult> { throw new Error("no route"); }
  async goToEntry(region: string, index: number): Promise<NavigationResult> { throw new Error("no history"); }
  async ensureRegion(region: string) {}
  async closeRegion(region: string) { return false; }
  async removeRegion(region: string) { return false; }
  async applyLocation(location: string) { return []; }
}

new RayXAppBuilder()
  .addPlatform().addSuspension().addAssets().addFonts()
  .addSingletonAs(NavigationService, () => new MyNavigation())
  .start()
  .then((services) => { /* Rust resolves `dyn NavigationService` as MyNavigation */ });
```

For tests, `rayx_app::testing::FakeNavigation` navigates any URI and records the requests (`requests()`).
