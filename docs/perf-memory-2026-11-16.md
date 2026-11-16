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

## Appendix: experiment knobs (worktree only, not committed)

`XP_NOFX` (`App::effects_lit` false), `XP_NOREST` (`App::fire_rests` false), `XP_NOADDFMT` (skip
`fx.add_format`), `XP_NOIMG` (no `ImageCache`), `XP_SIZE=w,h` (initial window size), `XP_OPAQUE` (non-transparent
window, so no system glass), `XP_NOCLIP` (no `clip_rounded` on the panel), `XP_NOSIDEBAR` / `XP_NOTABS` (empty
placeholders of the same size), `XP_SCROLL_LIVE` (`tx.live_only()` in the scroll probe), `XP_RELIEF`
(`malloc_zone_pressure_relief(NULL, 0)` after the scan and the scroll), `MDSPIKE_NOEXIT` (keep the app after
`MDSPIKE_SCROLL`), and a build with `mimalloc` as `#[global_allocator]`.

Limits: no 2× screen (all 2× numbers are estimates from measured 1× sizes); load 11–320 and 9 GB swap (footprint
counts compressed pages, so totals swing; categories and live heap are stable); MDLight's scroll metric is not the
same as RuneMD's; GPU time not measured.
