---
name: RuneMD
description: A Markdown reader for the Mac, in Rune, for the files that agents write in repos.
colors:
  panel-light: "oklch(0.995 0.001 270)"
  panel-dark: "oklch(0.155 0.004 270)"
  fg-light: "oklch(0.22 0.006 270)"
  fg-dark: "oklch(0.95 0.003 270)"
  fg2-light: "oklch(0.36 0.008 270)"
  fg2-dark: "oklch(0.82 0.006 270)"
  fg3-light: "oklch(0.45 0.008 270)"
  fg3-dark: "oklch(0.72 0.008 270)"
  fg2-glass-light: "oklch(0.25 0.008 270)"
  fg2-glass-dark: "oklch(0.90 0.006 270)"
  fg3-glass-light: "oklch(0.28 0.008 270)"
  fg3-glass-dark: "oklch(0.86 0.008 270)"
  body-light: "oklch(0.28 0.006 270)"
  body-dark: "oklch(0.90 0.004 270)"
  glass-light: "oklch(0.965 0.004 260 / 0.60)"
  glass-dark: "oklch(0.18 0.01 260 / 0.60)"
  line-light: "oklch(0 0 0 / 0.09)"
  line-dark: "oklch(1 0 0 / 0.14)"
  sel-light: "oklch(0 0 0 / 0.06)"
  sel-dark: "oklch(1 0 0 / 0.12)"
  field-light: "oklch(1 0 0 / 0.55)"
  field-dark: "oklch(0 0 0 / 0.22)"
  mark-light: "oklch(0.45 0.008 270 / 0.70)"
  mark-dark: "oklch(0.72 0.008 270 / 0.65)"
typography:
  doc-title:
    fontFamily: "Inter"
    fontSize: "28px"
    fontWeight: 680
    lineHeight: 1.2
    letterSpacing: "-0.015em"
  doc-heading:
    fontFamily: "Inter"
    fontSize: "19px"
    fontWeight: 640
    lineHeight: 1.3
    letterSpacing: "-0.005em"
  doc-body:
    fontFamily: "Inter"
    fontSize: "15.5px"
    fontWeight: 400
    lineHeight: 1.62
  doc-table:
    fontFamily: "Inter"
    fontSize: "14px"
    fontWeight: 400
    fontFeature: "tnum"
  doc-code:
    fontFamily: "ui-monospace, SF Mono, Menlo"
    fontSize: "13.5px"
  ui:
    fontFamily: "Inter"
    fontSize: "13px"
    fontWeight: 400
  outline:
    fontFamily: "Inter"
    fontSize: "12.5px"
    fontWeight: 400
    lineHeight: 2
  ui-small:
    fontFamily: "Inter"
    fontSize: "12px"
    fontWeight: 500
  kbd:
    fontFamily: "Inter"
    fontSize: "11px"
    fontWeight: 400
rounded:
  kbd: "4px"
  row: "6px"
  field: "7px"
  panel: "10px"
spacing:
  frame: "8px"
  sidebar: "248px"
  column: "680px"
  row: "28px"
  tree-row: "26px"
  tree-indent: "16px"
  sidebar-top: "44px"
  sidebar-pad: "8px"
  sidebar-bottom: "10px"
  field: "30px"
  field-margin: "0 2px 6px"
  row-pad: "10px"
  tree-row-pad: "8px"
  row-gap: "8px"
  qualifier-gap: "6px"
  icon: "14px"
  chevron: "10px"
  section-pad: "14px 10px 6px"
  kbd-pad: "1px 5px"
  hairline: "0.5px"
  tab-bar: "44px"
  tab-bar-pad: "0 10px"
  tab-bar-lead: "8px"
  tab-top: "8px"
  tab-pad: "12px"
  tab-divider: "1px"
  tab-max: "240px"
  recent-gap: "3px"
  recent-row: "44px"
  recent-share: "40%"
  find-field: "240px"
  find-ring: "1px"
  find-ring-active: "2px"
  outline: "196px"
  outline-pad: "31px 10px 24px"
  outline-row: "25px"
  outline-row-pad: "0 6px"
  outline-indent: "10px"
  outline-label-gap: "4px"
  outline-min-window: "1128px"
motion:
  press: "scale 0.98, 160ms ease-out"
  hover: "120ms ease"
  ease-out: "cubic-bezier(0.23, 1, 0.32, 1)"
  mode: "reading ↔ source: crossfade, critically damped spring 0.22 s in (95 % at ~165 ms), 0.17 s out (< 5 % at ~130 ms)"
components:
  sidebar-row:
    height: "{spacing.row}"
    rounded: "{rounded.row}"
    padding: "0 10px"
    typography: "{typography.ui}"
  sidebar-row-selected:
    backgroundColor: "{colors.sel-dark}"
  open-file-field:
    backgroundColor: "{colors.field-dark}"
    textColor: "{colors.fg2-dark}"
    rounded: "{rounded.field}"
    height: "30px"
  doc-panel:
    backgroundColor: "{colors.panel-dark}"
    rounded: "{rounded.panel}"
---

# Design System: RuneMD

## Overview

**Creative North Star: "The still page"**

RuneMD is for reading the Markdown files that agents write, while they write them. The text stays still in a
solid panel (black in the dark theme, white in the light one). Everything else, that is the sidebar, the tabs and
the frame, is translucent glass around the panel, as in Current. The app is calm: no accent color, no status pills,
no statistics. The only elements that move are three Rune GPU effects, and each one says one thing only.

Reference screen: the `scheda` variant in `design/mockups/2026-11-13-runemd-main/index.html`
(decision: `docs/decisions/2026-11-13-runemd-main-screen.md`).

**Key Characteristics:**
- Two materials: glass (frame, sidebar, tab bar) and a solid panel (the document).
- Neutral grays with a touch of blue (hue 260–270); no accent. The one exception: the icons of paths and links take
  their file type's color (see "Path icon colors").
- Three live signals, each with one meaning only: metal, fire, caret.
- Mac app density: 28 px rows, 13 px UI text; the document breathes (15.5 px, line height 1.62).

## Colors

A scale of OKLCH grays with very low chroma, in two themes. In Rune they are written the same way with
`oklcha(l, c, h, a)`.

