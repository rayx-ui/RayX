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
