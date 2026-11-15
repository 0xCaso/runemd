# Audit: Markdown formatting in reading mode

Date: 2026-11-16 (work of 2026-11-15). Record: `docs/decisions/2026-11-15-work-order-after-v1.md`, group 3.
Matteo's request (scrawl `1794741347588`, 0:16–0:43): highlight paths and file names, fix the unformatted
"Tasks"-like sections, check formatting and highlighting in general. The headings that ran out of the
column are already fixed (f41cdf1).

## How it was done

Build `cargo build --profile perf` at 7ccaaff, headless shots like `bench/long_title.sh`
(`--size 1240x2200 --scale 1 --at 1.0 --snap … --crop main`, `RUNEMD_THEME=light|dark`), on:

| File | What | Shots (light / dark) |
|---|---|---|
| `docs/audit/2026-11-16-markdown-formatting/prova.md` | every item of the list | `formattazione-light.png` / `-dark.png` |
| `.superpowers/sdd/plan/progress.md`, lines 1–40 | the SDD log from the recording | `progress-*.png` |
| `docs/plan.md`, lines 50–80 | a `### Task N:` with `- [ ] **Step**` | `plan-task-*.png` |
| `README.md` | README | `README-*.png` |
| `docs/decisions/2026-11-15-work-order-after-v1.md` | a decision record | `decisione-*.png` |

Shots in `docs/audit/2026-11-16-markdown-formatting/`. The test file covers: task lists, nested lists,
inline code with paths (`src/ui/tabs.rs:42`), bare paths in prose, tables with a right-aligned column,
quotes, `> [!NOTE]` and `> [!WARNING]` callouts, headings 1–6, `rust` and `sh` code blocks, horizontal rule,
footnote, strikethrough, italics, long URL, lines with a single line break.

The Markdown is drawn by `rune_kit::Markdown` (rev 384c299). The kit's parser turns on only tables, strikethrough and task
lists (`crates/rune-kit/src/markdown/parser.rs:466`) and brings all headings from 4 down to level 4
(`parser.rs:503`). The style is in `src/ui/markdown_style.rs`.

## What comes out well

Headings 1–2, body, bold, italics, strikethrough, nested lists (bullets and numbers), tables (`---:` column on the
right, tabular figures), code blocks with syntax colors, horizontal rule, plain quote,
long paths that wrap (f41cdf1).

## What comes out badly (fixes with no choice)

"fix" mockup in `design/mockups/2026-11-16-formattazione-markdown/` (`shots/fix-light.png`, `shots/fix-dark.png`).

| # | Element | Today | After | Shot | Where it is fixed |
|---|---|---|---|---|---|
| F1 | Long bare URL | runs out of the column and ends up under the "In this file" outline | wraps inside the column, the link stays one whole link | `formattazione-*.png`, "Long link" | `doc::breaks::protected` leaves URLs intact so as not to break the autolink (decision of 2026-11-15); it needs a wrap that does not change the link text (break only the drawn text, or `wrap_anywhere` on the link stretch only) |
| F2 | Callouts `> [!NOTE]`, `[!TIP]`, `[!IMPORTANT]`, `[!WARNING]`, `[!CAUTION]` | quote with "[!NOTE]" written out | quote with a label in words and an icon (Note, Tip, Important, Warning, Caution), 13/600 in `fg`, icon `fg-2`, no color | `formattazione-*.png` | the kit does not recognize them: in RuneMD before drawing, like `doc::breaks` (displayed text), or a kit `quote_frame` |
| F3 | Footnotes `[^1]` | "[^1]" and "[^1]: …" written as text | raised number in the text, notes at the end of the file below a line, 13.5 in `fg-2` | `formattazione-*.png` | the kit does not turn on `ENABLE_FOOTNOTES`: in RuneMD on the displayed text |
| F4 | Headings 4, 5, 6 | same as 3 (15.5/640); 3 weighs as much as a bold line (`**Files:**` in the plans) | 3 at 17/640 `fg`, 4 at 15.5/600 `fg`, 5 at 14/600 `fg-2`, 6 at 13/500 `fg-3` | `formattazione-*.png`, `plan-task-*.png` | the kit brings 4–6 to a single level (`parser.rs:503`): 5 and 6 need the real level; 3 is `markdown_style::fonts` |
| F5 | Task list checkbox | the kit's `border_weak` border: almost invisible in dark, under 3:1 | 1 border in `fg-3`, checked filled `fg-2` with a `panel` tick, like the Settings checkbox (7.88 / 7.34 and 11.19 / 10.71, "Contrast" table in DESIGN.md) | `plan-task-dark.png`, `formattazione-*.png` | `MarkdownStyle::task_checkbox` in `markdown_style::style_with` |

DESIGN.md must be updated with F2–F5 (callouts, notes, heading scale 3–6, checkbox) when they are done.

## Paths and file names (visual choice a)

Today a path is code only if the agent put it between backticks; bare, it stays normal text, and inside the
pill the file name weighs as much as the folders (`formattazione-*.png`, first line; `progress-*.png`, "Setup").
Variants in the mockup: `p-oggi`, `p-codice`, `p-nome`, `p-link` (stretch).

What counts as a path (for all variants): a word with at least one `/` and an extension or a trailing `/`
(`src/main.rs`, `design/mockups/`, `~/Desktop/x.md`), or a known file name without a folder with a common extension
(`Cargo.toml`, `README.md`), with an optional `:line`. It does not count inside code blocks, links and URLs.

## "Tasks" sections (visual choice b)

Matteo's files have two forms:

1. **SDD log** (`.superpowers/sdd/plan/progress.md`, the one from the recording, shot `frame-06`): under
   `## Tasks` each line is an entry (`Task 1: BASE…`, `Ruling: …`, `Task 1: minor (deferred): …`) separated by
   **a single line break**. For CommonMark a single line break is a space: all the lines become one paragraph,
   the wall of text Matteo circled. Cause: the kit treats the soft break as a space (standard behavior).
2. **Superpowers plans** (`docs/plan.md`): `### Task N: title`, then `**Files:**`, lists, `- [ ] **Step N:** …`.
   Here the structure is there, but heading 3 is 15.5/640 like `**Files:**` (F4) and the checkboxes are barely visible (F5):
   the tasks do not stand apart at a glance.

Variants in the mockup: `t-oggi`, `t-righe`, `t-sezioni`, `t-registro` (stretch). All the `t-` variants already include
F4 and F5.

## Mockup

`design/mockups/2026-11-16-formattazione-markdown/index.html`, light and dark shots in `shots/`.