| Role | Light | Dark | Use |
|---|---|---|---|
| `panel` | 0.995 | 0.155 | Document panel |
| `fg` | 0.22 | 0.95 | Headings, file names, strong text |
| `body` | 0.28 | 0.90 | Document body |
| `fg-2` | 0.36 | 0.82 | Table headers, "Open file" field |
| `fg-3` | 0.45 | 0.72 | Branch, section labels, file meta (outside rows with hover: there `fg-2`) |
| `glass` | 0.965 / 60 % | 0.18 / 60 % | Tint over the system glass, on frame and sidebar (Matteo's choice, 2026-11-14) |
| `line` | black 9 % | white 14 % | Decorative lines only: table rows, tree guides, field borders |
| `sel` | black 6 % | white 12 % | Selected row, hover, selection, fill of Find results |
| `mark` | light `fg-3` at 70 % | dark `fg-3` at 65 % | Marks with a meaning: ring of Find results, "changed" mark |

### Path icon colors

The only colors in the document (record `2026-11-23-t3-style-paths`, constraint 1, T3 Code's per-type colors). They go
on the 1 em icon before a path and nowhere else: the path text stays in the grays above, so the color never carries
meaning alone (the name says the file). Web link icons (globe, GitHub mark) stay gray, in the link's `fg`.

| Kind | Light | Dark | Files |
|---|---|---|---|
| Folder | `#5d6f8f` | `#9fb3d6` | a path ending in `/` |
| Data | `#b58a12` | `#f2c94c` | `.json` `.jsonl` `.yaml` `.yml` |
| Rust | `#c2621d` | `#ffa359` | `.rs` |
| Text | `#1a77c2` | `#69b1ff` | `.md` `.markdown` `.mdx` `.txt` |
| Image | `#9b2fb2` | `#d568ea` | `.png` `.jpg` `.gif` `.webp` `.svg` `.pdf` |
| Shell | `#16883a` | `#5ecc71` | `.sh` `.zsh` `.bash` `.fish` |
| Other | `#6f6f76` | `#adadb1` | every other file (generic file icon) |

The code header's language icon uses the same shapes in `fg-3`, without color.

**Contrast.** The ratios of each token are in the "Contrast" section below. With the tint at 60 % the panel grays
on glass fall under the minimums (measurements below, "Real glass at 60 %"): on glass, that is in the whole sidebar
(`Theme::on_glass`), `fg-2` and `fg-3` are the stronger `-glass` tokens; the document panel keeps its own.

In the app the glass is `El::system_material` (Liquid Glass on macOS 26+) in a transparent window: Rune turns on the
system glass only there, and when it is there it does not paint the element's fill. So the `glass` tint sits in a
layer over the glass, across the whole window. Without system glass (headless, material off) the root is the solid
tint (`Theme::glass_fallback`): dark `0f1216`, light `f2f3f6`, as before. `RUNEMD_GLASS=<percent>` changes the tint
for photos. If the contrast of `fg-3` falls under 4.5:1 on the real material, raise `fg-3`, do not remove the
glass.

**Real glass at 60 %** (2026-11-14, real window, sidebar background measured on 40×40 px and WCAG ratios with the
tokens): white background in the dark theme (glass `3c3e40`): `fg` 9.28, `fg-2` 6.15, `fg-3` 4.33, `fg-2` on glass +
`sel` 4.27. Black background in the light theme (`999a9c`): `fg` 6.15, `fg-2` 3.86, `fg-3` 2.64, `fg-2` on glass +
`sel` 3.44. Sonoma background: dark (`354416`) `fg-3` 4.25, `fg-2` + `sel` 4.23; light (`becb9d`) `fg-3` 4.33,
`fg-2` + `sel` 5.57.
These were the panel grays. Measured a second time, the glass varies by a few points (dark `3e3f40` and
`374618`, light `9e9fa1` and `bfcd9f`): the test uses the hardest measurement of each case. With the `-glass` tokens
(dark `fg-2` 0.90, `fg-3` 0.86; light `fg-2` 0.25, `fg-3` 0.28):

| Measured background | `fg` | `fg-2` | `fg-3` | `fg-2` + `sel` | `fg-3` + `sel` | Focus |
|---|---|---|---|---|---|---|
| Dark, white desktop (`3e3f40`) | 9.12 | 7.83 | 6.89 | 5.45 | 4.80 | 3.66 |
| Dark, Sonoma (`374618`) | 8.84 | 7.59 | 6.69 | 5.34 | 4.70 | 3.55 |
| Light, black desktop (`999a9c`) | 6.15 | 5.68 | 5.18 | 5.06 | 4.61 | 3.17 |
| Light, Sonoma (`becb9d`) | 10.08 | 9.31 | 8.49 | 8.21 | 7.49 | 5.19 |

### Contrast

WCAG ratios computed by `src/ui/contrast.rs` (`cargo test --profile perf contrast_table -- --nocapture`), with the
color composited on the background, alpha included, in encoded sRGB as the window composites it. "Glass" is the
sidebar background measured in the real window on the worst desktop background (white in the dark theme, `3e3f40`;
black in light, `999a9c`; `contrast::GLASS_GROUNDS`, which also measures Sonoma), with the text in
`Theme::on_glass`. Minimums: 4.5:1 for text, 3:1 for marks with a meaning in both themes, none for decorative lines
and fills. The tests fail if a minimum is not met or if the light theme gets worse. "Before" is 2026-11-13, "after"
the fix of 2026-11-14 (`docs/decisions/2026-11-14-find-dark-contrast.md`).

| What | Dark before | Dark after | Light before | Light after | Minimum |
|---|---|---|---|---|---|
| `fg` on panel | 16.89 | 16.89 | 17.07 | 17.07 | 4.5 |
| `body` on panel | 14.50 | 14.50 | 14.39 | 14.39 | 4.5 |
| `fg-2` on panel | 10.46 | 11.19 | 10.71 | 10.71 | 4.5 |
| `fg-3` on panel | 7.04 | 7.88 | 7.34 | 7.34 | 4.5 |
| `fg-3` on glass (`fg-3-glass` after) | 4.26 | 6.89 | 4.89 | 5.18 | 4.5 |
| `fg-2` on glass (`fg-2-glass` after) | 6.32 | 7.83 | 7.13 | 5.68 | 4.5 |
| `fg-3-glass` on glass + `sel` (hover) | — | 4.80 | — | 4.61 | 4.5 |
| `fg-2-glass` on glass + `sel` (hover) | — | 5.45 | — | 5.06 | 4.5 |
| Duplicate name of the selected row on glass + `sel` (`fg-3` before, `fg-2-glass` after) | 3.22 | 5.45 | 4.30 | 5.06 | 4.5 |
| Small text of pressable rows (Recent time, duplicate name, "Open System Settings") on glass + `sel` (hover; `fg-3` before, `fg-2-glass` after) | 3.22 | 5.45 | 4.30 | 5.06 | 4.5 |
| Highlighted outline heading (`fg`) on panel + `sel` (hover) | — | 12.53 | — | 14.94 | 4.5 |
| Outline headings (`fg-2`) on panel + `sel` (hover) | — | 8.30 | — | 9.38 | 4.5 |
| `mark` on panel (ring of Find results, "changed" mark) | 1.19 | 3.89 | 1.22 | 3.52 | 3 |
| `mark` on `sel` (Find ring, inner side) | 1.27 | 3.34 | 1.22 | 3.27 | 3 |
| Ring of the active result on `sel` (`fg-3` before, `fg-2` after) | 5.73 | 8.30 | 6.42 | 9.38 | 3 |
| "Is writing" ring on the tab bar (pixels, minimum, 5 px band; see below) | — | 5.01 | 3.17 | 4.74 | 3 |
| "Is writing" ring on the tab's `sel` fill (pixels, minimum, 5 px band) | — | 3.73 | 2.78 | 4.16 | 3 |
| Keyboard focus on glass | 1.77 | 3.66 | 1.68 | 3.17 | 3 |
| Keyboard focus on panel | 2.13 | 6.78 | 1.97 | 8.80 | 3 |
| Settings: chosen option and its check mark (`fg`) on panel + `sel` | — | 12.53 | — | 14.94 | 4.5 |
| Settings: options and buttons (`fg-2`) on panel + `sel` (hover) | — | 8.30 | — | 9.38 | 4.5 |
| Document: unchecked task box, `fg-3` border on panel | — | 7.88 | — | 7.34 | 3 |
| Document: checked task box (`fg-2`) and its check mark (`panel`) | — | 11.19 | — | 10.71 | 3 |
| Document: unchecked task box, `fg-3` border on panel + `sel` (selection) | — | 5.84 | — | 6.42 | 3 |
| Document: checked task box (`fg-2`) on panel + `sel` (selection) | — | 8.30 | — | 9.38 | 3 |
| Settings: unchecked box, `fg-3` border on panel | — | 7.88 | — | 7.34 | 3 |
| Settings: checked box (`fg-2`) and its check mark (`panel`) | — | 11.19 | — | 10.71 | 3 |
| Settings: unchecked box, `fg-3` border on panel + `sel` (hover) | — | 5.84 | — | 6.42 | 3 |
| Settings: checked box (`fg-2`) on panel + `sel` (hover) | — | 8.30 | — | 9.38 | 3 |
| Settings: width track (`fg-3`) and knob (`fg-2`) on panel | — | 7.88 | — | 7.34 | 3 |
| Settings: fire row (`fg-2`) on panel + `sel` (hover) | — | 8.30 | — | 9.38 | 4.5 |
| Settings: "Already watched" (`fg-2`) on panel | — | 11.19 | — | 10.71 | 4.5 |
| Label of the fire when off (`fg-2`) on panel | — | 11.19 | — | 10.71 | 4.5 |
| Frontmatter (`fg-2`) on panel + half `sel` | — | 9.89 | — | 10.03 | 4.5 |
| Tab name, front (`fg`) on panel + `tab-bar` + `sel` (pill) | — | 13.24 | — | 13.69 | 4.5 |
| Tab name, back (`fg-2`) on panel + `tab-bar` | — | 11.52 | — | 9.81 | 4.5 |
| Tab name, back on hover (`fg-2`) on panel + `tab-bar` + `sel` | — | 8.78 | — | 8.60 | 4.5 |
| "⌘E source" (`fg-3`) on panel + `tab-bar` | — | 8.10 | — | 6.72 | 4.5 |
| Tab × (`fg-2`) on panel + `tab-bar` + `sel` (front tab pill) | — | 8.78 | — | 8.60 | 3 |
| Tab × (`fg-2`) on panel + `tab-bar` + `sel` (back tab on hover) | — | 8.78 | — | 8.60 | 3 |
| Tab × (`fg-2`) on hover on panel + `tab-bar` + `sel` + `sel` | — | 6.01 | — | 7.54 | 3 |
| Width ↔ button (`fg-2`) on panel + `tab-bar` | — | 11.52 | — | 9.81 | 3 |
| Width ↔ button (`fg-2`) on panel + `tab-bar` + `sel` (hover, popover open) | — | 8.78 | — | 8.60 | 3 |
| Width popover: title (`fg-2`) and value (`fg`) on panel | — | 11.19 / 16.89 | — | 10.71 / 17.07 | 4.5 |
| Width popover: shortcuts and track (`fg-3`) on panel | — | 7.88 | — | 7.34 | 4.5 |
| Shortcuts ⌘ button (`fg-2`) on panel / panel + `sel` | — | 11.19 / 8.30 | — | 10.71 / 9.38 | 3 |
| Settings › Shortcuts: rows, keys (`fg-2`) and group headings (`fg-3`) on panel | — | 11.19 / 7.88 | — | 10.71 / 7.34 | 4.5 |
| ⌘P: name (`fg`) and repo or folder and time (`fg-2`) on panel + `sel` | — | 12.53 / 8.30 | — | 14.94 / 9.38 | 4.5 |
| `line` on panel (decorative lines) | 1.19 | 1.44 | 1.22 | 1.22 | — |
| `line` on glass (field border) | 1.28 | 1.52 | 1.21 | 1.19 | — |
| `sel` on panel | 1.23 | 1.35 | 1.14 | 1.14 | — |
| `sel` beside inline code | 1.04 | 1.15 | 1.03 | 1.03 | — |

- The keyboard focus is the system color (`Cx::focus_color`, AppKit gives it at 50 %), 3 pt, solid in both
  themes; in light it is also darkened by 68 % toward black and in dark lightened by 10 % toward white (in linear
  light), because on the real glass at 60 % the blue darkened by 20 % gave 1.70:1 in light and the full blue
  2.94:1 in dark. Measured with the default blue.
- Field borders stay in `line`: the field is recognized by its icon and the placeholder in `fg-2`, not by the
  border.
- The "is writing" ring is Rune's liquid metal (`LiquidMetalParams::chrome`), not a token: it has no color of ours
  to adjust. Measured on the pixels of `docs/proof/contrast-dark-after.png` (scale 2, one frame): for each point of
  the straight sides around the tab the pixel with the most contrast over the 3 px thickness is taken; the minimum
  is 4.62:1 on the panel and 3.44:1 on the tab's `sel` (medians 11.37 and 8.46), that is ≥ 3:1 on the whole
  perimeter. The reflected studio turns, so another frame gives slightly different values.
- In the light theme (final review I5) the same method (`bench/ring_contrast.py`, scale 2, `RUNEMD_THEME=light`,
  the tab of `bench/worst_case.sh` at rect 258,16,240×28 pt) on Rune's chrome gave a minimum of 3.17:1 on the bar
  and 2.78:1 on `sel` (medians 8.30 and 7.28): under 3 on `sel`. In light the metal is darker (`fx::ring::metal`:
  tint `d8dbe0` and highlights `e6e7ea` in place of `e9ecf0` and `ffffff`); now a minimum of 4.86:1 on the bar and
  4.26:1 on `sel` (medians 11.01 and 9.65), and on a second frame 1.5 s later 4.77 and 4.19 (medians 10.54 and
  9.24). Proof: `docs/proof/ring-light-before.png`, `docs/proof/ring-light-after.png`. In dark the metal stays the
  chrome; with the same script on the tab of `bench/worst_case.sh` it gives a minimum of 4.43 on the bar and 3.30
  on `sel` (medians 10.66 and 7.93).
- The numbers above are for the 1.5 px ring (a 3 px band at scale 2). With the 2.5 px ring (`fx::ring::RING_W`,
  commit 94c9fa9) the band is 5 px (`bench/ring_contrast.py`, default; 2026-11-15, `bench/worst_case.sh`, tab at
  258,16,240×28 pt, two runs with the same frame): dark minimum 5.01 on the bar and 3.73 on `sel` (medians 13.92
  and 10.36), light 4.74 and 4.16 (medians 13.87 and 12.16); ≥ 3:1 on the whole perimeter in both themes. Compared
  with the earlier values dark goes up (4.62 → 5.01, 3.44 → 3.73), light goes down a little (4.86 → 4.74,
  4.26 → 4.16), always above 3. On the same frame the 3 px band would give dark 3.15/2.34 and light 2.43/2.13: it
  sees only the inner 3 px of the ring, not its whole thickness. Proof: `docs/proof/ring-2-5px-dark.png`,
  `docs/proof/ring-2-5px-light.png`.
- 2026-11-17, before release, every metal edge is about 30 % thicker: ring and ⌘P panel 3.25 px (`RING_W`), calm
  tab edge 2.3 px (`CALM_W`, still 0.7 of the ring). Same script with a 6 px band (`bench/metal_tab.sh`, writing
  shots, tab at 369.5,16,238×28 pt, one frame): dark minimum 6.16 on the bar and 4.59 on `sel` (medians 14.60 and
  10.86), light 5.99 and 5.25 (medians 17.51 and 15.36); ≥ 3:1 on the whole perimeter in both themes. Proof:
  `docs/proof/thicker-metal/`.
- 2026-11-18, moving metal: every metal edge is one width, 2 pt (`RING_W`), calm and writing alike; it replaces
  3.25 (ring, ⌘P panel) and 2.3 (`CALM_W`, removed). Proof: `docs/proof/moving-metal/`.
  Contrast re-measured 2026-11-18 at 2 pt (`bench/metal_tab.sh`, `bench/ring_contrast.py PHOTO 368 16 240 28 4`:
  tab rect read off the pixels, a 4 px band = 2 pt at scale 2, one frame per shot). The metal moves, so each
  frame differs; four frames per theme of the calm edge (0.1, 0.6, 1.3, 2.1 s after ready) and one of the
  writing ring:

  | 2 pt edge, frame | Dark: min on bar / on `sel` (median) | Light: min on bar / on `sel` (median) |
  |---|---|---|
  | Writing ring | 5.38 / 4.00 (9.58 / 7.13) | 3.42 / 3.00 (17.03 / 14.93) |
  | Calm, 0.1 s | 1.53 / 1.14 (11.42 / 8.50) | 3.26 / 2.86 (11.26 / 9.87) |
  | Calm, 0.6 s | 5.94 / 4.42 (9.02 / 6.71) | 5.49 / 4.81 (16.58 / 14.54) |
  | Calm, 1.3 s | 5.14 / 3.83 (18.58 / 13.82) | 5.28 / 4.63 (13.74 / 12.04) |
  | Calm, 2.1 s | 5.45 / 4.06 (8.70 / 6.48) | 5.74 / 5.04 (16.98 / 14.89) |

  6 of 8 calm frames and the writing ring keep ≥ 3:1 on the whole perimeter on `sel`. Two frames dip: dark at
  0.1 s, 5 px of the left side (99 % of the perimeter ≥ 3; the studio's dark floor passes there, pixels
  `26 26 26`–`38 38 38` on `sel` `28 29 2b`) and light at 0.1 s (min 2.86 on `sel`, a few points; over 99.5 % ≥ 3). The dip moves on within a turn (`TURN`, 3 s); the 3.25 pt values above it no
  longer apply. Photos: not committed (scratch), regenerate with the commands above.
- The duplicate name (repo or folder) is in `fg-2` on the selected ⌘P row: in `fg-3` on `sel` it went down to
  3.2:1.
- Each small text inside a row that takes `sel` on hover on glass (the second line of Recent, that is folder or
  repo and time; the ⌘P duplicate name; the "Open System Settings" link) is in `fg-2` (`sidebar::row_meta`): with
  the panel grays `fg-3` on glass + `sel` gave 3.28:1 in dark and 4.30:1 in light (final review I4); today
  `fg-3-glass` + `sel` passes too (4.80 / 4.61). `fg-3` stays only outside pressable rows: section labels, the
  branch and icons of the tree title (which has no hover), "⌘E source" on the panel.

## Typography

Inter for everything, `ui-monospace` for code (in Rune: `families: ("Inter", "ui-monospace, Menlo")`, otherwise
the code comes out in Inter). Tabular figures everywhere (`tnum`).

| Role | Size | Weight | Line height |
|---|---|---|---|
| File title (H1) | 28 | 680 | 1.2, tracking −0.015em |
| H2 | 19 | 640 | 1.3 |
| H3 | 17 | 640 | 1.3 |
| H4 | 15.5 | 600 | 1.3 |
| H5 (`fg-2`) | 14 | 600 | 1.3 |
| H6 (`fg-3`) | 13 | 500 | 1.3 |
| Body | 15.5 | 400 | 1.62 |
| Table | 14 (headers 13, weight 560) | 400 | — |
| Code | 13.5 mono | 400 | — |
| UI (rows, tabs) | 13 | 400; repo 600 | — |
| Small UI (times, sections, meta) | 12 | 500 | — |

Bold in the body is weight 620 in `fg`. Headings are not huge: the hierarchy comes from weight and color. Headings
1 to 4 have the body color; 5 goes down to `fg-2` and 6 to `fg-3`: each level stands apart from the one before, and
3 does not weigh like a bold line.

A word longer than the line (a path, a file name) wraps after `/`, `-`, `_` or `.`, so the line ends with the
separator and the next one starts with the piece after it; only a run without separators wider than a line (a
hash) breaks at any point. No line overflows the column. A long bare URL wraps the same way and stays a single link,
with the address intact.

**Callouts** (`> [!NOTE]`, `[!TIP]`, `[!IMPORTANT]`, `[!WARNING]`, `[!CAUTION]`): the quote with the label in words
above it (Note, Tip, Important, Warning, Caution), 13/600 in `fg`, with the 14 icon in `fg-2` 6 before it; 6 under
the label, on the same line as the quote. The `[!NOTE]` marker is not shown. No color: the word and the icon say the
type.

**Footnotes** (`[^1]`): in the text the number in superscript, in order of first reference; the definitions are not
drawn where they are written but at the end of the file, under a 0.5 line in `line` and 12 of space, as a numbered
list at 13.5 in `fg-2`. A reference without a definition stays written as it is. Find finds the text of the notes in
the list at the end, where it is shown.

**Paths and file names** (with or without backticks, and local link destinations; record
`2026-11-23-t3-style-paths`, mockup `colori` in `design/mockups/2026-11-23-t3-style-markdown/`; it replaces the
2026-11-16 p-mix look). A path to a file or folder that exists (beside the file, in the repo, absolute or `~`) is a
link drawn as T3 Code draws it: the type icon (1 em, gap .24 em, color from "Path icon colors") and only the base
name, body font, weight 500, in `fg`, no background. A line number shows as `L88` (a range as `L42–64`) after the
name, in `fg-3`. Two files with the same name in one document show, in `fg-3`, the shortest folder tail that tells
them apart (at least two levels, as T3 Code). Hover: a 1 underline in `fg-2` and, at once (no delay), a popover with
the full path (home as `~`) and the size, or for a folder "Click shows it in the sidebar"; popover: `panel` fill,
0.5 ring in `line`, radius 8, 10 × 7 inside, 12/450 in `fg-2`, max 520 wide, the panel's shadow. Rune's tooltip look
is global, so the other tooltips (tabs, shortcuts) also open at once. Click: a file opens in a tab (at the line), a
folder opens in the sidebar tree (in Finder if no sidebar root holds it); ⌘-click opens the file in its default app.
Right-click: a native menu with Show in Finder, Copy Relative Path (from the repo root, else the file's folder), Copy
Full Path. A file path that does not exist is not a link (no icon, no hand cursor, no click): inline code, without a
pill if it is the only code in the block, with the pill if not; a missing folder or link stays as written. Nothing in
code blocks; words with a dot (`etc.`, `v1.2`, `Node.js`) and URLs stay text.

**Web links**: an icon before the text, in the link's `fg`: the GitHub mark for `github.com`, a globe for other
`http(s)` links (the kit's icons).

**Code block header** (record `2026-11-23-t3-style-paths`, feature 2): a 32 row inside the code card, on top: 12
left, 4 right, 6 between. On the left the language as written (or, if the fence names a file, its base name) at
12/500 in `fg-3` with its 13 type icon in `fg-3`; on the right two 26 square buttons, radius 6, 14 icons in `fg-2`,
hover `sel`: wrap (filled `sel` while on) and copy. Wrap is off by default: long lines scroll sideways; it holds for
the block until the app quits. No run button: a reader does not run commands. The card's edge is the table
box's: 1 in `line`, radius 6, in both themes, and no other border (review R2, record `2026-11-23-t3-paths-review`;
before, a gunmetal edge over the kit's own border).

**Copy table**: under each table, on the right, 4 below it, a ghost button: 24 tall, 7 at the sides, radius 6, the 13
copy icon and "Copy table" at 12/500 in `fg-2`, 5 between, hover `sel`. It opens a native menu: Copy as Markdown (the
table as written) or Copy as CSV.

**Registers and plans** (record `2026-11-16-tasks-sections-and-columns` with `2026-11-16-wrap-only-label-blocks`,
mockup t-mix): a paragraph in which more than half of the lines, and at least two, start with a label (one to three
words with a capital or a digit and a colon, like `Task 1` or `Ruling`) keeps each line break of the file, with the
labels in bold. The other paragraphs stay standard Markdown: a line break counts as a space. A `Task N: title`
heading shows the number in the heading font in `fg-3`, then the title, and on the right the progress ("2 of 4",
the small interface font in `fg-3`) from the boxes between the heading and the next one of the same or a higher
level. If the text column (without the rows' margins) is at least 480 wide, a label block goes on two columns: bold
labels without the colon on the left, as wide as the widest and at most 10 em, 16 of space, text on the right
aligned row by row; narrower, one column.

**Task list box**: like the one in Settings, 14 pt, radius 4, 8 inside the marker column; unchecked empty with a 1
border in `fg-3`, checked solid in `fg-2` with the check mark in `panel` ("Contrast" table).

## Layout

- Window: a glass sidebar 248 px wide on the left; the document panel sits 8 px from the top, right and bottom
  edges, and the glass surrounds it on all four sides.
- Sidebar, from the top: traffic lights; the "Open file ⌘P" field; the "Recent" section (rows on two lines: file
  name above, folder or repo and relative time below); a line; the tree title (repo icon, repo name and below it the
  branch with its icon), then the files indented by 16 px with a vertical guide line. No label above the tree: the
  repo name is the title. (`docs/decisions/2026-11-15-recent-two-lines.md`, mockup A in
  `design/mockups/2026-11-14-recenti-leggibili/`.)
- Recent takes at most 40 % of the sidebar height (`recent-share`) and then scrolls, with the system scroll bar
  (thin, auto-hiding); 3 px between rows (`recent-gap`), with or without rings. With 44 rows about 5 files fit (6
  in a sidebar 800 tall) instead of 7–8; the order stays the order of opening. When a file opens the tree scrolls
  just enough to show its row. (D1, D2: `docs/decisions/2026-11-14-sidebar-tabs-long-names.md`.)
- Long file names (Recent, tree, tabs, ⌘P) and the branch are cut in the middle: the end and the extension stay
  (`2026-11-14-arc…definitiva.md`). The repo name is cut at the end. (D4)
- Panel: the tab bar at the top (names of the open files, "⌘E source" on the right); the document column at most
  680 px wide and centered; the "In this file" outline on the right, separated by a line.
- The "In this file" outline hides when the window is narrower than sidebar + column + 200 (1128 pt,
  `outline-min-window`), and in the source (⌘E).
- Under the title, a meta line: folder, state ("modified 2 min ago" / "updating"), words. The items are separated
  with space (16 px), never with " · ".

## Elevation & Depth

Flat. The only depth is the panel on the glass: a 0.5 px border at `ink` 10 % and a soft shadow (`0 10px 30px`
black 35 %). No shadows on rows, tabs or fields.

## Shapes

Small, constant radii: panel 10, field 7, row 6, ⌘ key 4. Lines are 0.5 px.

## Components

**Sidebar row.** 28 px tall (26 in the tree), radius 6. Hover `sel`, only with a fine pointer. Pressed: scale 0.98
in 160 ms ease-out. Selected: `sel` fill, `fg` text.
Sidebar measurements (from the `scheda` mockup, now tokens in `spacing`): 44 of space at the top for the traffic
lights, 8 margins at the sides and 10 at the bottom, row inner margin 10 (8 in the tree), 8 between icon, name and
time, 6 between the name and the repo/folder of a duplicate name, 14 icons and a 10 folder arrow, section labels
with 14 above and 6 below. The hover's `sel` fill goes in and out in 120 ms (`ease`); the pressed row's scale goes
and comes back in 160 ms with the `motion.ease-out` curve. With "Reduce motion" both are immediate.

**Recent row.** 44 tall (`recent-row`), radius 6, 10 margin at the sides, hover `sel` and pressed scale like the
other rows. First line: the file name (UI 13/400, line height 16) in `fg`, as wide as the row; it is cut in the
middle only if it does not fit. Second line, 2 below (line height 15): on the left where the file is, 12/400, cut at
the end; on the right the relative time, 12/500 tabular figures; 8 between the two. Both in `fg-2` (`fg-2-glass`,
`sidebar::row_meta`), also on hover. Where the file is: the repo (the file's folder outside a repo); for a name that
comes back, the repo or folder that tells it apart (duplicate names rule). (Record `2026-11-15-recent-two-lines`,
constraints 1–2.)

**Tree title (repo).** In place of the "File's repo" label, under a 0.5 line in `line` (12 above, 8 below, 10 at
the sides). Repo icon (a book, 14) in `fg-3` on the left, centered on the two lines; 8 after it, the name 13/600 in
`fg` (line height 16, cut at the end) and 1 below the branch icon (12) and the branch, 12/400 in `fg-3` (line height
15, cut in the middle, without tabular figures: the dashes stay narrow), 4 between icon and branch. At least 44
tall, 4 above and below. The row has no hover, so `fg-3` sits on glass (`fg-3-glass`, 6.89:1 / 5.18:1 on the worst
measured background). The two icons are drawn like the kit's (viewBox 16, stroke 1, round joins; the branch one,
smaller, with a 1.2 stroke for the same weight). Outside a repo the title is the file's folder: folder icon and
name, no branch, at least 30 tall. (D3; record `2026-11-15-recent-two-lines`, constraints 4–5.)

**"Open file" field.** 30 px tall, radius 7, `field` fill, 0.5 px inner border in `line`. Icon, text and "⌘P" in
`fg-2` (it must be readable: Matteo's comment). The "⌘P" key is `kbd` text (11 px, 400) with a 0.5 px border in
`line`, radius 4 and inner margin `kbd-pad`; the field has margin `field-margin`.

**Quick Open (⌘P).** The "Open file" field is a text field: ⌘P gives it focus. While the query is not empty, the
results list takes the place of Recent and the tree. The rows are the sidebar's (28 px, radius 6, hover `sel`,
pressed scale 0.98); the row chosen with the arrows has a `sel` fill and `fg` text. A duplicate name shows repo or
folder in `fg-2`, as in Recent. Return opens, Esc clears and closes, no opening animation (a keyboard action). No
accent: caret `fg`, selection `sel`. No results: "No files" in `fg-3`.

**Tab.** VS Code's shape (record `2026-11-18-vscode-tabs`; the `c6b02b5` version with 8 between tabs read as "troppo
spazio"). The bar is 44 tall (`tab-bar`) on the `tab-bar` tint over `panel` (black 4 % light, 40 % dark: a shade off the
page), 8 before the first tab (`tab-bar-lead`), 10 margin on the right (`tab-bar-pad`), a 0.5 line in `line` at the
bottom. Tabs sit edge to edge, as wide as the file name up to 240 (`tab-max`, D5; beyond that, the name is cut in the
middle), from 8 below the bar's top (`tab-top`) down to the document, UI text 13 px with 12 before the name
(`tab-pad`). Back tabs: no fill, `fg-2` text, hover `sel`, pressed scale 0.98, and a 1 px divider in `line`
(`tab-divider`, 9 short of the tab's top and bottom) on the right, except next to the front tab. The front tab is a pill
inside the bar, as in the installed app (record `2026-11-18-tab-bar-style`): the tab's full width, 24 tall (`PILL_H`),
corners of 5 (`PILL_R`, half the page panel's `R_PANEL`: Matteo, 2026-11-18, constraint 5), `sel` fill and `fg` text; the bar's line runs on under it. A closed metal
ring goes all round the pill (`RING_W` outside it, concentric: outer radius 7, the moving chrome at 8 steps a second, still with Reduce Motion or an
inactive window), wholly inside the bar with the same room above it and below it, 8 (`RING_GAP`) to the bar's top and to
its line (Matteo, 2026-11-18: it had 12 above and sat on the line). The names, the × and the buttons on the right sit on
the pill's middle, 22 from the bar's top. The scroll bar runs in the middle of the room under the ring, 2.5 from the
ring and from the line, so it touches neither; the ring only crosses the neighbours' padding (const check `_RING_FITS`). No outline round the page.
At the end of each tab a "×" (glyph 14 in `fg-2`, area of 16, radius 4, hover `sel`, 6 after the name and 8 from the
right edge), always shown on the front tab and on the others only on hover (or with keyboard focus); its space is
always there, so hover changes no width. Label "Close <name>"; a click closes like ⌘W (the same question with unsaved
changes); the middle click stays. Preview tab (a single click in the sidebar, like VS Code): the same look, name in
italics; at most one, the next single click replaces it in the same place. Too many tabs: the strip scrolls sideways
and passes under the buttons on the right, whose backdrop (the bar's colour at 78 % over a 6 pt blur) fades in over
24 pt (`FADE_W`); the strip's own ends fade as well while tabs are hidden there. A 3 pt scroll bar (`fg-3` at 45 %,
at least 24 wide) sits under the front tab's ring while the tabs overflow, and the front tab scrolls into view whenever it
changes (click, keys, a file opened, launch), clear of the fade. "⌘E source" on the right like the "⌘P" key, in
`fg-3`.

**Text width (↔ button).** In the tab bar, just left of "⌘E source": 28 × 28 (`row`), radius 6, glyph "↔" 15 in
`fg-2`, hover `sel`; with the popover open it has the `sel` fill. Label "Text width". A click opens under the button,
aligned to its right edge and 4 lower, a popover 248 wide (`sidebar`): `panel` fill, radius 10, 0.5 border at `ink`
10 % and the panel's shadow (the same as the only depth there is); inside "Text width" (UI 13 in `fg-2`), the same
stepped slider as in settings and "⌘⌥+ and ⌘⌥− one step at a time" (12 in `fg-3`). There is only one value,
`settings.column`: slider, settings and shortcuts change it together and it is saved. Esc, a click outside or the
button close it. ⌘⌥+ (also ⌘⌥=) and ⌘⌥− take a step also with the popover closed. Contrast: glyph 11.19 / 10.71 on
panel and 8.30 / 9.38 on `sel`; shortcuts 7.88 / 7.34.

**Shortcuts (⌘ button).** Just left of ↔, the same as it: 28 × 28, glyph "⌘" 15 in `fg-2`, hover `sel`, full `sel`
with the popover open, tooltip and label "Shortcuts" (↔ has its own tooltip too, "Text width"). A click opens under
the button the width popover with other content: 300 wide, margins 12 / 14, "Shortcuts" (13 in `fg-2`), the groups
File, Tabs, Reading, App (12 weight 400 in `fg-3`, a step lighter than the rows; 14 above each group (the first: 14 below the
"Shortcuts" title) and 6 below, like the sidebar's section labels; record `2026-11-18-small-choices-review`, constraint 2) and 24 rows with the action (13 in `fg-2`) and the keys on the
right (11.5 in `fg-2`, border `line` 0.5, radius 4). Esc, a click outside or the button close it; opening ⌘ closes ↔
and the other way round. The list is `ui::shortcuts::GROUPS` (docs/decisions/2026-11-15-shortcuts-popover.md).

**Open file (⌘P).** A panel 560 wide (less with a narrow window, 16 from the edges), centered in the window, 20 under
the tab bar, over a `panel` veil at 60 %: it moves nothing. `panel` fill, radius 10, the popover's border and shadow.
At the top the field (48, text 15, icon in `fg-2`), then the rows on a single line (40: file icon, name in `fg`, right
after it repo or folder in `fg-2`, the time in `fg-2` at the far right; if there is not enough room the repo or folder
shrinks first, down to 40, then the name with the cut in the middle), selection and hover `sel`, eight in view; at the
bottom (36) ↑ ↓ choose, ↵ open, Esc close (keys in `fg-2`, words in `fg-3`) and "N files". With an empty query the
rows are Recent. The sidebar's "Open file ⌘P" field is a button that opens the panel. Click: preview, the panel stays;
double click: pinned tab and the panel closes; Return opens in preview and closes. Esc or a click outside close it (a
modal panel). Records docs/decisions/2026-11-15-compact-quick-open.md and
docs/decisions/2026-11-15-quick-open-one-line.md (a single line; Recent in the sidebar stays on two lines).

**Find in file (⌘F).** Only in reading. The field sits in the tab bar, to the right of the tabs and before "⌘E
source", so the document does not move: 240 wide (`find-field`), 30 tall, radius 7, `field` fill, 0.5 px inner border
in `line`, 8 on the right (`row-gap`); magnifier icon and placeholder "Find in file" in `fg-2`, text `fg`. The counter
on the right is small UI in `fg-3`: "3 of 12", or "No results"; with an empty query it is not there. In the text the
results have the `sel` fill and a 1 px inner ring in `mark` (`find-ring`), which sets them apart from inline code and
the selection; the active one has the ring in `fg-2` and 2 px (`find-ring-active`), darker and twice as thick, so it is
recognized at a glance in both themes. The `mark` ring is at 3.9:1 on the panel in the dark theme and at 3.5:1 in
light. No accent, no yellow. Return goes to the next result, Shift+Return to the previous one, wrapping around; the
view brings it to the center. Esc closes and removes the marks. The ⌘P field, the ⌘N one and this one close each
other.

**Outline of headings ("In this file").** From the `scheda` mockup: a column 196 wide (`outline`) to the right of the
document, separated by a 0.5 line in `line` as tall as the document. Label "In this file" in small UI `fg-3`, 4 above
the first heading (`outline-label-gap`), aligned to the document's first line (31 from the top edge, that is the
document's margin plus the row's). The file's H1–H3 headings, one per row: 12.5 px, line height 2 (rows 25 tall,
`outline-row`), `fg-2` text truncated with "…"; the text is 16 from the line (10 of column margin plus 6 inside the
row, `outline-row-pad`, which leaves room for the hover). Each level below the file's highest one is indented by 10
(`outline-indent`). The heading of the section being read (the last one above the first visible block) is in `fg` and
weight 600: color and weight together, because `fg` and `fg-2` alone are hard to tell apart in the dark theme. No mark
and no accent. Hover `sel`, pressed scale 0.98, like the sidebar rows. A click brings the heading to the top of the
view, without a glide (like Find: the row may not be mounted), and that heading stays highlighted even if it does not
reach the top, as long as it is in view and you do not scroll by hand. With the scroll at the bottom the last visible
heading is highlighted: short sections at the end never reach the top. If the outline is taller than the window it
scrolls by itself and the highlighted heading stays in view. A file without headings has no column. In the source
(⌘E) there is no outline: the blocks it points to do not exist there and the headings change while you type.

**Frontmatter.** A file that starts at byte 0 with a `---` line and has a closing `---` or `...` line
(`doc::frontmatter`) shows in reading the YAML between the two lines as a single metadata block: mono 12 (small UI),
`fg-2` text, half `sel` fill, radius 6, inner margin 12 × 10, in the column like a code block but quieter. The outline
does not list it; the source (⌘E) stays the file as it is. Without a closing line it is normal Markdown (a horizontal
rule, paragraphs).

**Reading ↔ source (⌘E).** Crossfade: the incoming view reaches 95 % in ~165 ms, the outgoing one goes under 5 % in
~130 ms (critically damped springs of 0.22 and 0.17 s, `motion.mode`: Rune animates entries and exits only with
springs, and a critical spring already has the tail of a strong ease-out). Before, it was Rune's default fade (0.30 /
0.24 s: 95 % at ~230 ms). With "Reduce motion" it is a hard cut.

**Notice row (source).** Under the tab bar, above the source: 44 tall (`tab-bar`), UI text 13 px in `fg` aligned
with the source's, buttons on the right like background tabs (28 tall, `row-pad` at the sides, `fg-2` text, hover
`sel`, pressed scale 0.98), a 0.5 line in `line` below. It is announced as an alert (`alert`). It says "The file
changed on disk" ("Reload", "Keep mine") or "Save failed" when a save does not write. On close (⌘W or a middle click
on a tab with unsaved changes) the tab comes to the front and the same row asks "Save changes to <file name>?" with
"Save" (also Return), "Don't Save" and "Cancel" (also Esc); the field keeps the focus. Without changes ⌘W closes at
once. Closing the window or quitting saves everything, without asking (Rune cannot cancel the close). No accent.

**Settings (⌘,).** No mockup: tokens only. A column as wide as the sidebar (248, `sidebar`) to the right of the
document, in the panel, in place of the outline, separated by a 0.5 line in `line`; ⌘, opens and closes it, Esc
closes it. Title "Settings" (UI, 600, `fg`), then sections with labels like the sidebar's: "Watched folders" (folder
icon in `fg-3`, path in `fg-2` with the home as "~", "Remove" on the right; "Add folder…" opens the folder dialog; a
folder already watched, or under a watched one, is not added and "Already watched" shows below in `fg-2` for 3 s or
until the next action), "Font" (Inter, SF Pro, New York if installed under its name, then the installed families; a
virtual list 8 and a half rows tall, so the ninth, cut, says it scrolls), "Size" ("−", "15.5 pt", "+"; headings,
tables and code scale with the body), "Text width" (a stepped slider from 680 to 1240 in steps of 80, the last one
"Full": the column follows the panel, the rows' margins stay; a 2 track in `fg-3`, the chosen part and a 12 knob in
`fg-2`, the value on the right in `fg` with tabular figures; arrows one step at a time; the block at the top of the
view stays at the top), "Theme" ("System", "Light", "Dark"), "Fire at the bottom of the panel" ("Brief", the default,
"Always", "Off", options like the theme's; under "Always" the note "Uses about 8–10 % CPU while the window is active."
in 12 `fg-3`, aligned with the option text; docs/decisions/2026-11-17-doc-fire-at-rest.md) and below it the fire's look: "Intensity" (40–175 %), "Height" (55–176 pt
with the fire lit, 110 the default), "Speed" (0.25–3×), three stepped sliders like the width one with the 13 label in
`fg-2` above, and "Color" ("Fire", "Blue", "Violet", "Green", "System accent", options like the theme's). The defaults
are the usual fire. The hot part of each color stays light, so the new text label over the flames keeps 4.5:1. With
the fire off these controls are at 40 % and disabled (out of the Tab order, without hover or click): a disabled
control has no contrast requirement. The rows are the sidebar's (28, radius 6, hover `sel`, scale 0.98). The chosen
option has a `sel` fill, `fg` text and a check mark in `fg`; the others `fg-2` text. With the fire off the label "N new paragraphs further
down" stays, in `fg-2` on a `panel` band 44 tall with the `line` line above. Everything changes at once, without a
restart.

**Sidebar and outline width.** The right edge of the sidebar and the left edge of the outline can be dragged: an
invisible 8 band in the panel's margin, ↔ cursor. Sidebar 180–420 (default 248), outline 150–360 (default 196); the
outline's threshold moves with the two widths, and the drag stops before the outline hides (without the outline, the
panel stays at least 400 wide). The snap learns like Snappy: when you let go of the edge the width is remembered (the
last 4); passing within 8 of a remembered one, or of the default, the edge stops there. Double click: the default. Esc
during the drag: as it was. The widths stay after a restart.

