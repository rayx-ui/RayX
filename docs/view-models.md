# View models

A view model holds a screen's state as [dXn.Reactive](https://github.com/dXnSix/dXn.Reactive) signals, streams, lists
and commands. RayX resolves it in the screen's scope, activates it while the screen is shown, and binds its state to the
UI as NSH state.

A view model notifies on the thread that changed it; the bindings carry each change to the engine's thread.

## Activation

```rust
let vm = use_view_model::<TodoViewModel>()?; // resolved in the screen's scope, active until its cleanup
```

The view model implements `ActivatableViewModel` and is registered like any other service, usually scoped so each
screen gets its own. For a TSX screen, a Rust bridge takes the screen's scope as an `Instance<ServiceScope>` and
resolves in it; the view model stays active until the scope is disposed:

```rust
let vm = scope.with_mut(|scope| scope.view_model::<TodoViewModel>())?;
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
