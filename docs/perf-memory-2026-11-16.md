# Memory and scroll: where RuneMD's MB and dropped frames go (16/11)

Question (Matteo, Activity Monitor): RuneMD 122.3 MB, 12 threads, 314 ports; MDLight 65.9 MB, 8 threads. Scroll
feels slightly laggier in RuneMD. Why about twice the memory, how low can it go, how to make scroll smoother.

## Setup

- Code: `3d41c32` built with `cargo build --profile perf` in a throwaway worktree (own target dir), bundled like
  `make app` but ad-hoc signed. Experiments are env knobs (`XP_*`) in that worktree only, listed in the appendix;
  nothing is implemented on main. `main` moved on to `5c43b2d` during the session (docs and a tagline/fire-height
  tweak; not re-measured).
- MDLight: the instrumented copy `_vendor/md-light-bench/build/MDLight.app` (same app as the spike, with the
  `MDBENCH_SCROLL` probe). Its scroll metric is the interval between its own ticks, not presented frames (spike doc,
  `rune-md-spike/docs/spike/2026-11-13-rune-vs-md-light.md`), so it is only roughly comparable.
- Machine: M1 Pro, 16 GB, **9.0 GB of swap in use**, one screen on (external 3440×1440 at **1×**, 100 Hz; the
  Retina panel was off, so nothing could be measured at 2×). Load average (`sysctl vm.loadavg`, 1 min) went from
  **11 to 320** during the session; every row gives its range. RuneMD opens at 900×1000 pt = 900×1000 px here.
- Prefs: a copy of Matteo's `prefs.json` and `index.json` under `RUNE_DIRS_ROOT` (fire on / fire off copies), so the
  real roots (`~/Desktop/GitHub`, 7448 entries) are watched and scanned as in his app.
- "Memory" = `phys_footprint` (what Activity Monitor shows), from `footprint -p PID`; categories from the same
  output and `vmmap --summary PID`; live heap from `heap -s PID` ("All zones: … bytes"); per-allocation sites from
  `heap -s` on a process launched with `MallocStackLogging=lite`.

Method per memory row: `open -n <app> --env RUNE_DIRS_ROOT=… --args <file>`, wait 20–35 s, then `footprint`,
`vmmap --summary`, `heap -s`, kill the exact PID. Scroll rows: `MDSPIKE_SCROLL=4` (4 s, 1500 pt/s normal,
3000 pt/s big), `missed = missed_refreshes / (frames + missed_refreshes)` from the `scroll frames` probe line,
CPU per frame from `MDSPIKE_FRAMES` (`update_ms + encode_ms`).

## Answer in short

1. **At the same scale and file, RuneMD is not twice MDLight.** At 1×: RuneMD 46–117 MB, MDLight 50–113 MB
   (same commands, same minutes). RuneMD's *live* heap is smaller than MDLight's (29–41 MB vs 56–64 MB).
2. The gap Matteo sees comes from three things, in this order (the first is an estimate: no 2× screen today):
   - **Swapchain at Retina scale.** RuneMD keeps **3 window-sized BGRA drawables** (measured at 1×: 3 × 3.67 MB
     "CAMetalLayer Display Drawable" 900×1000); MDLight keeps **1** CA backing store. At 2× a 900×1000 pt window is
     13.7 MiB per drawable: **~41 MiB vs ~21 MiB, +20 MiB** (estimate: measured sizes × 4).
   - **Heap fragmentation.** Dirty "Malloc Small" is 21–78 MB for a live heap of 29–41 MB: **0–38 MB of pages
     that hold a few live objects**, different on every launch, worst after the startup scan and after a long scroll.
     `malloc_zone_pressure_relief` gives back **0 bytes** (they are not free pages). mimalloc as the global
     allocator cut the worst case from **117 to 69–75 MB**.
   - **Startup and scroll peaks.** Footprint peaks at **93–102 MB** around the first frame and decays to 59–75 MB
     in 10–30 s; a 10 s scroll of big.md adds **+5–10 MB live** (glyph cache, Rune vectors, layouts) and up to ~45 MB dirty.
3. **Scroll.** Under the same load the full RuneMD UI misses **17–26 %** of refreshes on big.md while the spike binary
   (same Rune rev `384c2991`, simple opaque UI) misses **3.3–5.5 %** (6 interleaved pairs, load 14–112). The long frames
   are **not CPU frames** (CPU p50 2.3 ms, p95 3.4 ms; 0 of 11 long frames had CPU > 8 ms). The strongest lever
   measured is **pixels per frame**: a 2400×1400 px window (the pixel count of 900×1000 pt at 2×) misses
   **10–44 %**. Fire on/off, `effects_lit()`, the Rgba16Float format, the rounded panel clip, sidebar and tab bar:
   **no single one moves the number beyond noise**. An opaque window was better at 2400×1400 in 3 of 3 paired rounds.

## Where the MB go

big.md open, 900×1000 px at 1×, 25–35 s after launch, fire on. Typical run first, range of all runs after.