**Live signals (Rune GPU effects).** Each has one meaning only, and no other element uses it.

| Effect | Meaning | Where | Start | End |
|---|---|---|---|---|
| Liquid metal, writing (ring on 4 sides, 2 pt, a 3 s turn) | An agent is writing this file | The file's tab, the whole ⌘P panel (while any file in its list is written; no ⌘P row has metal), its row in "Recent", its row in the tree | Appears in 220 ms ease-out at the first change | Stops when the file no longer changes (assumption: 3 s without writes); it goes back to the calm edge below where there is one |
| Liquid metal, calm (the same chrome, same colour, same 2 pt, always moving; `fx::ring::calm_layer`, docs/decisions/2026-11-18-moving-metal-edges.md) | The tab you are reading, the open ⌘P panel | The active tab, the whole ⌘P panel (the chosen row keeps only its `sel` fill) | With the tab or the panel, no entrance; every calm edge turns in step on Rune's clock | When the file is written (for the panel: any listed file) it is the writing ring above. Reduce Motion or an inactive window: every metal edge is still. At rest it steps 8 times a second without the display link and without running the view (`ring::METAL_FRAME`) |
| Rune fire (flames and embers) | At rest, low (26 px): it is the bottom of the panel. 110 px tall: there is new text below the part you are reading | Bottom of the panel, in reading: with "Brief" (default) at rest for 5 s when the window becomes active, when a document comes to the front and after the tall fire calms, then it fades out in 1 s (smoothstep, display link) and is gone, with no wake-ups; with "Always" at rest all the time; tall, with the label "N new paragraphs further down", in Brief and Always | From rest it rises to 110 in 420 ms ease-out | Goes back to rest in 300 ms ease-out when you scroll to the new text |
| Caret (2 px, `fg`, pulses in 4 steps over 1.6 s: opacity 100/81/62/81 %) | An agent has just written here | At the point of the last change, also in the middle of the file: after the last word changed (or added at the end), as tall as the letter; outside the flow, the text does not move | At the first reload from outside | With the ring (the file no longer changes); if it is out of view the view does not jump |

