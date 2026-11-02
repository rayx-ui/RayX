# RayX documentation

## Guides

- [Getting started](getting-started.md): install the CLI, create an app, add RayX to a crate, upgrade.
- [Architecture](architecture.md): the crates, the two layers, dependencies and releases.
- [Dependency injection](dependency-injection.md): services, lifetimes, scopes and screens.
- [Configuration and options](configuration.md): configuration sources and .NET's options model.
- [Startup](startup.md): services that initialize asynchronously, and starting the app from TSX or Rust.
- [View models](view-models.md): activation, and binding dXn.Reactive state to Rust components and TSX.
- [Navigation](navigation.md): navigation regions as NSH navigation stacks.
- [TSX](tsx.md): the classes a TSX app uses.

## Reference

- [App services](services.md): every service in `rayx_runtime`, how it is registered and configured.
- [CHANGELOG](../CHANGELOG.md): every release, with `Migration:` notes for breaking changes.

## The CLI

- [README](../crates/rayx_cli/README.md): installing `rayx`, and every command.
- [Projects](cli/projects.md): how `rayx` finds a project and its pins.
- [Decisions](cli/decisions.md): the design decisions that still stand.
- [Design proposal](cli/architecture/rayx-nsh-proposal.md): `rayx` on NSH.
- [Manual verification](cli/manual-verification.md): checks run by hand on real machines.

## Maintaining RayX

- [Releasing RayX](releasing.md): release-plz, cargo-dist, the 0.x version policy and pushing to `main`.
- [Contributor guidance](../AGENTS.md): crates, dependencies, validation and commits.