| category (footprint) | RuneMD | MDLight | what it is | source |
|---|---|---|---|---|
| **total** | 56 (typical) [46–117] | 86–113 | | `footprint -p`, 20 launches each config, 16/11 14:00–16:00 |
| peak since launch | 63–118 | 81–123 | first frame / doc build | `phys_footprint_peak` |
| Malloc Small, dirty | 30 [21–78] | 65–77 | heap pages incl. fragmentation | `footprint` |
| └ live heap | 30.4 [28.7–41] | 56–64 | bytes in use | `heap -s` |
| └ ColorSync TRC tables | 9.2 | 9.9 | system colour transforms (display profile), both apps | `heap -s` + MallocStackLogging |
| └ glyph atlas CPU mirror | 4.0 (one 2048² R8 page) | — | `rune::text::atlas::Atlas::alloc` | same |
| └ Rune vectors (instances, buffers) | 2.8 still, 5.5 after scroll | — | `RawVecInner::finish_grow` in rune | same |
| └ index entries | 1.5 (7454 allocs) | — | `std` paths of the 7448 indexed files | same |
| └ document blocks | 1.2 (3041 allocs) | — | `doc::ids::assign` | same |
| └ parley layouts | 0.65 still, 1.4 after scroll | — | mounted rows | same |
| └ AppKit/ObjC/Swift runtime | ~3 | ~5 + TextKit storage | | `heap -s` |
| Malloc Large | 0.1 (file) / 3.4–4.1 (empty screen) | 0–10 | wordmark pixels on the empty screen | `footprint` |
| IOSurface (drawables) | 11.0 = 3 × 3.67 [1–3 drawables: 3.7–11] | 2.5–7.6 (1 × "CA Whippet Drawable", RGhA) | swapchain | `vmmap` region list |
| GPU-owned (unmapped, graphics) | 5.3 file / 9.3–10 empty | 0–1.2 | atlas page 4 MB + buffers; +4–4.7 wordmark | `footprint` |
| IOAccelerator (graphics) | 1.4–4.4 | 1.2 | Metal buffers | `footprint` |
| __DATA / __DATA_DIRTY | 2.1 + 2.3 | 2.4 + 2.3 | statics | `footprint` |
| stacks (6–15 threads) | 0.4–0.6 | — | threads are not a memory item | `footprint` |

At 2× (estimate, sizes × 4): drawables 3 × 13.7 = 41 MiB vs MDLight ~21 MiB; atlas glyphs 4× the area, so the second
2048² page (another 4 MB GPU + 4 MB CPU) fills sooner (`MAX_STEADY_PAGES = 2`, `crates/rune/src/text/atlas.rs`).
A plausible reading of Matteo's 122 MB (estimate): 56–75 at 1× + 30 for 2× drawables + 0–38 fragmentation + 0–8
second atlas page.

Threads and ports: 6–15 threads (`ps -M`) depending on moment (Rune image decoders, `runemd-index`, the watcher,
Find, reload threads, Metal/CA). Their stacks add < 1 MB; the 314 ports are mostly IOSurface/CA/FSEvents rights. Not
a memory lever.

## Experiments (each in the worktree, measured against the same binary without the knob)

Memory: 2–3 launches each, interleaved, load 100–320. Total footprint swings ±30 MB on its own (fragmentation), so
the table also gives the deterministic GPU categories.

| experiment | knob | total MB (with / without) | category delta | verdict |
|---|---|---|---|---|
| all effects off (no fire, `effects_lit` false → `draw_ui`) | `XP_NOFX=1` + fire off | 64–84 / 56–87 | IOSurface, GPU-owned, IOAccel unchanged | no measurable MB |
| fire off (setting) | prefs `fire: false` | 61–98 / 63–117 | unchanged | no measurable MB |
| fire not animated at rest | `XP_NOREST=1` | 54–63 / 56–87 | unchanged at 1× | no MB at 1×; idle CPU below |
| no `add_format(Rgba16Float)` | `XP_NOADDFMT=1` | 69–80 / 56–87 | unchanged | no MB: rune-fx pipelines build lazily (`pipes.add` only registers) |
| no wordmark images (empty screen) | `XP_NOIMG=1` | 56–65 / 64–73 | Malloc Large 3.4–4.1 → 0–0.1; GPU-owned 9.3–10 → 5.3 | **−4 MB CPU, −4 to −4.7 MB GPU**, empty screen only |
| file open from launch | — | — | Malloc Large 0.1, GPU-owned 5.3 | images are not loaded when a file is open; release after "empty → open file" not measured |
| window 1700×1100 px | `XP_SIZE=1700,1100` | 71–83 / 56–87 | IOSurface 7.4–22 (2–3 × 7.5) | drawables scale with pixels |
| no watched roots (no scan, no watcher) | `RUNEMD_ROOTS=<empty dir>` | 60–61 / 56–87 | Malloc Small 31–32 for 30.7–31.2 live | **no fragmentation** in both runs; live −0–2.5 MB |
| after a 10 s scroll of big.md | `MDSPIKE_SCROLL=10 MDSPIKE_NOEXIT=1` | 73–117 / 56–87 | live 38–41 vs 29–33; Small 40–78 | +5–10 MB live, up to ~45 MB dirty |
| give free pages back after scan/scroll | `XP_RELIEF=1` (`malloc_zone_pressure_relief`) | 82–109 / 74–111 | released 0.0 MB (probe log) | no effect: fragmentation, not free pages |
| **mimalloc global allocator** | `mimalloc 0.1.52` | **59–75 / 46–117** (6 pairs: scrolled 69, 75 / 117, 73; empty 65, 59 / 62, 57; normal 60, 59 / 69, 46) | tail removed | **worst case −40 MB**, median about the same |
| 2 drawables instead of 3 | not run (Rune constant `FRAME_LATENCY`, needs a Rune patch) | — | −3.67 MB at 1×, −13.7 MiB at 2× (estimate from region sizes) | |
| smaller glyph atlas | not run (Rune constant `ATLAS_SIZE = 2048`) | — | 1024² page: −3 MB GPU −3 MB CPU per page (estimate) | |

Idle CPU, big.md, 20 s after a 12 s settle (`ps -o cputime`): fire at rest **6.0 %** in the run where the window
was active, 0.2 % in the run where it was not (the rest wake-ups stop in an inactive window); `XP_NOREST` 0.1–0.4 %;
fire off 0.0–0.2 %; MDLight 0.1–0.2 %. The 12 Hz rest frames also keep the window from Rune's rest state, which
after 10 s without frames gives back spare layer targets and `El::cache` textures
(`crates/rune/src/window/rest.rs`, up to 32 MiB → 8 MiB); at 1× there were none to give back.

## Scroll