In the light theme the fire uses luminance as a mask (on white, screen would disappear). Its tips stay warm: Rune's flame cools to near black at the tips, grey on white, so over it goes the same flame 1.8× taller at 80 % with Screen (`fx::fire::tips`): where the flame turns dark it lightens it to orange-yellow, white stays white (docs/decisions/2026-11-18-release-small-choices.md). The dark theme draws the flame alone. The caret at its faintest
step stays ≥ 4.5:1 on the panel and on `sel` (`ui::contrast`): the minimum is 62 % and not the 50 % of the
`caret-piu-forte` record, because at 50 % it gives 3.32:1 in light (4.30:1 in dark on `sel`); at 62 %, panel and
`sel`: light 4.83/4.60, dark 6.80/5.78 (minimum). With "Reduce motion" the ring and the caret do not animate (they stay
still and visible, the caret solid) and the fire is still, at rest or tall, without rising or falling; with the window
inactive it is still in the same way. At rest the flames do not hold the display link: they redraw 12 times a second
(CPU at rest ~3.5 % against ~11 % at 120 Hz, measurements in `.superpowers/sdd/plan/v11-batch-a-report.md`). With
"Brief", once the fire has faded the CPU is as with "Off" (docs/proof/doc-fire-brief/). A still Brief fire (Reduce
motion, inactive window) does not fade: it goes at the end of its 6 s.

