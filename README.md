# RuneMD

A native macOS Markdown reader for the files that coding agents write, live. While an agent edits a
plan, a spec or a README, RuneMD shows the change as it lands: it marks the file that is being written,
points to new text further down, and keeps your place in the document.

Written in Rust with [Rune](https://github.com/anomalyco/rune). It starts in under 350 ms, also with
1 MB files.

## Features

- Reading with virtualized scroll, tabs, a sidebar with Recent files and the folders of the file's repo,
  Quick Open (⌘P), Find in file (⌘F), an outline of the headings, and Mermaid diagrams.
- Live signals while an agent writes: a metal ring (the file is being written), fire (new text further
  down) and dither (a block is arriving). With macOS "Reduce motion" they stay still and visible.
- An editable source view (⌘E), with a save that checks for conflicts with the file on disk. If a save
  fails when you quit, the text goes into a recovery copy in
  `~/Library/Application Support/RuneMD/Recovered/` and opens again at the next start.
- Settings (⌘,): watched folders, font, size, theme, fire on or off.

## Requirements

- macOS 26 or later.
- The Rust toolchain. [rustup](https://rustup.rs) installs the version in `rust-toolchain.toml` on the
  first build.

## Build and run

The first time, create the local signing certificate "RuneMD Local":

    scripts/make_cert.sh

It goes into its own keychain (`~/Library/Keychains/runemd.keychain-db`), because in the login keychain
`codesign` waits for a dialog and blocks.

Then build the app and open a file:

    make app                                # builds build/RuneMD.app, signed "RuneMD Local"
    open build/RuneMD.app --args file.md

## Opening files

- Finder, "Open With" or `open -a RuneMD file.md`: Launch Services delivers the files with
  `application:openURLs:` (`src/open/odoc.rs`). Several files open in one tab each.
- Terminal: `ln -s "$PWD/scripts/runemd" ~/.local/bin/runemd`, then `runemd file.md`.
- To open all `.md` files with RuneMD: in Finder, "Get Info" on a `.md` file, "Open with" RuneMD,
  "Change All…".
- Debug: `RUNEMD_LOG_OPEN=1` prints the paths it receives to stderr (`open -a … --stderr file.log`).

## Development

    make check    # clippy, tests, gitleaks

Run it before every commit. Measurements: `bench/startup.sh`, `bench/scroll.sh`, `bench/memory.sh` and
`bench/edit_big.sh` (test files in `fixtures/`). Headless checks: `bench/reduce_motion.sh`
(`RUNEMD_REDUCE_MOTION=1|0` overrides the system setting) and `bench/worst_case.sh` (sidebar with 28
repos and long names).

## Status

Version 1 (plan in `docs/plan.md`, tasks 1–28) is complete, except for two items:

- scroll: dropped frames are above the 3 % limit (measured with a load average of 7–19; to measure again
  on an idle machine, `docs/perf.md`, section "v1");
- the meta line under the title in `DESIGN.md` is not there: after v1, only "last modified" and outside
  the document (`docs/decisions/2026-11-14-sidebar-tabs-long-names.md`, D6).

Spec in `docs/spec.md`, plan in `docs/plan.md`, design in `DESIGN.md`, decisions in `DECISIONS.md` and
`docs/decisions/`, measurements in `docs/perf.md`, videos of the flow in `docs/proof/`.

## License

[MIT](LICENSE)

Fonts: Inter (`assets/fonts/`, [SIL OFL 1.1](assets/fonts/OFL.txt)). The RuneMD wordmark on the
empty screen is made of Outfit 800 glyph outlines (`assets/wordmark/`, `scripts/make_wordmark.py`; no font file
ships), [SIL OFL 1.1](assets/fonts/OFL-Outfit.txt), Copyright 2021 The Outfit Project Authors.