| case (big.md unless noted) | missed refreshes | dt p95 ms | load | source |
|---|---|---|---|---|
| spike binary `rune-md-spike/bench/out/RuneMD.app` | **3.3–5.5 %** (6 runs) | 13.7–15.2 | 14–112 | `MDSPIKE_SCROLL=4`, interleaved with the next rows |
| RuneMD full UI (glass, fire on), paired with the spike | **17–26 %** (6 runs) | 20.4–22.4 | 14–112 | same |
| RuneMD, everything stripped (no fx, opaque, no sidebar, tabs, clip) | 0.5–14.4 % (3 runs) | 11.1–21.4 | 23–87 | `XP_NOFX XP_OPAQUE XP_NOSIDEBAR XP_NOTABS XP_NOCLIP` |
| one part removed at a time: clip / sidebar / tabs | 17–22 / 8–24 / 13–25 % | ~20–22 | 20–111 | each knob alone |
| glass + fire / glass, no fx / opaque + fire / opaque, no fx (900×1000) | 6–25 / 9–28 / 15–28 / 16–25 % | 18–22 | 25–141 | 3 rounds each |
| same at 2400×1400 px | 10–31 / 11–32 / **2.5–21** / **0–25** % | 11–26 | 11–117 | 3 rounds; opaque better in every round |
| fire / fire off / no fx, normal.md | 6–13 / 3–13 / 8–12 % | 13–20 | 242–327 | first matrix, 3 rounds |
| MDLight big / normal (tick intervals) | 1.2–3.0 / 1.2–2.0 % at load 16–97; up to 17 % at load 300 | 12–14 | | `MDBENCH_SCROLL=4` |
| scroll without re-running the view (`tx.live_only()` in the probe) | 4–28 % | 14–23 | 18–317 | `XP_SCROLL_LIVE=1`: inconclusive |

CPU per frame during scroll (`MDSPIKE_FRAMES`, big, 900×1000, load 66): update + encode **p50 2.25, p95 3.35,
max 7.8 ms**; 11 frames with dt > 15 ms, none with CPU > 8 ms. Stripped UI, load 52: p50 2.0, p95 6.2. At
2400×1400: p50 3.9 ms. At load 180–200 the p95 reaches 6–12 ms. Budget at 100 Hz: 10 ms.

What costs per scroll frame:
- **CPU, small**: `Runtime::frame` (handlers, the view: the probe's poll returns `true`, so the whole view runs every
  frame), layout of mounted rows, shaping of rows entering the overscan, `App::visible_rows` (builds a `Vec<u64>` of
  all block ids, 3041 on big, every report). 2–4 ms total; not the cause of the misses.
- **Compositor walk** (`effects_lit()` always true): walks every instance once (O(instances), ~µs) and draws with
  `draw_with`. Consequence that matters more: `draw_with` frames never use Rune's damaged-rect redraw
  (`crates/rune/src/render/damaged.rs`), so hover, caret and the 12 Hz fire-rest frames redraw the whole window.
  During scroll the whole document moves, so this costs nothing extra there.
- **GPU / presentation, the main suspect**: full-window redraw every frame, a transparent window over the system glass
  (WindowServer blends a non-opaque layer), three drawables in flight. Rune exposes no GPU time and the Metal HUD log
  is private-redacted (`MTL_HUD_LOG_ENABLED=1` tried), so the split between these is **not measured**. The pixel-count
  result (2400×1400) and the opaque A/B point there.

Conclusion for "slightly laggier": on Matteo's Retina display the window has ~4× the pixels of these runs; the
2400×1400 rows (10–44 %) are the closest proxy. The CPU side has room; the work is in what each frame makes the GPU
and WindowServer do.

## Ranked fixes

| # | fix | expected gain | risk | effort |
|---|---|---|---|---|
| 1 | **Measure GPU per frame at 2× on an idle Mac** (Instruments "Metal System Trace", window on the Retina panel, `bench/scroll.sh`): split render vs WindowServer compositing; count Rune isolated layers per frame (Rune `frame-profile` feature) | decides fixes 2–4 with data | none | S (½ day) |
| 2 | **mimalloc as global allocator** (`#[global_allocator]`, 2 lines + dep) | worst case 117 → ~70 MB measured; Activity Monitor stops jumping | low: allocator swap; re-run `make check` and the startup bench | XS |
| 3 | **Two drawables instead of three on macOS** (Rune: `FRAME_LATENCY` 1 or an app option) | −13.7 MiB at 2× for 900×1000 pt (estimate), −3.7 MB at 1× | medium: less GPU/CPU overlap can add dropped frames; measure scroll before/after | S in Rune + measure |
| 4 | **Opaque panel path / glass cost**: try the window opaque (or the glass only where it shows) | scroll: opaque better 3/3 at 2400×1400 (0–25 % vs 10–32 %) | visual choice (glass is in DESIGN.md): ask Matteo with a mockup | M |
| 5 | **Index scan without a `PathBuf` per directory entry** (`watch::walk`: one reused buffer, check the name before building the path; ~168k entries walked under `~/Desktop/GitHub`, `find` count) | the no-roots runs had no fragmentation (Small 31–32 for 31 live); expect a good part of the 0–38 MB | low | S |
| 6 | **Fire at rest: slower or stop after N s idle** (now 12 Hz forever while active) | idle CPU 6 % → ~0.2 %; lets Rune rest and give back spare targets (up to 24 MiB at 2×, estimate) | visual: ask Matteo | S |
| 7 | **`effects_lit()` true only with a moving or new effect**, or let Rune damage-redraw around effect slots | hover/caret/rest frames redraw only their rect; no scroll gain | medium: calm edge and fire must still draw | M (Rune API) |
| 8 | **Glyph atlas: drop the CPU mirror after upload, or 1024² pages** (Rune) | −4 to −8 MB CPU, −3 MB GPU per page (estimate) | medium: eviction re-uploads | M (Rune) |
| 9 | **Empty screen: rasterize the wordmark at the drawn width and free the decoded pixels after upload** | −4 MB CPU on the empty screen (measured with `XP_NOIMG`) | low | S |
| — | not worth it | `add_format(Rgba16Float)` (0 MB), fire off for memory (0 MB), ColorSync tables (system, MDLight has them too), threads/ports (< 1 MB) | | |

## Results after fixes

Two of the ranked fixes are on main (16/11). Numbers from the commit messages and the worktree logs; both measured
at 1× (900×1000 px), A/B against the commit before.

