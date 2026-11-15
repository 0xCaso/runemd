# What we can reuse for the Markdown app

November 13, 2026. I read the public repos of github.com/anomalyco and the Rune sources in
`~/Desktop/GitHub/_vendor/rune` (rev `384c2991`).

## What I understood you want

- A Mac app in Rune to **read and edit** Markdown files. It must be as beautiful and as fast as
  possible.
- You want it to become **the default app for `.md` files**.
- The style is the "Current" one: translucent frame and sidebar, black content in the dark theme and white
  in the light theme.
- Features still open: recent files, folder or repo tree, font choice, more.

## OpenCode

OpenCode is written in TypeScript (the TUI uses opentui), so its Markdown code cannot be used
in a Rust app. **But the best piece is already in Rune:** `rune-highlights` is the code highlighter
of the OpenCode desktop app (`@pierre/highlights`, 80 languages), rewritten in native Rust. We will have
the same colors as OpenCode in code blocks.

## What Rune already has (no need to write it from scratch)

| Need | Rune piece | Notes |
|---|---|---|
| Markdown rendering | `rune_kit::Markdown` | GFM tables, clickable task lists (`on_task`), links, images, diffs. Parsing cache. Already used in the spike. |
| Colored code | `rune-highlights` | The same as OpenCode desktop. |
| Diagrams | `rune-mermaid` | Native Mermaid (flowchart, sequence, gantt…), no web view. |
| Find in the document | `rune_kit::search` + `markdown::find` | Finds like Safari: case, accents, Unicode. |
| File tree | `rune_kit::Tree` + `FileTree` | Virtualized (50,000 nodes at the cost of 40), full keyboard. |
| Window structure | `rune_kit::shell` | `SplitView` with a resizable sidebar, `Toolbar`, `TabStrip` (tabs), `WindowTitlebar`. |
| Glass / translucency | `El::system_material` | Liquid Glass (`NSGlassEffectView`) on macOS 26+, vibrancy before. Examples: `frosted-bar`, `glass-panel`. |
| Diff between versions | `rune_kit::diff` | Useful for "what changed since I opened it". |
| Mac helpers | `rune-mac` | Remembers window size and position, full screen. |
| Example of a real app | `anomalyco/clip` | Published Rune app for macOS: it is the model for the bundle and icon. |

## What is missing and must be written

1. **Opening `.md` files from the Finder (default app).** Rune only receives links with its own scheme
   (`myapp://`, Apple Event `GURL`). It does not receive "open this document" (Apple Event `odoc`), which
   macOS uses for double-click and "Open With". An `odoc` handler must be added in our app (same
   method as `crates/rune/src/open_url/incoming.rs`), plus `CFBundleDocumentTypes` for `.md` in
   Info.plist. Estimate: half an agent day (estimate, no measured class).
2. **Selection and copy beyond the screen** (limit found in the spike): Cmd+A and copying long
   stretches.
3. **Editor.** Today `text_input` shows the raw source of one block. A real editor (source with
   colors on the left and preview on the right, or editing inside the view) is the biggest part and the one
   most open to decision.
4. Our own typographic style instead of `MarkdownStyle::neutral`.

## Risk

Rune must be pinned to a rev of the `dev` branch with four `[patch.crates-io]` patches (see the spike). Any
Rune update can break something.
