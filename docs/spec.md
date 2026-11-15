# RuneMD v1: design

November 13, 2026. Questions, approach and design decided with Matteo in Waggle case
c_ed9ba7bf-6c2c-489c-ab4f-65258cabb203. EGM review (comprehension, zoom-out, design) applied; my own choices
are marked **[my choice]**.

Documents cited (all in `~/Desktop/GitHub/rune-md-spike`, to copy into the new repo under `docs/`):
- `DESIGN.md`: look, tokens, live signals;
- `docs/decisions/2026-11-13-runemd-main-screen.md`: record of the chosen screen;
- `design/mockups/2026-11-13-runemd-main/index.html`: mockup, `scheda` variant (earlier rounds in `index-r*.html`);
- `docs/spike/2026-11-13-rune-vs-md-light.md`: spike measurements; `docs/app/2026-11-13-reuse-research.md`: what
  Rune already has.

Rune sources: `~/Desktop/GitHub/_vendor/rune`, rev `384c2991`.

## Intent

Every day Matteo reads the Markdown files that agents write in his repos (specs, plans, reports, CLAUDE.md, memories).
In his words: "you mostly read files written by agents in the repos --> this. I really rarely
write, sometimes I edit."

RuneMD is a Mac app in Rune to **read** these files while agents write them, and to **edit** them now and then.
It becomes the default app for `.md` files. It must be as fast and as beautiful as possible. Speed is also the
subject of a post (Rune against Swift, `docs/spike/post/card.html`: first frame 13× faster on 1 MB).

## Goals

1. Double-click on a `.md` in the Finder, "Open With" (several files together too) or `open FILE.md` from the terminal open the
   file in RuneMD, both with the app closed and with the app open.
2. The open file updates by itself when an agent rewrites it, and what you are reading does not move.
3. You can see which file an agent is writing and whether there is new text below the visible part (live signals).
4. The sidebar shows the recent Markdown files of all repos and the tree of the open file's repo.
5. Cmd+E switches from reading to the source of the whole file, with colors; you save and go back to reading.
6. Features Matteo chose for v1: find in file (Cmd+F), quick open (Cmd+P), heading outline, font
   choice, tabs, what changed, Mermaid diagrams, select and copy all, new file (Cmd+N; added in the
   spec review).
7. Light and dark theme, look as in `DESIGN.md`.

## Non-goals

- Rich editor, side-by-side preview, editing inside the view (Matteo chose the single Cmd+E key).
- Sync, cloud, plugins, PDF export, Windows and Linux.
- Distribution: notarization, App Store, website. v1 is a local `.app`.

## Anti-goals (it fails even if it works)

- The text jumps while an agent writes.
- The look becomes "AI slop": status pills, neon glows, decorative stats, random accents (Matteo's
  comments in the screen record).
- It is slower than md-light on the spike measurements.

## Constraints

- Stack: Rune + rune-kit + rune-fx at rev `384c2991` of the `dev` branch, with the spike's four `[patch.crates-io]`
  (`Cargo.toml`). Any Rune update can break something: update only with a written reason.
- New repo `~/Desktop/GitHub/runemd`, local only, no push (proposed in the "approaches" round, no objection).
  From the spike we bring `src/blocks.rs`, the base of `src/main.rs`, `fixtures/`, `bench/` and the documents cited above.
- macOS 26+ (Liquid Glass via `El::system_material`, transparent window, `glass` tint at 60 % on top).
- Inter goes in the bundle (macOS does not have it); `AppSpec.families = ("Inter", "ui-monospace, Menlo")`.

## Approaches

- **Rune with rune-kit (chosen).** The spike showed a first frame 13× faster than AppKit on 1 MB and equal
  scrolling. Almost everything exists: `Markdown` (with `rune-highlights` and the `mermaid` feature), `FileTree`, `SplitView`,
  `TabStrip`, `markdown::find`, `combobox` (fuzzy), `rune::prefs`, the `rune-fx` effects.
- Swift/AppKit: long selection and `odoc` for free, but it loses the speed that is the reason for the app and the post.
- Web (Tauri or Electron): slower and heavier than both.

For editing Matteo chose **"One key: reading or source"**, not "Source and preview side by side" nor
"Edit inside the view".

## What Matteo decided

### Main screen ("Live tab")

Details in `DESIGN.md` and in the record. Glass sidebar with "Open file ⌘P", "Recent" and "File repo" (repo
row with name and branch, files indented with guide lines). Solid panel 8 px from the edges, glass on all four
sides. Tabs at the top, "⌘E source" on the right, with the ↔ text width button to its left (popover with a slider, ⌘⌥+ and ⌘⌥−), 680 px column, "In this file" outline on the right.