| commit | fix (# above) | measured | source |
|---|---|---|---|
| `e624c52` perf(watch) | 5: the index scan and the watcher's rescans check the entry name first and build a `PathBuf` only for a Markdown file they return | one walk of `~/Desktop/GitHub` (7325 files, 21k folders): **57.9 → 11.5 MB allocated, 527k → 372k allocations**, same result (new test against the old walk). Heap waste after the startup scan **unchanged: 34–36 MB either way** (5 A/B pairs); the commit attributes most of it to macOS loading its accessibility bundles, not to the scan | `git show e624c52` |
| `b70de9a` perf(forge) | 9: the empty screen rasterizes the wordmark at the drawn strip's step (200/280/360/440 pt × scale) and uses a 440 px strip PNG when it covers the pixels | live heap **30.6–30.7 → 29.6–29.9 MB** (4 A/B pairs, empty screen and narrow window); GPU-owned unchanged at 9.3 MB (Rune's 1024² image atlas page has the same size whatever the image) | `git show b70de9a`; scratchpad log `work/logs/mem.log` (`f5` = before, `f9` = after) |

Not done yet: mimalloc (#2), two drawables (#3), glass (#4, decision below), fire at rest (#6, decision below), Rune-side
items (#7, #8).

### GPU time per frame (Metal System Trace, 16/11 17:28–17:55)

Fix #1 of the ranked list, measured on the `b70de9a` binary with the `XP_*` knobs: `xctrace record --template 'Metal
System Trace'`, 5 s scroll of big.md at 3000 pt/s, `RUNEMD_ROOTS` an empty folder, external display at 1×, 100 Hz,
load 8–39. GPU busy = union of the app's (or WindowServer's) GPU intervals in the scroll window, from the
`metal-gpu-intervals` table. "Missed" = the app's `scroll frames` probe in the same run. Source: scratchpad log
`work/logs/xt.log` (two `rune-big` runs lost their GPU export; their missed refreshes are listed).

| case | app GPU per frame (p50 / p95) | WindowServer GPU per 10 ms refresh | missed refreshes | runs |
|---|---|---|---|---|
| RuneMD, glass (today), 2400×1400 px | **2.97–3.24 ms** (2.77–3.17 / 3.92–4.38) | **3.75–3.85 ms** | 1.2–9.1 % | 4 (GPU in 2) |
| RuneMD, transparent window but all filled solid ("a1" of that day: system glass hidden under opaque fills), 2400×1400 | 3.50–4.00 ms (3.38–3.79 / 5.15–7.23) | 2.72–3.20 ms | 1.8–5.0 % | 3 |
| RuneMD, opaque window (A2), 2400×1400 | 4.98–5.44 ms (5.11–5.89 / 6.32–7.29) | **1.07–1.83 ms** | 1.0–6.6 % | 3 |
| RuneMD, glass, 1060×864 px | 1.87–1.92 ms (1.91–1.92 / 2.03–2.30) | 1.35–1.78 ms | 0.6–1.4 % | 3 |
| MDLight (its default window) | 0.15–0.18 ms | 0.31–3.47 ms | (tick metric) | 3 |

Reading: at 2400×1400 the frame costs the GPU about 3 ms in RuneMD and 3.8 ms in WindowServer with the glass;
an opaque window cuts WindowServer's share to 1.1–1.8 ms but RuneMD's own GPU time rises to about 5 ms (not
explained yet; the opaque path still draws the same fx compositor). Sum per refresh: glass 6.7–7.1 ms, opaque
6.1–6.8 ms. Both fit the 10 ms budget at low load; MDLight's GPU work is ~20× smaller (CoreAnimation scrolls a
cached layer). These runs were at low load (8–39): the 17–26 % misses of the first matrix (load 14–320) were
mostly load, not GPU.

## Material for decisions (17/11)

Built from `b70de9a` in a throwaway worktree with experiment knobs (appendix). Unless noted, runs on a private
virtual display (`~/.claude/bin/vscreen`, 1920×1200 pt at **2×**, so a real Retina-scale backing store, never shown on
Matteo's screens). Windows on that display are never key: they draw in the inactive style (grey traffic lights,
inactive vibrancy), and RuneMD's "window active" state is forced with `XP_FORCE_ACTIVE` where it matters. Load
(`vm.loadavg`, 1 min) in each row.

### A. Glass vs opaque

Variants (`XP_GLASS`):
- **today**: transparent window, system glass under the whole root, 60 % `glass` tint above it. The document panel is
  already opaque (`theme.panel`, alpha 1.0), so the glass shows behind the sidebar and in the `FRAME` margin around
  the panel.
- **A1**: glass only behind the sidebar (the material and tint move to a sidebar wrapper); the margin around the panel
  is filled with the solid tint (`glass_fallback`). The window stays transparent.
- **A2**: opaque window (`transparent: false`), no system glass anywhere; the root is the solid tint.

Shots (normal.md, 900×1000 pt, same position over a colourful striped backdrop, captured at 2×, cropped and scaled to
1200 px tall):

| | dark | light |
|---|---|---|
| today | `docs/proof/perf-2026-11-16/glass-today-dark.png` | `glass-today-light.png` |
| A1 | `glass-a1-dark.png` | `glass-a1-light.png` |
| A2 | `glass-a2-dark.png` | `glass-a2-light.png` |

What they show: today and A1 look almost the same (the only difference is the thin margin around the panel, which
turns solid in A1); A2 loses the blurred backdrop behind the sidebar. The window is drawn inactive in all six, so
the glass is the inactive variant; an active window shows a slightly brighter, more saturated blur.

Scroll, big.md, 4 s at 3000 pt/s, window 2400×1400 px, 5 rounds alternating today / A1 / A2 (missed refreshes =
`missed / (frames + missed)` from the `scroll frames` probe). Per variant: range, then median.

