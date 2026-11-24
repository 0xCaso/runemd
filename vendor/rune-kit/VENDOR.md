# rune-kit, vendored

Source: github.com/anomalyco/rune, `crates/rune-kit` at rev `384c2991e582418ba529835279c5876ac3915e02`
(the rev every other Rune crate in `Cargo.toml` uses). License: MIT, `LICENSE` here is Rune's.

Why: T3 Code-style paths (docs/decisions/2026-11-23-t3-style-paths.md) need a type icon before a
path link and a wrap toggle on code blocks; the kit at this rev draws link icons only for web pages
and has no per-block wrap. Nothing is pushed upstream.

Changes from the rev:
- `Cargo.toml`: workspace fields written out; Rune crates from the same git rev; no examples,
  tests or dev-dependencies (the `#[cfg(test)]` modules are kept but never built here).
- `Markdown::link_icon` (`markdown/link_icon.rs`): the inline icon before a link, by destination,
  any scheme; it replaces `MarkdownStyle::link_icons`' globe for the links it answers.
- `Markdown::code_wrap` (`markdown.rs`, `markdown/render.rs`): code block `index` wraps its lines,
  or scrolls sideways (`Renderer::code_wrapped`).
- `markdown/table.rs`, `markdown/link_icon.rs` (`Renderer::link_icons_advance`): a table counts
  the icons before its cells' links in its columns' widths (review R1, record
  2026-11-23-t3-paths-review); without it a cell with a path icon wrapped its last word.
- `MarkdownStyle::table_open_end` (`markdown/style.rs`, `markdown/table.rs`): no rule under the
  last row, for a table in a box the app draws (R1: a full-width rule doubled the box's bottom).
- `Markdown::code_header` (`markdown.rs`, `markdown/render.rs`): a row drawn on top of each code
  block, inside its card (language, wrap and copy buttons).
- `MarkdownStyle::code_look` (`markdown/style.rs`): fill, font, text colour and token colours of a
  code block, read back so the JSON viewer (decision 2026-11-23-json-viewer) draws its own code
  card with the same look.
- `MarkdownStyle::list_wrap` (`markdown/style.rs`, `markdown/render.rs`): a wider gap between the
  items of a list whose paragraphs wrap (feedback 2026-11-24 G6, record
  2026-11-24-label-tables-list-spacing); one-line lists keep `list`'s gap.
- HTML `<img>` and badge rows (`markdown/html_img.rs`, `markdown/parser.rs`, `markdown/image.rs`,
  `markdown/render.rs`, `Block::Image`'s `width`, `center`, `html`, `href`, `joined`,
  `Markdown::image_density`, `ImageFrame`'s `width`, `html`, `href`, `in_line`): a README's
  `<p align="center"><img width>` shows centred at its width; an image that is all a link holds is
  lifted with the link's destination; images with only spaces between them share a wrapping line
  (feedback 2026-11-24 I2, decision 2026-11-24-remote-images).

To drop the vendor: once Rune has equivalents, point `rune-kit` back at the git rev.