### Live signals, one per meaning

| Signal | Meaning |
|---|---|
| Liquid metal ring on 4 sides around a tab, a Recent row, a tree row | An agent is writing this file |
| Fire from the bottom of the panel, always on and low; high with "N new paragraphs below" | High: there is new text below the visible part |
| Dither pulsing in place of a block | The block is still arriving |

### Where files come from

"Both": the link opens the file; the sidebar shows the recent files of all repos and the tree of the open file's
repo. The watched folders are `~/Desktop/GitHub` (28 repos today), plus others added in the settings.

### Name

"RuneMD" ("it says right away what it is and what it is made with. Useful for the post").

## Behavior

### Opening a file

- `Info.plist`: `CFBundleDocumentTypes` for `net.daringfireball.markdown` (`.md`, `.markdown`), Editor role.
  `make_app.sh` registers the bundle with `lsregister`. RuneMD becomes the default once, by hand, from "Get
  Info" in the Finder; the README explains it.
- **`odoc` [my choice, after the review].** Rune's GURL handler (`crates/rune/src/open_url/incoming.rs`) is
  `pub(crate)` and is installed before NSApplication starts; for `odoc` AppKit would install its own on top.
  RuneMD, in `main` before `App::run`, observes `NSApplicationWillFinishLaunchingNotification` and there adds
  `application:openURLs:` to the delegate class, as `crates/rune/src/platform/reopen.rs:91-114` does. The paths
  go into a static queue until the window exists, then reach the app with `AppHandle::update`. If it does not work after
  one day: a small patch to Rune with `App::on_open_file` next to `on_open_url` (`app.rs:496`).
- A file already open in a tab is brought to the front, not duplicated.
- From the terminal: `open -a RuneMD FILE.md`, or `runemd FILE.md` (link in the PATH to the bundle binary) that uses
  `rune::single_instance` (`open_url/instance.rs`) to pass the file to the instance already open.
- A path clicked in T3 Code: to check whether it goes through Launch Services; if not, the limit is documented.

### Stable reading