## Do's and Don'ts

**Do**
- Keep the text panel still: no effects on the document, except the caret at the point of the last change.
- Give each effect one meaning only; a new state gets a new signal or none.
- Write numbers with tabular figures, right-aligned in tables.
- Measure the contrast on the real material each time a gray or the glass opacity changes.

**Don't** (the anti-slop base of `app-design`, plus RuneMD's rules)
- No " · " or "—" to join meta information: space, named fields or words.
- No labels above a heading that repeat the context; no spaced UPPERCASE labels.
- No cards inside cards, no grids of identical cards, no decorative gradients.
- At most one accent, only for actions, selection and state. RuneMD has none today.
- No color in the document except the path and link icons ("Path icon colors"); never on text, callouts or code.
- No filler labels, no "→" at the end of buttons, no text that explains what the UI already shows.
- No neon glows, status pills, glowing dots or orange bars (Matteo's comment).
- No decorative stats rows ("Today 165 files…").
- The repo is not at the same level as its files.

**Intended exceptions to the skills' rules**
- Colored path icons (record `2026-11-23-t3-style-paths`): the "no accent in the document" rule gives way for the
  icon before a path only, as T3 Code does it; the name beside it says the same thing, so color is never the only cue.
- Glass on the frame and the sidebar: Impeccable forbids it, here it is the identity (Current) and it is system
  material.
- GPU effects (metal, fire, caret): Impeccable and "no decoration" would suspect them; here each one carries a state
  and goes away with the state.
- Press scale 0.98 (Emil 0.97, `better-ui` 0.96): dense 28 px rows, a stronger scale looks like a jump.
