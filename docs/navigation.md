# Navigation

The navigation service (`add_navigation(..)`) keeps a journal per region: the main region follows the app's URL, and
any number of named regions (a side panel, tabs, a dialog) navigate on their own. The URL region's route is the
location's path; the other regions' routes are in its query (`/projects/12?panel=%2Fsettings`). The service itself,
its routes and options are in [the navigation service](services/navigation.md).

## Regions as NSH navigation stacks

`navigation_region` shows a region as an NSH navigation stack whose pages are the region's journal up to its current
entry. Navigating pushes, replaces, resets, goes back or forward; a page the user pops natively (a swipe back, the
system back button) takes the journal back to the page below.

```rust
let pages = navigation_region("root", |route| {
    nav_page().title(route.path.clone()).child(screen_for(route))
})?;
```

`navigation_view` shows a region as its current route alone, rendered again on every navigation: for a region with no
stack, such as tabs or a side panel.

```rust
let panel = navigation_view("panel", |route| side_panel(route))?;
```

Both are called in a component's body, under the app's services.

## The location on NSH

On NSH the navigation's location is NSH's app address (`app_url`, `set_app_url_with`; TS's `UrlScheme.url()` and
`setUrl`): the web shows it in the address bar and keeps it in the browser's history, and the native platforms keep it
for the app, starting at the link that opened it. `rayx_app` registers `NshLocationSynchronizer` for it, so nothing
is to set up:

- **Startup** goes to the default route, then, once the app is ready, to NSH's address at start (the page's address,
  a deep link), which NSH answers only once the engine runs. Nothing is shown before that, so the address stays.
- **Navigations** show their location there: a push as a new history entry, the other modes (replace, reset, back,
  forward, refresh) in place of the current one. Closing a region is a navigation of the default mode, a push, so
  back opens it again.
- **The navigations of one turn share one history entry**, as everything an NSH app shows in one turn does: a TSX
  handler that navigates the main region and opens a panel leaves one entry, a push when any of them was one.
- **The platform's moves** (the browser's back and forward, a typed address: NSH's `next_app_url_change`) take the
  navigation along: each region goes to the nearest journal entry with its route, or to a new one, and a query region
  the location no longer names closes. They are not sent back. The synchronizer is the one listener NSH allows, so
  with it on the app hears them through the navigation rather than TS's `UrlScheme.on("change")`.

The address is a URL and the location the part after its origin (`https://example.com/app/projects/1` is
`/app/projects/1`), or after the scheme for a link (`myapp://projects/1` is `/projects/1`; `location_of_address`). On
the web set the navigation's `base_path` to `nsh.json`'s `web.base`; a location shown is set as it is, which the web
resolves against the page's origin. `synchronize_location: false` in the navigation's options keeps navigations out of
NSH's address and its moves out of the navigation, startup included.

## From TSX

`Navigation.of(app)` drives the URL region: `navigate(uri)`, `back()`, `forward()`, `current()` and `onChange`.

`NavigationRegion.of(app, "panel")` drives a named region:

- `navigate(uri, replace?)`, `back()`, `forward()`, `goTo(index)`, `close()`;
- `pages()`: the journal up to the current entry, for a `NavStack` of `NavPage`s, each `onDismiss` going to the page
  below;
- `onChange`: every change of its pages.

```tsx
const panel = NavigationRegion.of(app, "panel");
const pages = signal(panel.pages());
panel.onChange(pages.set); // called with the pages after every change
panel.navigate("/settings");
```
