# Contributing

## Requirements

- macOS 26 or later.
- The Rust toolchain. [rustup](https://rustup.rs) installs the version in `rust-toolchain.toml` on the
  first build.

## Build and run

The first time, create the local signing certificate "RuneMD Local":

```sh
scripts/make_cert.sh
```

It goes into its own keychain (`~/Library/Keychains/runemd.keychain-db`), because in the login keychain
`codesign` waits for a dialog and blocks.

Then build the app and open a file:

```sh
make app    # builds build/RuneMD.app, signed "RuneMD Local"
open build/RuneMD.app --args file.md
```

Command line: `ln -s "$PWD/scripts/runemd" ~/.local/bin/runemd`, then `runemd file.md`.

## Checks

```sh
make check    # clippy, tests, gitleaks
```

Run it before every commit. Commits follow [Conventional Commits](https://www.conventionalcommits.org)
(`feat(scope): subject`).

Measurements: `bench/startup.sh`, `bench/scroll.sh`, `bench/memory.sh` and `bench/edit_big.sh` (test
files in `fixtures/`); results and budgets in `docs/perf.md`. Headless checks: `bench/reduce_motion.sh`
(`RUNEMD_REDUCE_MOTION=1|0` overrides the system setting) and `bench/worst_case.sh` (sidebar with 28
repos and long names).

Debugging file opening: `RUNEMD_LOG_OPEN=1` prints the paths Launch Services delivers to stderr
(`open -a … --stderr file.log`); the handler is `src/open/odoc.rs`.

## Release package

```sh
make release    # build/RuneMD-<version>.zip (ad hoc signed) and build/RuneMD-<version>-notes.md
```

It does not tag or publish. The notes come from `assets/release-notes.md`. `make release` replaces
`build/RuneMD.app` with the ad hoc build: run `make app` afterwards to get the local one back.

## Design

The look is described in `DESIGN.md`; the spec is `docs/spec.md`.
