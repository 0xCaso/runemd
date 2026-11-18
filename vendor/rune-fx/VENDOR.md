# rune-fx, vendored

Source: github.com/anomalyco/rune, `crates/rune-fx` at rev `384c2991e582418ba529835279c5876ac3915e02`
(the rev every other Rune crate in `Cargo.toml` uses). License: MIT, `LICENSE` here is Rune's.

Why: the chrome headings need a fill clipped to a glyph mask, which rune-fx at this rev does not have
(docs/decisions/2026-11-18-full-forge-round-2.md, constraint 2). Nothing is pushed upstream.

Changes from the rev:
- `Cargo.toml`: workspace fields written out; `rune-motion` and `rune-staging` from the same git rev;
  no dev-dependencies (examples and tests are not vendored).
- `src/pipes.rs`: includes `src/shaders/rune_clip.wgsl`, a copy of `crates/rune/src/clip.wgsl`.
- `FillParams::masked` (see the commit that adds it): the fill's coverage times the alpha of the
  layer's source texture (`FxLayer::source`).

To drop the vendor: once Rune has an equivalent, point `rune-fx` back at the git rev.