| setup | today | A1 | A2 | load |
|---|---|---|---|---|
| virtual 2× display, **60 Hz** (1200×700 pt) | 0–13.6 %, median 0.8 % | 0–6.3 %, median 0.4 % | 0.4 % in all 5 | 23–37 |
| virtual 2× display, **120 Hz** (`--hz 120`, probe reports 120) | 2.1–15.8 %, median 7.5 % | 2.3–9.4 %, median 4.8 % | 6.9–13.3 %, median 7.9 % | 13–32 |
| external display at 1×, 100 Hz, window launched in front on Matteo's screen (focus not checked; runs before the virtual display existed) | 16.2–26.8 %, median 23.5 % | 24.2–32.8 %, median 26.5 % | **5.0–9.8 %, median 8.2 %** | 92–169 |

dt p95: 17.7–21.0 ms at 60 Hz, 9.4–16.4 ms at 120 Hz, 15.6–20.8 ms at 1×/100 Hz. Sources: scratchpad logs (not committed)
`glassab/logs/visible-scroll.log`, `visible-scroll-120hz.log`, `scroll.log`; script `scratchpad/bin/run_visible.sh`.

GPU time from 16/11 (section above): opaque cuts WindowServer's GPU from ~3.8 to 1.1–1.8 ms per refresh but raises
RuneMD's own GPU time from ~3.1 to ~5.2 ms per frame, so the sum barely moves.

Reading:
- At Retina scale on the virtual display no variant wins clearly: at 120 Hz A1 has the lowest median (4.8 % vs
  7.5 % today, 7.9 % opaque), but the ranges overlap and five rounds per variant are few.
- The only large gap is the 1×/100 Hz run under heavy load (92–169) with a window launched in front: opaque misses about a
  third as many refreshes as glass. Those windows were most likely key, so their glass was the active material; the virtual
  display can only show the inactive one, which may be cheaper. **Not measured: active glass at 2×.**
- A1 costs the same as today in every setup and looks the same, so it buys nothing by itself.

Options for Matteo:
1. **Keep today's glass**: no visual change; scroll cost at 2× within noise of the alternatives.
2. **A1, glass only behind the sidebar**: looks like today; no measured gain. Only worth it as a step towards a
   per-region choice.
3. **A2, opaque window**: loses the glass (DESIGN.md calls for it); clearly fewer missed refreshes under load at 1×,
   no clear gain at 2× on the virtual display.

What would settle it: the same alternated scroll on the real Retina panel with the window key (needs Matteo's screen
free, or a way to make a virtual-display window key).

### B. Calm fire on the empty screen

Empty screen (no file), fire on, 900×1000 pt window at 2×. %CPU = `ps -o time` delta over 30 s, from 25 s to 55 s
after launch (the freeze variant has been still for 5 s when the window opens), plus `ps -o %cpu` sampled every
second. fps = frames drawn in those 30 s (`MDSPIKE_FRAMES`). Three rounds each, load 7–52.

| variant | knob | active window: CPU (fps) | inactive window: CPU (fps) |
|---|---|---|---|
| today (`REST_FRAME` 83 ms, "12 fps") | — | **4.6–5.4 %** (22.7–24.1 drawn/s) | 0.07–0.23 % (0) |
| ~6 fps | `XP_REST_MS=166` | **2.2–3.5 %** (10.7–18.2 drawn/s) | 0.03–0.13 % (0) |
| moves for 20 s after the last activity, then still | `XP_IDLE_FREEZE=20` | **0.07–0.20 %** once still (0); during the 20 s it runs today's code, so today's cost (not measured separately) | 0.07–0.13 % (0) |

Notes:
- Each rest wake-up draws about two frames (24 drawn/s at a 12/s wake-up; 11–18 at 6/s): the fire frame plus a
  second one not traced here. Halving the wake-up rate cut CPU by about 40 %, not 50 %.
- "Activity" for the freeze knob is a key press or a focus change (`on_key`, `on_window_focus`); a shipped version
  would also count pointer moves and scrolls.
- The inactive window was simulated with `XP_INACTIVE=1` (same code path: `fire_rests` is false). It matches
  16/11's real inactive run (0.2 %).
- Discarded: a first set of virtual-display runs without `XP_FORCE_ACTIVE` (the never-key window flipped
  between still and moving, 0.1–6.6 %), and runs on Matteo's screen whose window lost focus mid-run.

Videos (5 s each, dark, captured at 2×, H.264): `docs/proof/perf-2026-11-16/fire-rest-today-12fps.mp4`,
`fire-rest-6fps.mp4`, and `fire-rest-12fps-vs-6fps.mp4` (wordmark only, today on top, 6 fps below, scaled 2× to
see the steps).

Options for Matteo:
1. **Keep 12 fps**: ~5 % CPU on one core whenever the empty screen is front and idle.
2. **6 fps**: ~2–3.5 %; check in the video whether the flames look choppy.
3. **Move 20 s, then still** (until a key, click or focus change): ~0.1 % at rest, the fire looks the same while
   Matteo is using the app; it also lets Rune rest and give back spare layer targets (16/11, `rest.rs`). Can combine
   with 2.

### Resting fire with a document open

Answers Matteo's question in `docs/decisions/2026-11-17-calm-fire-speed.md`: with a document open the CPU does not go
to zero while "Fire at the bottom of the panel" is on. The document's resting fire costs about twice the empty
screen's wordmark fire.

Same setup as B (`glassab/g.app`, worktree on `b70de9a` with the XP knobs; 900×1000 pt window at 2× on a `vscreen`
display, `XP_FORCE_ACTIVE=1`, %CPU from 25 s to 55 s after launch), with `README.md` open in reading mode. Fire off =
`"fire": false` in a copy of `prefs.json` under `RUNE_DIRS_ROOT` (the real settings were not touched). Runner:
`scratchpad/bin/fire_doc_cpu.sh`, rounds alternated, load 9.7–16.7.

| screen | fire setting | CPU, `ps -o time` over 30 s | `ps -o %cpu` mean (max) | drawn frames/s |
|---|---|---|---|---|
| `README.md` open | on (default) | **8.2–9.9 %** (3 rounds) | 7.6–9.2 % (12.9–18.2 %) | 27.6–30.1 |
| `README.md` open | off | **0.07–0.17 %** (3 rounds) | 0.0–0.1 % (0.4–3.1 %) | 0–0.1 |
| empty screen | on | **4.7 %** (1 round, B: 4.6–5.4 %) | 4.1 % (7.7 %) | 24.6 |