- The file is split into top-level blocks (the spike's `blocks.rs`) and shown in `virtual_rows`.
- **Stable block ids [my choice].** Each block has a persistent id, not its index (the spike uses
  `format!("b{i}")`, `src/main.rs:101`). On each new version an LCS on the block hashes (the `similar` crate or
  our own code; `rune_kit::diff` is a patch view, not useful here) assigns the ids: an identical block keeps its
  id; a block changed in place keeps the old one's id; repeated identical blocks (`---`) are told apart with
  a counter. `virtual_rows_version` follows the file revision.
- **Reading anchor.** `virtual_rows` keeps the top row still by key
  (`crates/rune/src/el/virtual_rows.rs:50-53`). If the top block changes height, the offset is corrected inside the
  block; if it disappears, the anchor moves to the next block that still exists.
- **Long words.** Long paths and names in headings, paragraphs, lists and cells wrap after `/ - _ .`
  (between two letters or digits); anywhere only if the stretch without these marks is wider than a line. Code
  blocks, HTML, bare URLs and link destinations stay as they are. Copy, outline and Find read the real
  source (`doc::breaks`, fixture `fixtures/long-title.md`).
- Acceptance test: with a script that inserts text above the view, lengthens the top block and
  lengthens the last block, the pixel at the top of the view does not move.

### Watching files

- **One event service [my choice].** One recursive FSEvents stream (the `notify` 8.2 crate, like
  `crates/rune-luau/src/watch.rs`, with a `Waker` to wake the UI) on the watched folders. From it come
  the reload of the open file, the per-tab "writing" state, Recent and the tree, and the index for Recent and Cmd+P.
  One more stream only for roots outside the watched folders (files opened elsewhere and their repo).
- We watch the folder, not the file: an agent that writes to a temporary file and then renames it does not
  lose the file.
- Filter: only `.md` and `.markdown`, skipping paths with `.git`, `node_modules`, `target`. **No
  `.gitignore` filter** [my choice]: agent files (`.claude/`, local plans) are often ignored by git, and they are
  exactly the ones to see.
- Reread with debounce (hypothesis: 50 ms). An empty read, or one much shorter than the previous one while the file
  is changing, is discarded and read again after the debounce.
- **Writes that are not from an agent.** RuneMD remembers the hash of what it saves and ignores the matching event. A
  change to `.git/HEAD`, `.git/ORIG_HEAD` or `.git/FETCH_HEAD` in the same repo in the last 2 s does not light
  the ring [my choice]: checkout writes `HEAD`, pull and `reset --hard` only move refs but write `ORIG_HEAD`
  and `FETCH_HEAD`; in a worktree we look at the `HEAD` in `.git/worktrees/<name>/` (`watch::git_head_root`).
- First task of the plan: record with `fs_usage` how Claude Code and Codex really write a long spec
  (full rewrite, rename, append) and tune debounce and thresholds on that data.

### Live signals: when they light up

- **Ring ("writing")**: the file was changed by an external write in the last N seconds (hypothesis: 3 s).
- **Fire** (v1.1: always on in reading mode, low, at rest): it rises when there are new or changed blocks
  below the visible part, and goes back to rest when the last of these enters the visible part. The setting
  "Fire at the bottom of the panel" turns it off completely (only the label with the new text stays). Below it, intensity, height, speed and color
  (Fire, Blue, Violet, Green, System accent) are saved in prefs.json; an invalid value goes back to the default, and the
  defaults are the earlier fire. Speed does not change the wakeups at rest. At rest it
  redraws 12 times a second, without a display link, so the CPU stays low. The label counts these blocks and stays "N new paragraphs below" (approved text).
  With the window inactive the animation stops and the fire stays still and visible [my choice].
- **Dither ("arriving")**: only for the last block of the file, when it is open (``` fence not closed, or
  the file does not end with a blank line after the block) and the file changed in the last N seconds. Otherwise the
  text is shown. The bar is one line tall and sits at the bottom of the document, so it moves nothing above.
  Consequence: if agents rewrite the whole file in one go, the dither is rarely seen (see "Questions for
  Matteo").
- **What changed**: new or changed blocks have a thin gray mark on the left for a few seconds
  (hypothesis: 8 s).
- With "reduce motion" the ring and the dither stay still and visible, the fire is still and changes height without rising.

### GPU effects (rune-fx) [my choice, after the review]

rune-fx is not a method of `El`: it needs one `App::compositor` (`crates/rune/src/compose.rs`, `app.rs:554`) that
draws every frame with `FxRenderer`, plus `El::effect(mark)` on the elements.

- One mark for each place that can have an effect (tab, Recent row, tree row, bottom of the panel, arriving
  block). `animating()` is false with no active agents; the fire at rest (v1.1) redraws with the 12 fps wakeups.
- The ring is `LiquidMetal` as a border: `.border(w)` paints inside the shape (`rune-fx/README.md:108`), so the
  mark is 1.5 px larger than the row on each side, or `effect_bleed` is used.
- Fire in the light theme: same coverage as the dark theme, the text stays under the flames (decision `docs/decisions/2026-11-14-light-theme-fire.md`).
- Pipelines compiled ahead of time at startup outside the first frame (20–150 ms each with a cold cache, README:170).
- **Plan task 1**: the spike with the compositor attached and no active effect must stay within the startup
  and scroll thresholds. If not, stop and decide before building the rest.

### Sidebar

- **Startup**: the first frame shows only the document. A background scan (mtime of the `.md` files in the watched
  folders) fills Recent and the Cmd+P index; the index is saved in Application Support and updated from the
  events.
- **Recent**: the recently changed `.md` files, newest at the top, relative time, at most 20 rows (hypothesis).
- **File repo**: root = the nearest folder with `.git` above the file (outside a repo: the file's
  folder); name and branch from `.git/HEAD`. `FileTree` with `compact_folders(false)` and folders open down to the open
  file; the paths come from the index (only folders with `.md` files).
- **Permissions**: `~/Desktop` is protected (TCC). The `.app` is always signed with the same local certificate, so the
  permission survives every build. If access is denied, the sidebar says so ("No access to Desktop") with the
  "Open System Settings" link to the system settings.

### Tabs

- rune-kit `TabStrip`, always `status(None)` (its dots and spinners are forbidden by `DESIGN.md`); the ring goes
  with `TabItem::decorate`.
- Cmd+W closes, and so do the × on the front or hovered tab (v1.1) and a middle click; Cmd+1…9 and Ctrl+Tab switch tabs [my choice, Mac convention].
- Preview tab (after v1, like VS Code): a single click on a file in the sidebar (Recent, tree,
  ⌘P results, Enter in ⌘P too) opens it in one preview tab, name in italics, which the next single
  click replaces in the same place. It becomes pinned with a double click on the row or on the tab, an edit in the
  source or ⌘S. From the Finder, the terminal, ⌘N and recovery copies the file opens pinned. A file already open: its tab
  is brought to the front.

### Editing (Cmd+E)

- Cmd+E shows the source of the whole file in a multiline `text_input` with Markdown colors (rune-highlights).
  Cmd+S saves; Cmd+E again saves (only if there are changes) and goes back to reading.
- On entry RuneMD remembers the file hash. Before each save it compares it with the disk: if it changed,
  it does not save and shows a row "The file changed on disk" with "Reload" and "Keep mine". Never overwrite
  silently. While editing, the view does not reload by itself.
- **Threshold**: typing on `fixtures/big.md`, CPU per frame p95 ≤ 8 ms (hypothesis). `text_input` does a single layout
  for the whole text and clones it on every view (`crates/rune/src/el/input.rs:57-63`); the spike only measured a
  6 KB paragraph.
- **Plan B if the threshold fails** [my choice]: virtualized source, one `text_input` per block in
  `virtual_rows`, colors only on mounted rows, blocks joined again on save. There is still one source view.

### Other v1 features

- **Find (Cmd+F)**: `markdown::find`, Enter and Shift+Enter through the results.
- **Quick open (Cmd+P)**: fuzzy matching from `rune-kit/src/combobox.rs` on the index names.
- **YAML frontmatter** (v1.1): at byte 0, closed by `---` or `...`, it is a quiet metadata block in reading mode,
  outside the outline; the source does not change (DESIGN.md, "Frontmatter").
- **Heading outline**: H1–H3 on the right; the heading of the visible section is highlighted; a click scrolls.
- **Font**: body family and size. Inter, New York, SF Pro first; then the list of installed fonts via
  CoreText (Rune does not expose it).
- **Mermaid**: rune-kit `mermaid` feature.
- **Select and copy all**: at the pinned rev Rune already has selection on unbuilt virtual rows and
  Cmd+A/Cmd+C with row rebuilding (`crates/rune/src/runtime/virtual/selection_collect.rs`, cap of 10,000
  rows and 4 MiB). First step: a `--script` with Cmd+A and Cmd+C on `big.md` that compares the clipboard with
  the file. If the copied text is not the Markdown source, `selection_copy` on the group maps the selected blocks
  to the stable ids and copies their source. Above 4 MiB Cmd+A copies the whole file from disk [my choice].

- **New file (Cmd+N)** [my choice on the how]: a field like the Cmd+P one asks for the name; the file is created
  empty in the open file's folder (with no open file: the first watched folder), and opens in a new tab
  already in source (Cmd+E). `.md` is added if missing; an existing name opens that file, it does not overwrite it.

### Settings

Watched folders (add, remove), font, size, theme (system, light, dark), saved with `rune::prefs`.
They open with Cmd+, in a panel in the main window [my choice: it avoids a second window].

## Performance (v1 thresholds)

Same machine and same files as the spike (`fixtures/normal.md`, `fixtures/big.md`), with compositor, sidebar and
watcher on:

- warm startup to the first frame with content ≤ 350 ms on normal and on big (spike: 302 and 295 ms);
- sidebar filled ≤ 1 s after the first frame with the saved index (hypothesis);
- scroll: dropped refreshes ≤ 3 % (spike: 2.4 and 2.6 %);
- idle memory on big with the index loaded ≤ 100 MB (spike: 78 MB);
- from the agent's write to the new frame ≤ 150 ms, FSEvents latency and debounce included (hypothesis);
- editing: p95 ≤ 8 ms per frame on big (hypothesis).

They are measured again at the end of each phase.

## Work order [my choice]

1. **Foundations and risks**: repo, signed bundle, measured rune-fx compositor, `odoc`, stable ids and anchor,
   recording of how agents write, Cmd+A/Cmd+C test.
2. **Live reading**: single watcher, reload, the three signals, what changed.
3. **Sidebar**: index, Recent, tree, Cmd+P, permissions.
4. **Tabs and editing**: `TabStrip`, Cmd+E, conflicts, measurement and plan B, new file.
5. **The rest**: find, heading outline, font, Mermaid, settings, light theme, final threshold check.

## Matteo's answers (spec review, 13/11)

1. **Rare dither: keep it that way.** Only on the last open block. Matteo: "I don't know how often it will happen, maybe
   if it happens it's fine for it to be shown the way we set it up".
2. **Signing: yes, local self-signed certificate** in the Keychain, only for RuneMD.
3. **"New file" goes into v1** (Matteo: "of the part you left out I would move "new file" to "in"").

## What is left to the builder

- Modules, types, how the state is split.
- The values for debounce, "writing", "what changed" and the rune-fx parameters, starting from the hypotheses above and
  from the task 1 recording.
- Bundle and icon (model: the spike's `bench/make_app.sh` and the Rune app `anomalyco/clip` on GitHub).
- The design skills to use: `skills-eval.md`.
