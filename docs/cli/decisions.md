# The CLI's decisions

The durable decisions behind `rayx` (`crates/rayx_cli`), folded in from RayX.CLI's AgentX memory
when the CLI moved into this repository (2026-11-02). The design they come from, with its decision
log D1 to D15, is [architecture/rayx-nsh-proposal.md](architecture/rayx-nsh-proposal.md); where a
decision changed with the move, it says so. Current source and tests remain the first evidence.

## What changed with the move

- **Repository and version (D4).** `rayx` is no longer a CLI of its own in `rayx-ui/RayX.CLI`
  starting at 1.0.0: it ships with RayX from `rayx-ui/RayX`, on RayX's 0.x version, under RayX's
  release policy ([docs/releasing.md](../releasing.md)). Every release `vX.Y.Z` from 0.6.0 on has a
  `rayx` X.Y.Z; the 1.0.0 floors (`OLDEST_RELEASE`, `MIN_REFRESH_RELEASE`) are gone.
- **Above NSH (D14).** `rayx new` scaffolds RayX apps: it names `rayx` (at its own tag) and RayX's
  classes in the files it writes into the app. `rayx-cli` itself still links no RayX or NSH crate
  (`tests/library_guard.rs`, over `cargo tree -p rayx-cli`).
- **One NSH pin.** The default nsh is the fork tag and the crates of RayX's own
  `[patch.crates-io]` (`tests/nsh_pin_rayx.rs`), so a new app builds with the NSH its RayX release
  does. Which lock packages are NSH's follows nsh's own `NSH_CRATES`. A new app also patches the
  NSH crates nsh's generated workspace depends on (`DEFAULTS.nsh.generated`: hosts, runners,
  platforms, the test runner), which nsh otherwise takes from crates.io, where some are missing
  and the rest are upstream's (`tests/proof_app_rayx.rs`).
- **Dependencies.** The repository's rule holds: crates.io, or public `https://` git by tag
  (`tests/dependency_sources.rs`), where RayX.CLI pinned git by `rev`.
- **License.** `MIT OR Apache-2.0`, as the whole repository.

## host

- **One seam for the host.** `host::facts()` builds `HostFacts { os, arch, emulated, wsl, distro }`
  from a `Probe` (environment, files, `IsWow64Process2`, `sysctl.proc_translated`); tests use
  `FixtureProbe`. Every external program runs through `host::Runner` over a `CommandSpec`, in
  `Execute`, `Print` (the exact command line, `sudo` for non-root steps, an admin marker) or
  `Record` mode; `Runner::query` runs read-only probes. `host::path_env` edits the user `PATH` (the
  Windows registry, a marked profile block on Unix). So `--check`, doctor and tests are exact.
- `rayx` never reads, stores or forwards a password or token and never runs itself elevated:
  elevation is per command, through the operating system's prompt. `Print` mode has no side
  effects, `PATH` edits included.

## project

- **Pins come from the project or one defaults table.** `Pins::resolve` takes each pin from the
  project (`Cargo.lock`, `rust-toolchain.toml`, `[workspace.metadata.rayx]`) with source `project`,
  else from `DEFAULTS` in `project::pins` with source `default`. No other module hard-codes a
  version; nothing else selects a pin (no member name, no compile-time path).

## nsh

- **The pin, the store, the install (D1, D2, D7, D10, D11).** The pin is the one source of the
  project's NSH packages in `Cargo.lock`, else the `[nsh]` table of a `rayx.toml` beside `nsh.json`,
  else the default. The store keeps one entry per pin in `$RAYX_HOME/nsh/<id>` (else `~/.rayx/nsh`),
  `id` being the commit's 12 hex digits or `v<version>`; `receipt.json`, written last, makes an
  entry complete, and `<id>.lock` is held while installing or pruning. An install takes the
  release's `nsh-<platform>` archive and its `.sha256`, else a shallow checkout built with
  `cargo build --release --locked -p nsh-cli --target-dir target`; `nsh --version` must name the
  pinned version and commit.