Notes:
- With the fire off the open document is fully idle: no wake-ups, no frames.
- With the fire on it draws about 30 frames/s against about 24 on the empty screen, and each frame is bigger (the
  band spans the panel's width, the wordmark is small), hence about twice the CPU.
- Screenshots of both runs checked (fire band visible at the bottom with the setting on, absent with it off); not
  committed.

## Appendix: experiment knobs (worktree only, not committed)

`XP_NOFX` (`App::effects_lit` false), `XP_NOREST` (`App::fire_rests` false), `XP_NOADDFMT` (skip
`fx.add_format`), `XP_NOIMG` (no `ImageCache`), `XP_SIZE=w,h` (initial window size), `XP_OPAQUE` (non-transparent
window, so no system glass), `XP_NOCLIP` (no `clip_rounded` on the panel), `XP_NOSIDEBAR` / `XP_NOTABS` (empty
placeholders of the same size), `XP_SCROLL_LIVE` (`tx.live_only()` in the scroll probe), `XP_RELIEF`
(`malloc_zone_pressure_relief(NULL, 0)` after the scan and the scroll), `MDSPIKE_NOEXIT` (keep the app after
`MDSPIKE_SCROLL`), and a build with `mimalloc` as `#[global_allocator]`.

Added 17/11 (worktree on `b70de9a`): `XP_GLASS=a1|a2` (glass only behind the sidebar / opaque window), `XP_POS=x,y`
(window origin), `XP_REST_MS` (rest wake-up period in place of `REST_FRAME`), `XP_IDLE_FREEZE=secs` (the calm fire
stops that long after the last key or focus change), `XP_INACTIVE` (start with `window_active` false),
`XP_FORCE_ACTIVE` (focus events never clear `window_active`). Runner: `scratchpad/bin/run_visible.sh`
(`shots|scroll|cpu|video`, `HZ=120` for the 120 Hz display), on a `vscreen` display it creates and removes.

Limits: no 2× screen (all 2× numbers are estimates from measured 1× sizes); load 11–320 and 9 GB swap (footprint
counts compressed pages, so totals swing; categories and live heap are stable); MDLight's scroll metric is not the
same as RuneMD's; GPU time not measured.

## Release pass 2026-11-17

Branch `perf/release-pass` from `b967ca7`. All runs on a private virtual display (`~/.claude/bin/vscreen create
--width 1920 --height 1200 --hidpi --hz 120`): 2× backing store, 120 Hz, window at its default 900×1000 pt (1800×2000
px), never key (inactive glass). `RUNEMD_ROOTS` an empty folder, `RUNE_DIRS_ROOT` a scratch folder (Matteo's settings
untouched), `RUNEMD_THEME=dark`. Builds: `cargo build --profile perf`, bundled like `make app` but ad-hoc signed.
Load (`vm.loadavg`, 1 min) in each row.

### Scroll: the resting fire ran the whole view on every frame

Cause. While the bottom fire rests (5 s after a file opens with the default Brief mode, or always with Always), `poll`
returned `true` on every call, so Rune committed the whole view on every frame of a scroll: ~0.9 ms view + ~1.4 ms
commit on big.md, 2.0 ms of the 2.6 ms frame. The `tx.live_only()` meant to avoid it has no effect inside `poll`: Rune
reads it only in input and layout handlers (`crates/rune/src/runtime/frame.rs:513`, `live_only_tests.rs:152`).

Fix (`691b805`). `poll` commits for the resting fire only on the `REST_FRAME` wake-up of a resting window
(`App::rest_wake`: at least ¾ of `REST_FRAME` since the previous poll); inside a stream of frames the compositor
already moves the flames on each frame's clock. The empty screen's fire is unchanged. Test:
`the_resting_fire_does_not_run_the_view_on_every_frame` (147 commits before the fix, 87 after).

The scripted scroll changed too (`76bf06a`): it returned `true` from `poll` on every frame, so it always ran the view,
unlike a wheel. It now returns `poll`'s own result, like a wheel scroll (`MDSPIKE_SCROLL_VIEW=1` restores the old
probe). "Before" below is `b967ca7` with the new probe, so both sides scroll the same way.

Command (one run; `MDSPIKE_LOG` gets the `scroll frames` line, `MDSPIKE_FRAMES` one line per frame):

```sh
~/.claude/bin/vscreen launch --name agent-screen --env "MDSPIKE_LOG=$LOG,MDSPIKE_FRAMES=$FR,MDSPIKE_SCROLL=4,MDSPIKE_SPEED=3000,RUNEMD_ROOTS=$EMPTY,RUNE_DIRS_ROOT=$DIRS,RUNEMD_THEME=dark" -- RuneMD.app fixtures/big.md
```

big.md, 4 s at 3000 pt/s, 460–478 frames per run. CPU = `update_ms + encode_ms` per drawn frame; missed = refreshes
with no new frame (intervals over 1.5 periods); dt = interval between drawn frames.

| load | build | CPU p50 / p95 / p99 / max (ms) | dt p95 / p99 (ms) | missed refreshes |
|---|---|---|---|---|
| 15–16 | before | 1.90–1.92 / 2.45–2.51 / 2.85–2.87 / 3.1–3.5 | 9.7–10.3 / 10.1–16.3 | 15, 3 |
| 13–16 | after | 0.65–0.68 / 0.86–2.56 / 3.2–4.5 / 7.4–7.6 | 10.0–11.9 / 11.7–16.3 | 15, 19, 3 |
| ~37 | before | 2.00–2.01 / 2.60–2.61 / 2.85–3.03 / — | 9.6–10.4 / 10.5–16.2 | 3, 17 |
| ~37 | after | 0.67–0.68 / 0.96–2.11 / 3.36–3.71 / — | 10.3–11.0 / 12.5–14.0 | 5, 10 |
| ~95 | before | 1.96–1.97 / — / — / — | — | 26, 19 |
| ~95 | after | 0.68–0.69 / — / — / — | — | 11, 12 |

