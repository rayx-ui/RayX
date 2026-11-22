# App architecture

How a RayX app is built so that every view can be shown on its own, with data of a design's choosing, and tested
without the rest of the app: dumb views, view models that hold the state, services that do the work, all registered
in [dependency injection](dependency-injection.md) under contracts, and design implementations swapped in where the
app is composed. Each rule has a short example; `Trip` is an example app's screen.

## Views are dumb

A view renders only the data and event props it is given. It holds no state beyond what a control needs while the
user works it (focus, a field's text while it is typed), resolves nothing and calls no service.

```tsx
// DO: data and events in, nothing else.
export function TripView(props: { title: string; days: number; onSave: () => void }) {
  return (
    <View>
      <Text>{`${props.title}, ${props.days} days`}</Text>
      <Button text="Save" onPress={props.onSave} />
    </View>
  );
}

// DON'T: a view that fetches and keeps its own data.
export function TripView() {
  const trips = useServices().getRequiredService(TripService);
  const [trip, setTrip] = signalOf(trips.load());
  // …
}
```

## A screen's or a panel's state is a view model

The state of a screen or a panel, and what its events do, is a view model, resolved with `useViewModel` (Rust
`use_view_model`) under its contract. The screen's component connects it to its view and adds nothing else.

```tsx
export function Trip() {
  const trip = useViewModel(TripViewModel); // the contract's token, active while Trip lives
  return <TripView title={trip.title()} days={trip.days()} onSave={() => trip.save.execute()} />;
}
```

```rust
// In a Rust screen's component:
let trip = use_view_model::<dyn TripViewModel>()?; // the contract, active until the component's cleanup
let title = use_reactive(&trip.title())?;
trip_view(title, move || trip.save())
```

## Work and data are services

Work, I/O and data (files, the network, storage, the clock) are services, each under a contract: in TS a
`ServiceToken` named as its interface, in Rust a trait. A view model takes the services it needs from the provider it
is resolved in, never a concrete class.

```ts
export interface TripService {
  load(id: string): Promise<Trip>;
}
export const TripService = new ServiceToken<TripService>("TripService");
```

```rust
#[contract]
pub trait TripService {
    async fn load(&self, id: String) -> Result<Trip, TripError>;
}
```

## Names

A service's contract is `<Name>Service`, with the implementations `Default<Name>Service` (the app's) and
`Design<Name>Service` (what a design shows); a view model's is `<Name>ViewModel`, with `Default<Name>ViewModel` and
`Design<Name>ViewModel`.

```ts
export class DefaultTripService implements TripService { /* reads the app's data */ }
export class DesignTripService implements TripService { /* fake trips */ }
export class DefaultTripViewModel implements TripViewModel { /* over a TripService */ }
export class DesignTripViewModel implements TripViewModel { /* fixed state, no service */ }
```

## The composition root, and design registrations beside it

The app is composed in `configure(builder)`, exported from its `App.ts`. Design implementations are registered in
`configurePreviews(builder)` beside it, which a design tool calls after `configure`, so its registrations replace the
app's. A view model or a service never asks whether it runs in a design tool (`design.active()`): what differs is
registered, not branched on.

```ts
// App.ts
export function configure(builder: RayXAppBuilder): RayXAppBuilder {
  return builder
    .addPlatform().addSuspension().addAssets().addFonts()
    .addSingletonAs(TripService, () => new DefaultTripService())
    .addScopedAs(TripViewModel, (services) => new DefaultTripViewModel(services.getRequiredService(TripService)));
}

export function configurePreviews(builder: RayXAppBuilder): RayXAppBuilder {
  return builder.addSingletonAs(TripService, () => new DesignTripService());
}
```

```ts
// DON'T: a service that branches on design mode.
class DefaultTripService implements TripService {
  load(id: string) {
    return design.active() ? Promise.resolve(FAKE_TRIP) : fetchTrip(id);
  }
}
```

## What design implementations give

A `Design<Name>Service` may give fake data, or data from a development service (a local server, a fixture
folder). Design registrations replace persistent stores with in-memory ones, so a design never reads or writes the
app's data.

```ts
export function configurePreviews(builder: RayXAppBuilder): RayXAppBuilder {
  return builder
    .addSingletonAs(TripService, () => new DesignTripService(SAMPLE_TRIPS))
    .addSingletonAs(TripStoreService, () => new InMemoryTripStoreService());
}
```

## Data of one state comes from a preview state's services

What one preview state shows comes from that state's `services`, registered in the preview instance's own scope
([previews](previews.md#design-services)): for its tokens they win over the app's and `configurePreviews`' registrations
for that instance alone.

```tsx
states: {
  Empty: { services: (scope) => scope.addScopedAs(TripViewModel, () => new DesignTripViewModel([])) },
  Long: { services: (scope) => scope.addScopedAs(TripViewModel, () => new DesignTripViewModel(LONG_TRIP)) },
},
```

The order, for each instance: `configure`, `configurePreviews`, the state's `services`, then the state's `props`.

## No module-level mutable state

State lives in view models and services, which the provider makes and drops with their scope. A module holds no
mutable state (no `let` or `signal` at the top of a file, no Rust `static` with interior mutability serving as an
app's state): two instances of one view in one process must not share it.

```ts
// DON'T
export const selectedTrip = signal<string | null>(null);

// DO: the state is the view model's, one per scope.
export class DefaultTripListViewModel implements TripListViewModel {
  readonly selected = signal<string | null>(null);
}
```

## Tests

A view model is tested with fake services; a view with literal props.

```ts
const trip = new DefaultTripViewModel(new DesignTripService([TRIP]));
await trip.load.execute("lisbon");
expect(trip.title()).toBe("Lisbon");
```

```tsx
render(<TripView title="Lisbon" days={3} onSave={() => saved++} />);
```

## Adopting it in an app made by `rayx new`

An app made from a `rayx new` template starts small and adopts these rules as it grows: its composition root moves into
an `App.ts` with `configure(builder)` (and `App.tsx`, which imports it as `./App.ts`, keeps only components), and
`configurePreviews(builder)` goes beside it. An app composed in Rust (its root is `RayXAppBuilder::new()….start()`) has
an `App.ts` beside its `App.tsx` too, composing through the TS builder and its extensions: RayX's builder methods are
the Rust ones in camelCase, and the app's own Rust extensions are exported to TS with `#[app_extension]`, so
`configure` registers what the Rust root does and `configurePreviews` replaces it.

## See also

- [Dependency injection](dependency-injection.md): lifetimes, scopes, screens, and scopes with registrations of their
  own (`createScope(configure)`).
- [View models](view-models.md): activation, commands and binding, in Rust and TSX.
- [Previews](previews.md): preview files, `configurePreviews` and a state's `services`.