- Never the `nsh` on `PATH`; nsh runs without `CARGO`, and a checkout builds without
  `RUSTUP_TOOLCHAIN`. An unreadable pin is an error, never the default; a dry run writes nothing,
  not even a lock. An entry without a receipt is never run.
- **Working on NSH.** A path dependency on an NSH checkout makes `rayx app` build that checkout's
  CLI and run it; `RAYX_NSH=<path>` runs that binary instead of the pin's, announced on every
  command. Neither writes to the store.

## app

- **`rayx app` passes nsh's grammar through (D12).** It reads only the command name, `--target`
  and `--browser`, runs the prerequisites (the base set, whose last step is the pinned nsh, plus the
  sets the command and target need), and launches the pin's CLI in the directory the pin was read
  in. An argument after the command is never rewritten, except adding `--browser <the test set's
  Chromium>` for `test --target web` when no browser is installed.
- **`rayx new` pins the app from its first commit (D5, D7).** It installs the default pin, runs its
  `nsh init`, makes the app a RayX app (unless `--nsh-only`), and pins it: `.cargo/config.toml` and
  `cargo generate-lockfile` for `mixed` and `rust`, `rayx.toml` for `ts`. It writes nothing outside
  the app except the store, and never replaces the user's `.cargo/config.toml` or `rayx.toml`.

## setup

- **One plan for setup, `--check` and doctor.** `setup::plan(env, sets)` expands the sets (`base`,
  `web`, `test`, `ios`, `android`; no GPU set, D15) into ordered steps, each with a probe and an
  install. A probe never installs; `--check` executes, installs, records and downloads nothing; app
  commands reuse the same steps for their prerequisites. Missing apt packages install in one
  batched `sudo apt-get install`.
- **Guests.** WSL distributions and Lima VMs go through one guest layer: `rayx` installs the Linux
  `rayx` of the chosen version with that release's installer, clones only the current checkout
  and runs `rayx setup <flags>` inside. `setup.json` records the sets and project roots of real
  runs, `guests.json` the guests; `setup --update` re-runs the recorded setups and refreshes every
  current guest, replacing one older than the Ubuntu pin side by side. Guests `rayx` did not record
  are never touched, and nothing is deleted except by `lima remove`, `wsl remove` and
  `wsl compact --clean`, always confirmed unless `--yes`.
- **Named Lima VMs and chosen releases.** `setup --lima --vm <name>` (default `rayx`) never adopts a
  VM it did not create; `--release X.Y.Z` installs exactly that published release (a headers-only
  probe first, downgrades confirmed, nothing remembered).
- **WSL scripts run with `--exec`** (`wsl -d <name> --exec bash -lc <script>`), so the login shell
  does not parse them.
- **Android.** Android Studio installs through its own wizard on Windows (`winget … --interactive`,
  unelevated); the NDK pin takes effect through `ANDROID_NDK_HOME`, which a user's own value always
  overrides.

## distribution

- **cargo-dist builds and publishes `rayx`; axoupdater updates it.** dist owns
  `.github/workflows/release.yml` (changed only through `dist generate` with the pinned dist
  version), the six targets, checksums, attestations and the shell and PowerShell installers into
  `$RAYX_HOME/bin` or `~/.rayx/bin`. `rayx self update` uses the install receipt for the app
  `rayx-cli` from `rayx-ui/RayX`; without a receipt it says how to update instead.
- **release-plz tags, dist releases.** release-plz keeps the release pull request and pushes the tag;
  dist builds the binaries on the release pull request and publishes the GitHub Release on the tag
  ([docs/releasing.md](../releasing.md)).

## repository

- **Docs are checked against the CLI.** `tests/readme_nsh.rs` checks every `rayx …` command line and
  flag row of the CLI's README against the clap surface, every command and flag against the README,
  the sets-per-target table against `app::prereqs::required_sets`, and the pins tables of the
  README and [projects.md](projects.md) against `DEFAULTS` and the keys `Pins::resolve` reads;
  `readme_versions` and `readme_guests` check the guest and release sections.