Reading:
- CPU per frame: **p50 2.0 → 0.67 ms** in every run. With Rune's `frame-profile` (`cargo build --features
  frame-profile`, `RUNE_FRAME_LOG`), after the fix view and commit are 0 in the median frame (build p50 0.54 ms).
- Missed refreshes: fewer under heavy load (26/19 → 11/12 at ~95); at low load within noise (3–17 before, 3–19 after).
  The misses left are late frame starts (the main thread starts the frame late; drawable wait p50 0.05 ms), not CPU.
- The tail moved: about 1 frame in 35 now runs the view, when rows enter the virtual list's overscan, and shapes
  5–158 rows at once (2.6–5 ms at load 13, up to 7.6 ms at load 15). Before, the view ran on every frame and rows
  entered a few at a time. This is the same path a wheel scroll already took outside the fire's 5 s; the overscan is a
  Rune constant (`crates/rune/src/el/virtual_list.rs:181`, `OVERSCAN = 2.0`).
- Dropped: a `Live` value toggled on every rest frame (a resting window draws only for a poll that changed something,
  so the fire stopped) and `tx.wake_after(0)` (43–110 frames/s at rest instead of 12).

GPU: not measured this pass (no xctrace). Nothing in the fix adds GPU work; frames drawn are the same.

### Memory

Command (one process: empty screen at launch, big.md opened at 12 s, 10 s scroll from 25 s; samples at 10, 22 and 45 s;
`MDSPIKE_OPEN_AT` and `MDSPIKE_NOEXIT` added in `0585344`):

```sh
~/.claude/bin/vscreen launch --name agent-screen --env "MDSPIKE_LOG=$LOG,MDSPIKE_OPEN_AT=12:$PWD/fixtures/big.md,MDSPIKE_SCROLL=10,MDSPIKE_DELAY=25,MDSPIKE_NOEXIT=1,RUNEMD_ROOTS=$EMPTY,RUNE_DIRS_ROOT=$DIRS,RUNEMD_THEME=dark" -- RuneMD.app
footprint -p $PID          # phys_footprint and categories
ps -o rss= -p $PID         # RSS
vmmap --summary $PID       # zones and fragmentation
heap -s $PID               # live heap ("All zones: … bytes"); MallocStackLogging=lite for allocation sites
```

Before = `0585344` (fire fix and probe, no memory change), after = `e4d9467`. Three runs each, load 6–20.

| state | phys_footprint before | after | RSS before | after | live heap before → after |
|---|---|---|---|---|---|
| launch, empty screen | 122, 139, 122 MB | 138, 124, 137 MB | 132–140 MB | 161–163 MB | 24.2 → 24.1 MB |
| big.md (1 MB) open | 124, 137, 138 MB | 136, 124, 137 MB | 134–153 MB | 168–170 MB | **32.4 → 29.9 MB** |
| after a 10 s scroll | 159, 157, 159 MB | 155, 157, 156 MB | 142–163 MB | 136–181 MB | 41.8 → 39.3 MB |

RSS counts clean shared pages (fonts, system libraries) and moves with what the system keeps resident; it varies more
than the footprint and says little about the app.

Fix (`e4d9467`): the empty screen's wordmark handles stayed in `App::forge_images` after a file opened, so the image
cache kept their decoded pixels and mips (1.8 MB of mips plus the decoded strip). The view now drops them while a tab
is in front; the empty screen decodes them again when it comes back. Live heap −2.5 MB; the total footprint swings
±15 MB between launches on its own, so the gain does not show there. Test: `opening_a_file_lets_the_wordmark_images_go`.

Where the ~157 MB after a scroll go (footprint categories, typical run):

| category | MB | what | lever |
|---|---|---|---|
| Malloc Small | 72–76 | DefaultMallocZone: 34–37 MB allocated, **38–44 MB fragmentation** (`vmmap`), already 40 MB at launch with 21 MB allocated | not from Rust code: with mimalloc (below) the system zone keeps 51 MB |
| IOSurface | 28–43 | 2–3 drawables × 14.4 MB (1800×2000 BGRA) | Rune: `FRAME_LATENCY` (`crates/rune/src/window.rs:694`), set on every `configure` (`gpu/metal/surface.rs:112`) |
| GPU-owned (graphics) | 10–11 | glyph atlas and image atlas pages, buffers | Rune constants |
| Malloc Large | 4–7 | glyph atlas CPU mirror (4.2 MB), image atlas first page (4.2 MB, kept by design: `image/pixels.rs:325`) | Rune |
| live growth while scrolling | +9 | Rune vectors +2.9, parley layouts +1.4, accesskit +0.6, app vectors +1.2 (`heap -s` diff) | mostly Rune |

Tried and dropped:
- **mimalloc** as `#[global_allocator]` (3 interleaved pairs): 155–157 MB without, 159–161 MB with after the scroll.
  Malloc Small drops 74 → 51 MB, but mimalloc's own segments add 33 MB (shown by `footprint` as "IOAccelerator"). The
  fragmentation sits in the system frameworks' allocations, not in Rust's. (16/11 at 1× it cut the worst case; at 2×
  with no index scan it gives nothing.)
- **Two drawables** from the app: Rune sets `maximumDrawableCount` on every surface configure (resize included), and
  with `allowsNextDrawableTimeout(false)` a third frame in flight would block the main thread in `nextDrawable`. Needs
  a Rune option and a scroll A/B; −14.4 MB at this window size (estimate from the region size).
- Waiting at rest: 30 s more without frames after the scroll gives back 6 MB (163 → 157 MB, one run).

MDLight: no build found on this Mac (`_vendor/md-light-bench/build/MDLight.app` not found), so no 2× numbers; 16/11 at 1×: 86–113 MB with big.md.

### Idle CPU

`ps -o time` delta from 8 to 18 s after launch, same display. Before = `b967ca7`, after = `e4d9467`.

