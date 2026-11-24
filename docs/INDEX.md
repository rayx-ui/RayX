# RayX documentation

## Guides

- [Getting started](getting-started.md): install the CLI, create an app, add RayX to a crate and pick its features,
  upgrade.
- [Architecture](architecture.md): the crates (the core, the NSH host, the service crates, the umbrella), the two
  layers, the dependency rules, the umbrella's features, and releases.
- [Dependency injection](dependency-injection.md): services, required services and declared dependencies, lifetimes,
  scopes (with registrations of their own) and screens.
- [Configuration and options](configuration.md): configuration sources and .NET's options model.
- [Startup](startup.md): services that initialize asynchronously (in Rust or TS), the services the host requires and the
  checks before the start, ready hooks, and starting the app from TSX or Rust.
- [App architecture](app-architecture.md): dumb views, view models and services under contracts, their names, the
  composition root `configure` and the design registrations of `configurePreviews`, and how an app adopts them.
- [View models](view-models.md): activation, and binding dXn.Reactive state to Rust components and TSX.
- [Navigation](navigation.md): navigation regions as NSH navigation stacks.
- [TSX](tsx.md): a TSX app's crate (RayX's attribute), its builder and provider, RayX's service classes, services written
  in TS, contracts (shapes, threads and costs), implementing a RayX service in TS, options and app extensions, each
  beside its Rust twin.
- [Components and theme](components.md): RayX's component library, the set `rayx/components` every app serves, its
  layout components (`AutoLayout`, Figma's auto layout, and `Slot`, a component's place for content), the roles it
  reads, its default theme from the Figma tokens, another theme or a patch over RayX's, its meta
  files, and the gallery app.
- [Meta files](component-meta.md): a component's states declared in a `*.meta.tsx` file (`ComponentMeta<C>`, from
  `rayx/components`), for design tools and galleries: the size of a component with none of its own, design services
  (`configurePreviews` and a state's `services`), the previews of its private components, and the previews a library
  (RayX's own) ships.
- [Dev tools](devtools.md): the in-app agent of dev builds, how it connects to a tool, and the agent protocol.

## Reference

- [App services](services.md): RayX's services with their crates and features, how a service is registered,
  configured and replaced (in Rust or TS), and how to write one. Each service's page has its contract, builder methods
  in Rust and TS, what it requires, and how to replace it.
  - [I18n](services/i18n.md), [Operations](services/operations.md), [History](services/history.md),
    [Storage](services/storage.md), [Navigation](services/navigation.md), [Selection](services/selection.md),
    [Input map](services/input-map.md), [Platform](services/platform.md), [Suspension](services/suspension.md),
    [Application configuration](services/application-configuration.md), [Assets](services/assets.md),
    [Fonts](services/fonts.md), [Theme](services/theme.md)
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
