# rune, vendored

Source: github.com/anomalyco/rune, `crates/rune` at rev `384c2991e582418ba529835279c5876ac3915e02`
(the rev every other Rune crate in `Cargo.toml` uses). License: MIT, `LICENSE` here is Rune's.
Wired in with `[patch."https://github.com/anomalyco/rune"] rune = { path = "vendor/rune" }`, so
rune-kit, rune-fx and every Rune git crate build against this copy.

Why: Rune colours each glyph by the byte its quad carries (text highlight `fg`, link hover, Find).
The x -> byte table that maps glyphs to bytes walked only glyph clusters and skipped inline boxes,
so after an inline icon (`El::inline_icon`, a pill's padding) every glyph took the byte of a cluster
about the icon's width to its right: a path chip `below.rs L24` drew `below` bright, `.rs L2` grey.
The icon is a parley inline box made inside Rune, so rune-kit cannot fix this on its side (a
placeholder character would move every byte range and show up in copy and Find). Nothing is
pushed upstream.

Changes from the rev:
- `Cargo.toml`: workspace fields written out; `procwatch`, `rune-motion`, `rune-staging`,
  `rune-taffy` and `tiny-skia-path` from the same git rev; no examples, tests, README,
  dev-dependencies or workspace lints (the `#[cfg(test)]` modules are kept but never built here).
- `src/text/glyph_emit.rs`: `cluster_table`, the table built from `line.items()`, adding each
  inline box's width to `x`; `glyphs_in_band` uses it.
- `src/text/glyph_cache.rs`: `Entry::flatten` uses `cluster_table` in place of its own copy.
- `src/el/pills.rs`: `InlineIcon::advance`, the width an inline icon takes (size + gap), so
  rune-kit's tables count a path's icon in its column's width (review R1,
  docs/decisions/2026-11-23-t3-paths-review.md).

Test: `tests/inline_icon_glyph_bytes.rs` (fails on the rev, passes here).

To drop the vendor: once Rune counts inline boxes in that table, remove the `[patch]` and this folder.