| screen | before | after |
|---|---|---|
| big.md open (the Brief fire ends at 5 s) | 0.10 % | 0.10 % |
| empty screen (wordmark fire, by design) | 4.50 % | 4.40 % |

The window on the virtual display is never key. With a document open the 1–5 s rest frames are unchanged: 47 frames in 4 s, all running the view, CPU
p50 1.55 → 1.50 ms.

## Moving metal 2026-11-18

Every metal edge (calm tab, ⌘P panel, writing rings) is the moving chrome (docs/decisions/2026-11-18-moving-metal-edges.md, on
main). Method: `after` = branch `feat/moving-metal-edges`, `before` = `release/2026-11-18` plus the same probes, both
`--profile perf`, ad-hoc signed bundles, on a private 120 Hz virtual display (1920×1200 pt, 2×), `fixtures/big.md`, dark
theme, empty `RUNEMD_ROOTS`, fresh `RUNE_DIRS_ROOT` (fire mode Brief, the default), `MDSPIKE_ACTIVE=1` (the window counts
as active: virtual-screen windows are never key). Idle: the app's PID only, 10–70 s after launch (the Brief fire has
gone), `cpu_time` = `ps -o time` delta / 60 s, `ps_mean` = 60 samples of `ps -o %cpu`, frames/s from `MDSPIKE_FRAMES`
(`drawn=1`). Scroll: `MDSPIKE_SCROLL=4 MDSPIKE_SPEED=3000`, the `scroll frames` line. Run only while `pgrep -x cargo` and
`pgrep rustc` were empty; another agent's app was open, so the load average (column) varies.

| Idle, 60 s | cpu_time | ps_mean | frames/s | load |
|---|---|---|---|---|
| before r1 / r2 / r3 | 0.50 / 0.07 / 0.08 % | 0.40 / 0.02 / 0.03 % | 1.2 / 0 / 0 | 38 / 11 / 15 |
| after r1 / r2 / r3 | 2.37 / 2.18 / 1.93 % | 1.98 / 1.85 / 1.62 % | 16.1 / 16.1 / 16.0 | 17 / 22 / 15 |

| Scroll, ~4 s at 120 Hz | frames | dt p50 / p95 / p99 ms | missed refreshes | frame CPU p50 / p95 / p99 ms |
|---|---|---|---|---|
| before r1 / r2 / r3 | 348 / 464 / 473 | 8.34 / 9.28 / 12.99 · 8.34 / 11.37 / 13.75 · 8.33 / 9.40 / 13.34 | 115 / 15 / 6 | 0.71 / 1.09 / 3.84 · 0.71 / 1.77 / 3.98 · 0.65 / 0.87 / 3.61 |
| after r1 / r2 / r3 | 444 / 470 / 466 | 8.38 / 13.78 / 16.24 · 8.33 / 9.97 / 13.56 · 8.32 / 9.39 / 12.18 | 34 / 9 / 5 | 0.91 / 3.58 / 7.34 · 0.68 / 1.11 / 3.54 · 0.65 / 0.98 / 3.58 |

Scrolling is the same (r1 of both ran under load 17–38); the moving metal costs ~2 % at rest.

How it moves without the display link and without running the view:
- First version: the calm edge as an animating layer holds the display link: 120 frames/s, **14.5 %** idle. Dropped.
- Now: the compositor draws calm edges, and writing rings past their 220 ms entrance, as paced layers, and stamps
  `App::metal_drawn` when it draws moving metal. `poll`, when nothing else changed and the metal moves
  (`App::metal_moves`: window active, no Reduce Motion, a tab, ⌘P or a ring), asks `wake_after(ZERO)` once
  `ring::METAL_FRAME` (125 ms) has passed since the stamp, or `wake_at` the step otherwise (`App::metal_due`). A zero wake
  on a resting window draws a frame without a commit (Rune `resting_wake`), so the view does not run.
- Rune draws a second frame ~8 ms after each such frame: the zero wake is scheduled again from the poll after the frame
  it asked for (`frame_polled`), so 8 steps/s are 16 frames/s. The app cannot ask for exactly one frame (only a commit
  does, and a commit runs the view). Measured idle by step rate (one run each): 12/s (`fire::REST_FRAME`) 2.85–4.0 %,
  8/s 2.2 %, 6/s 1.9 %. 8/s is the slowest that does not look choppy, at the ~2 % mark.
- Reduce Motion or an inactive window: every edge is the still pose and `metal_moves` is false, so no wake-ups
  (test `an_inactive_window_and_reduce_motion_keep_the_metal_still`). The fire at rest is unchanged ("Release pass
  2026-11-17").

## Tab outline 2026-11-18

One metal outline round the front tab and the page (`fx::outline`, branch `feat/release-small-choices`). Method as
"Moving metal 2026-11-18": `before` = a1420a2 (moving metal, VS Code tabs, ring on the front tab only), `after` =
be7dea2, both `make app` (`--profile perf`), private 120 Hz virtual display 1920×1200 pt 2×, `fixtures/big.md`, dark,
empty `RUNEMD_ROOTS`, fresh `RUNE_DIRS_ROOT`, `MDSPIKE_ACTIVE=1`; idle 10–70 s after launch, runs alternated, no
cargo/rustc running; another agent's virtual display was up, so the load varies.

| Idle, 60 s | cpu_time | ps_mean | frames/s | load |
|---|---|---|---|---|
| before r1 / r2 / r3 | 2.87 / 2.67 / 3.28 % | 2.39 / 2.31 / 3.09 % | 15.8 / 15.8 / 15.9 | 19.5 / 11.2 / 7.2 |
| after r1 / r2 / r3 | 2.92 / 3.22 / 3.07 % | 2.39 / 2.97 / 2.84 % | 15.9 / 15.9 / 15.8 | 16.7 / 7.7 / 6.2 |

Means: before 2.94 % cpu_time, after 3.07 % (+0.13 points, inside the run-to-run spread of 0.6). Frames/s are the same
(8 steps/s, 16 frames): the outline adds 5 more metal layers to the same paced frames, no extra wake-ups.
