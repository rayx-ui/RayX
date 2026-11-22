# View models

A view model holds a screen's state as [dXn.Reactive](https://github.com/dXnSix/dXn.Reactive) signals, streams, lists
and commands. RayX resolves it in the screen's scope, activates it while the screen is shown, and binds its state to the
UI as NSH state. Where view models sit in an app, under which contracts and names, is the
[app architecture](app-architecture.md).

A view model notifies on the thread that changed it; the bindings carry each change to the engine's thread.

## Activation

```rust
let vm = use_view_model::<TodoViewModel>()?; // resolved in the screen's scope, active until its cleanup
```

The view model implements `ActivatableViewModel` and is registered like any other service, usually scoped so each
screen gets its own. In TSX, `useViewModel` activates it ([below](#activation-in-tsx)); a Rust bridge can also take a
scope (the `ServiceProvider` that
`services.createScope()` makes) as an `Instance<ServiceProvider>` and resolves in it; the view model stays active until
the scope is disposed (`TodoBridge`, in the fragment below, is that bridge, the app's class):

```rust
let vm = scope.with_mut(|scope| scope.view_model::<TodoViewModel>())?;
```

<!-- tsx: fragment -->
```tsx
const scope = services.createScope();      // on the screen's mount
const todos = TodoBridge.viewModel(scope); // the bridge above
scope.dispose();                           // in its cleanup: deactivates it
```

## The TS side

TSX has the same API in RayX's native set, as functions and classes that call the Rust ones (nothing is
reimplemented in TS). View models written in TS are TS objects; Rust view models are reached through their class.

| Rust (`rayx_app_nsh`) | TS (the app's set) |
| --- | --- |
| `services()` | `useServices(): ServiceProvider` |
| `use_screen_scope()` | `useScreenScope(): ServiceProvider` |
| `provide_services(provider)` | `provideServices(provider)` |
| `use_view_model::<T>()` | `useViewModel(token)` |
| `ViewModelActivator`, `ActivationToken`, `DisposeBag`, `ActivatableViewModel` (`dxn_reactive`) | the same names, over `dxn_reactive`'s own |
| a `dxn_reactive` command, bound as `ReactiveCommand` | `ReactiveCommand` (unchanged); `Command` for TS view models |
| `app_state()` | `appState()` |

The hooks run in the calling component's body: NSH runs their Rust code under the Rust owner it ties to the
component's JS owner (`CallCx::with_owner`), so Rust and TSX components in one tree share one screen scope, and what
they activate ends with that owner ([dependency injection](dependency-injection.md#screens-in-tsx)).

## Activation in TSX

A TS view model is an object whose `activator` is a `ViewModelActivator` (the `ActivatableViewModel` type).
`whenActivated(bag => …)` registers work that runs against a fresh `DisposeBag` when the first live token is taken;
more tokens only count; disposing the last token disposes that activation's bag (each function or `Disposable` in it,
once); the next first token runs every callback again; a callback registered while active runs at once. These are
`dxn_reactive`'s own activator and bag, so the behaviour is the Rust one. Registered like any TS service (usually
scoped), it is activated by `useViewModel(token)` for as long as the calling component lives:

<!-- tsx: fragment -->
```tsx
class CounterViewModel {
  constructor() {
    this.count = signal(0);
    this.activator = new ViewModelActivator();
    this.activator.whenActivated((bag) => {
      log.push("ts on");
      bag.add(() => log.push("ts off"));
    });
  }
}
```

<!-- tsx: fragment -->
```tsx
function Uses(props) {
  const ts = useViewModel(CounterViewModel);
  const rust = useViewModel(PageViewModelClass);
```

`useViewModel` resolves the token in `useServices()`, as `getRequiredService` does. A Rust view model is reached
through its class, registered in the set with `registry.view_model_class::<C>()` (`ServiceClassRegistryExt`), and
activated as `ServiceProvider::view_model` activates it. Two components using one view model keep it active until the
last of them cleans up. A token with nothing registered throws naming it; a service that is not a view model throws
saying so; nothing is activated then. A TS view model's state is its own NSH signals, which a view reads directly:

<!-- tsx: fragment -->
```tsx
  return <Text>{`${props.name}: ${ts.count()}`}</Text>;
```

## Commands in TSX

`Command` is the command of TS view models: made from an action (sync or returning a promise) and an optional
`canExecute` accessor. `canExecute()` and `isExecuting()` read as NSH signals. `await execute(arg?)` passes `arg` to
the action; it follows `dxn_reactive`'s `IgnoreWhileRunning`: while `canExecute()` is false or while it runs, it
rejects saying the command cannot run, so of two overlapping calls the second rejects and the action runs once.
`isExecuting()` is true until the action's promise settles, and the action's error is rethrown once it is false again.

<!-- tsx: fragment -->
```tsx
const command = new Command(async (arg) => {
  ran.push(String(arg));
  await new Promise((resolve) => setTimeout(resolve, 20));
  if (arg === "boom") throw new Error("the action broke");
});
```

A component bound to a view model turns either command into a plain value for its view (enabled, busy) and a
callback its view calls (a press); in the fragments, copied from RayX's tests, `presses` and `labels` are the test's
own records. A `Command` through its accessors:

<!-- tsx: fragment -->
```tsx
function SaveButton(props) {
  const enabled = () => props.command.canExecute() && !props.command.isExecuting();
  const save = () => props.command.execute().catch(() => {});
  presses.save = save;
  const label = () => (enabled() ? "save" : "saving");
  effect(() => labels.push(label()));
  return <View onPress={save}><Text>{label()}</Text></View>;
}
```

A Rust view model's `ReactiveCommand` through its `canExecute` `Observable`, bound into a signal (as
[below](#binding-in-tsx)), with `execute()` as the callback:

<!-- tsx: fragment -->
```tsx
function StepButton(props) {
  const enabled = signal(props.command.canExecute.value);
  props.command.canExecute.bind(enabled.set);
  const step = () => props.command.execute();
  presses.step = step;
  return <View onPress={step}><Text>{enabled() ? "step" : "wait"}</Text></View>;
}
```

## Binding in a Rust component

Each hook returns an NSH signal that starts at the view model's value and follows it until the component's cleanup.

```rust
let count = use_reactive(&vm.count.read_only())?;  // ReadSignal<u32>
let title = use_two_way(&vm.title)?;               // Signal<String>: what the UI writes goes back
let items = use_list(&vm.items)?;                  // ReadSignal<Vec<TodoItem>> from an ObservableList
let status = use_stream(&vm.status, Status::Idle)?; // ReadSignal<Status> from a ValueStream
let save = use_command(&vm.save)?;                 // can_execute, is_executing, execute()
```

## Binding in TSX

A view-model class exposes `Observable`s and `ReactiveCommand`s:

```rust
#[class]
pub struct CounterVm {
    count: VmSignal<u32>,
    increment: Command,
}

#[class]
impl CounterVm {
    pub fn new() -> Self {
        let count = VmSignal::new(0_u32);
        let counter = count.clone();
        let increment = Command::new(move || {
            counter.update(|count| *count += 1).map(|_| ()).map_err(Into::into)
        });
        Self { count, increment }
    }

    #[getter]
    pub fn count(&self) -> Result<Instance<Observable>, String> {
        Observable::of(&self.count.read_only()).map_err(|error| error.to_string())
    }

    #[getter]
    pub fn increment(&self) -> Instance<ReactiveCommand> {
        ReactiveCommand::of(&self.increment)
    }
}
```

```tsx
const vm = new CounterVm();
const count = signal(vm.count.value);
vm.count.bind(count.set); // now and on every change
vm.increment.execute();   // throws when the command refuses
```

`Observable` has `value`, `bind(setter)` and `onChange(listener)`; `ReactiveCommand` has `execute()`, and `canExecute`
and `isExecuting` as `Observable`s.
