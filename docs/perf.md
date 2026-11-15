# v1: all thresholds (task 28)

14/11 15:56–15:59, `make app` at `7698fa5`, Mac unlocked with only one screen on (external 3440×1440 at 100 Hz,
the same as the 13/11 measurements; the Retina screen was off), Matteo's apps open (Spotify, browser, T3 Code).
The load average did not go below 3: after waiting 20 minutes (minimum 6.9) we measured anyway. Each row has
the 1-minute load average (`sysctl vm.loadavg`) before and after the run. Compositor, sidebar and watcher on;
the real app uses the watched folders from the settings (`~/Desktop/GitHub`, 6986 files in the index). WORKDIR
`/tmp/runemd-bench` with `build/RuneMD.app`, `fixtures/normal.md`, `fixtures/big.md`. Spike: numbers from
`rune-md-spike/docs/spike/2026-11-13-rune-vs-md-light.md`, idle machine.

| threshold | command | load | v1 | spike | met |
|---|---|---|---|---|---|
| warm startup ≤ 350 ms (p50), normal | `bench/startup.sh /tmp/runemd-bench 5 warm` | 7.1 → 19.3 | p50 **293** [280–1394] | 302 [269–953] | yes |
| warm startup ≤ 350 ms (p50), big | same | same | p50 **297** [281–330] | 295 [279–320] | yes |
| sidebar filled ≤ 1 s after the first frame | `RUNE_DIRS_ROOT=<tmp> MDSPIKE_LOG=… runemd normal.md`, 4 launches, the first without cache | 17.6 → 18.0 | with the cache: 6986 entries read before the first frame (3.0–3.4 ms), Recent full **at the first frame**; scan done 1.0–1.9 s later. Without cache: scan 0.84 s after the first frame | — (the spike had no sidebar) | yes |
| dropped refreshes while scrolling ≤ 3 %, normal | `bench/scroll.sh /tmp/runemd-bench 3` | 17.3 → 18.6 | **8.6–11.5 %**; dt p95 19.2–19.4 ms | 2.4 % | **no** |
| dropped refreshes while scrolling ≤ 3 %, big | same | same | **12.5–14.3 %**; dt p95 19.5–19.7 ms | 2.6 % | **no** |
| idle memory on big ≤ 100 MB | `bench/memory.sh /tmp/runemd-bench 3` | 19.3 → 17.2 | **87–90 MB** (normal 68–89) | 78 [73–97] | yes |
| write → frame ≤ 150 ms | real window, `MDSPIKE_LATENCY`, 25 appends to a copy of big.md every 200 ms | 17.6 → 17.2 | p50 90.4, **p95 105.6**, max 110.1 (25 samples) | — | yes |
| edit p95 ≤ 8 ms per frame on big | `bench/edit_big.sh` (headless) | 17.2 | p50 1.43, **p95 2.42**, max 4.15 | p95 5.6 (6 KB paragraph) | yes |

Dropped refresh percentage as in the 13/11 measurements: `missed_refreshes / (frames + missed_refreshes)` per
run, from the `scroll_summary.log` rows (same formula: the 13/11 runs give 1.3–4.3 % and 4.0–6.5 %).

**Scroll: threshold missed.** On 13/11 (task 2), on the same 100 Hz screen and with load average 72–79, the same
runs gave 3.0 % and 5.3 %; today 9.8 % and 13.2 % (averages), with dt p95 of 19 ms against 13–16. Frame CPU time
does not explain it: `bench/outline_scroll.sh` (headless, big, 1280×800, same moment, load 17.2) gives p50 0.85, p95
4.16, max 15.3 ms, under the 10 ms period. Hypothesis, not verified: the cost is in presentation (GPU and
WindowServer, at 43 % CPU in the 15:35 `ps` sample), for example the system glass (`El::system_material`, since `320df94`) or the
load of other processes. In the A/B below the binary from before the glass drops the same refreshes. To measure again on an idle machine.

## Scroll: A/B with `c727ff7` (before the glass), same load

14/11 16:06–16:09. Binary of `c727ff7` (the last commit without `El::system_material`) built in a temporary
worktree, bundled with the assets of its tree and signed like `make app`. `bench/scroll.sh <dir> 1`
alternated between HEAD (the app in `/tmp/runemd-bench`, code of `7698fa5`: the commits after it only change documents) and `c727ff7`, 3 rounds (each round: normal then big), so the two binaries see the
same load. 1-minute load average before each run: 81 / 71, 63 / 56, 51 / 44 (HEAD / `c727ff7`), much
higher than the 15:56 measurements (at that moment a Chrome GPU process was at 344 % CPU).

| file | build | dropped refreshes per run | average | dt p95 |
|---|---|---|---|---|
| normal | HEAD | 6.8 / 8.6 / 9.0 % | 8.1 % | 15.9–17.0 ms |
| normal | `c727ff7` | 7.3 / 14.0 / 7.5 % | 9.6 % | 16.1–18.0 ms |
| big | HEAD | 13.6 / 21.4 / 7.6 % | 14.2 % | 16.2–20.9 ms |
| big | `c727ff7` | 16.5 / 19.1 / 8.8 % | 14.8 % | 16.2–19.4 ms |

What the numbers say: under the same load the two binaries overlap run by run; the glass and the rest
of the work after `c727ff7` do not explain today's dropped refreshes. What they do not say: why today's runs are
worse than on 13/11 (3.0 % and 5.3 % with the task 2 binary). Today's load is different (GPU and audio processes
instead of CPU only), but `load average` does not measure it; the threshold is still to verify on an idle machine.

# Perf: rune-fx compositor at idle (task 2)

`make app` build (perf profile), WORKDIR `/tmp/runemd-bench` (outside Desktop: TCC), Mac with the screen locked.

## Startup, warm, 5 runs (`bench/startup.sh /tmp/runemd-bench 5 warm`)

| file | min | p50 | max | threshold | task 1 baseline (p50) |
|---|---|---|---|---|---|
| normal | 225 | 236 | 251 | 350 | 244 |
| big | 222 | 245 | 292 | 350 | 242 |

Threshold met: no measurable cost (the rune-fx renderer is created on the first frame, no pipeline is compiled without layers).

## Scroll

| test | status |
|---|---|
| scroll, 13/11 23:50, load average 72–79 (other sessions active) | dropped refreshes: normal 3.0 % [1.3–4.3], big 5.3 % [4.0–6.5]; dt p95 normal 12.9–14.6 ms, big 14.2–16.1 ms (6 runs per file, `bench/results/scroll-2026-11-13-sotto-carico.log`) |
| scroll, threshold | **not valid**: measured under load; the spike gave 2.4 % / 2.6 % on an idle machine. To repeat at low load average before the end of phase 2 (same command: `make app && mkdir -p /tmp/runemd-bench && cp -R build/RuneMD.app fixtures/normal.md fixtures/big.md /tmp/runemd-bench/ && bench/scroll.sh /tmp/runemd-bench 3`) |

With the screen locked (`CGSSessionScreenIsLocked = true`) the window is occluded and every frame is `Skipped`
(`frames=0`), even without the compositor. At idle `FxCompositor::render` with no layers calls `Frame::draw_ui`
(Rune's default frame); `animating()` and `draws_own()` are false.

## Choices

- Tab rings start at 100 (not 1): ids 1 and 2 belong to the fire and the caret (formerly the dither).
- No warm-up thread: rune-fx does not expose a warm-up per kind; with no layers no pipeline is compiled.
  Tasks 11-13 decide whether to warm up on the first `set(Some)`.

# Reload of the open file (task 10)

## Cost of a reload of `fixtures/big.md` (1 MB)

`cargo test --profile perf reload_cost -- --ignored --nocapture` (`src/live/reload.rs`, 40 rounds per case, Mac with
other processes active: the p95 values swing between runs; here the quietest of three runs).

| case | all on the UI thread p50 / p95 | UI thread after the fix (snapshot + commit) p50 / p95 | separate thread (`read`) p50 / p95 |
|---|---|---|---|
| append of one paragraph | 4.96 / 19.23 ms | 0.03 / 0.17 ms | 6.12 / 13.04 ms |
| every block changed (worst Myers) | 13.42 / 26.15 ms | 0.03 / 0.24 ms | 13.96 / 25.82 ms |

The 8 ms p95 threshold was exceeded in all runs: reading, UTF-8, hashing, block splitting and diff (`ids::assign`)
run on a separate thread (`live::reload::read`); on the UI thread only `Document::commit` remains, which replaces the
blocks if the revision is still the snapshot's (otherwise it reads again). The thread wakes the app with the `Waker`.

## Write → frame (real window)

13/11 23:48, screen unlocked, `MDSPIKE_LATENCY`, 25 appends to big.md every 200 ms (24 samples): p50 **91.6 ms**,
p95 106.7 ms, max 106.7 ms. Threshold ≤ 150 ms met (same load as the scroll measurement).

# Markdown index (task 14)

14/11 01:05, Mac with other sessions active: **load average 95–110** (`uptime` before and after). `~/Desktop/GitHub`:
37 entries, 29 with `.git`.

## Scan and cache

`cargo test --profile perf scan_cost -- --ignored --nocapture` (`src/index/mod.rs`, 5 rounds, cache in a temporary
folder).

| round | entries | scan | cache save | cache reload | `recent(20)` |
|---|---|---|---|---|---|
| 1 (closest to cold: disk page cache not fully warm) | 6916 | 2801 ms | 11.2 ms | 4.4 ms | 0.04 ms |
| 2–5 | 6916 | 574–624 ms | 6.0–9.8 ms | 2.2–3.1 ms | 0.02 ms |

Cache: 1059 KB of JSON. "Cold" here means without the index cache; the disk cache does not empty without
`sudo purge`. Entries in hidden folders (they are included, spec "Watching files"): `.worktrees` 1299, `.claude` 587,
`.superpowers` 483, `.repos` 388, `.agents` 27, `.build` 26, `.venv` 16, others ≤ 4.

## Index ready compared with the first frame (real app)

`RUNE_DIRS_ROOT=<tmp>/dirs MDSPIKE_LOG=<tmp>/runN.log build/RuneMD.app/Contents/MacOS/runemd fixtures/normal.md`
launched from the terminal (same TCC permissions), killed after 5 s; 4 rounds, the first without cache.

| round | cache read before the first frame | entries from the cache | scan done after the first frame |
|---|---|---|---|
| 1 (without cache) | 0.14 ms | 0 | 1710 ms |
| 2 | 3.13 ms | 6916 | 1668 ms |
| 3 | 3.22 ms | 6916 | 1548 ms |
| 4 | 3.01 ms | 6916 | 1579 ms |

With the cache the index is already full at the first frame (the cache is read in `init`, 3 ms): the threshold "sidebar filled ≤ 1 s
after the first frame with the saved index" is met for the data; the sidebar that shows them comes with task 16. Without
cache the index arrives with the scan, 1.5–1.7 s after the first frame at this load (that is not the threshold, which applies with
the saved index). Startup to the first frame (`bench/startup.sh`) not measured again: at load average 100 the number is not valid.

## UI thread after fix round 1 (rescan and moved folders)

14/11 01:18, **load average 92** (`uptime`), `~/Desktop/GitHub`, 6918 entries.

| measurement | command | p50 | p95 | max |
|---|---|---|---|---|
| one watcher rescan (`Core::rescan` + `drain`, recursive root) | `cargo test --profile perf rescan_ui_cost -- --ignored --nocapture` (40 rounds) | 0.000 ms | 0.001 ms | 0.014 ms |
| pickup of the whole scan (`Index::finish_scan`) | `cargo test --profile perf scan_pickup_cost -- --ignored --nocapture` (40 rounds) | 1.40 ms | 1.63 ms | 1.73 ms |
| copy of the entries for saving (`Library::save`) | same command | 0.14 ms | 0.32 ms | 0.56 ms |

Before the fix the rescan walked `~/Desktop/GitHub` on the UI thread (≈ 0.6 s warm, 2.8 s cold, plus
≈ 6900 stats in the watcher and as many in the index). Now a recursive root becomes an `FsEvent::Rescan` and the
scan runs on the `runemd-index` thread; sorting and dedup happen there, the UI thread only merges. Real
app after the fix (same command as above, 2 rounds): cache 3.08 ms before the first frame with 6919 entries; scan
done 1.1–1.2 s after the first frame.

# Source editing (task 22)

`bench/edit_big.sh` (headless, `bench/edit_big.json`): Cmd+E, scroll to the middle of the source, one click for the cursor,
200 characters one per frame at 20 fps; p95 of CPU time per frame (`quality.cpu_ms` from `--trace`) between the
`typing` and `typed` marks. Mac with other sessions active; the load average is the one from `uptime` at the start of each run.

| source | file | load average | p50 | p95 | max | threshold |
|---|---|---|---|---|---|---|
| one `text_input` for the whole file (task 20) | big | 14.9 | 147.0 | **149.4** | 181.4 | 8 ms: no |
| one `text_input` per piece in `virtual_rows` (plan B) | big | 16.5 (the lowest we had) | 1.26 | **1.57** | 1.79 | 8 ms: yes |
| plan B, 3 more runs | big | 36–37 | 1.20–1.24 | 1.47–1.53 | 1.74–4.07 | 8 ms: yes |
| plan B | normal | 36.1 | 0.89 | 1.10 | 2.97 | — |

Without GPU or encode (`cargo test --profile perf source_typing_cost -- --ignored --nocapture`, one key and one
`Runtime::frame`, load average 21): big p50 0.56 / p95 0.87 ms (task 20: 148.9 / 164.3 ms), normal 0.57 / 0.80 ms.

Scrolling in the source of big with the field focused (the view reads `Cx::caret` and runs again on every scroll frame): 240
frames at 60 fps, p50 0.39 / p95 1.22 / max 2.35 ms (load average ~20, ad hoc script, not in the repo).

# Find in file, Cmd+F (task 24)

`cargo test --profile perf find_cost -- --ignored --nocapture` (`src/ui/find.rs`): `fixtures/big.md` (1 MB, 3041
blocks) on the headless runtime with the app's view; 10 searches typed letter by letter, 3 rounds (164 keys,
the first key excluded). 14/11, three runs, load average 19–26.

Before, with the search on the UI thread: `markdown::find` over all blocks costs ~5 ms per MB even with no
results ("zzzq": 5.3 ms), ~10 ms for "e" (97,692 results); `Find::search` alone p50 6.2 / p95 8.7 ms,
key + frame p95 12.5 ms. Threshold (8 ms) exceeded. That is why the search runs on a separate thread.

| measurement | p50 | p95 | max |
|---|---|---|---|
| key + frame (field, request to the thread) | 0.68–0.72 | 1.27–1.30 | 1.7–2.2 |
| results arrived + frame (marks, view to the active one) | 1.32–1.35 | 2.69–2.98 | 13.3–13.5 |
| UI thread per key, the worse of the two | 1.32–1.35 | **2.69–2.98** | 13.3–13.5 |
| search thread response | 5.6 | 8.2–8.6 | 10–12.4 |
| reference: view jump to an unmounted row, without Find | 6.6–6.9 | 10.6–11.1 | 11.6–25.3 |

The first key parses all blocks on the search thread (15–17 ms, off the UI thread). The maximums
above 8 ms are view jumps to a distant result: they cost as much as any jump on big.md (last row).

After fix round 1 (`line` ring on the results, queued steps, results tied to the document), 14/11 13:00, load
average 34: key + frame p95 1.32 ms; results + frame p95 2.84 ms (max 13.4, one jump); UI thread per key
**p95 2.84 ms**; thread response p95 8.4 ms.

# Heading outline (task 25)

14/11, Mac with other sessions active, load average 8–12 (`uptime` printed by each run).

## Scrolling big.md, before and after

`bench/outline_scroll.sh` (headless, `tests/scripts/outline_scroll.json`): `fixtures/big.md` (1 MB, 3041 blocks,
435 headings), 1280×800 window (the outline is visible), 16 swipes of 4000 pt every 0.5 s on `doc`, 60 fps; CPU time
per frame (`quality.cpu_ms` from `--trace`) between the `scrolling` and `scrolled` marks, 504 frames per run. "Before" is the
binary of `247c1ce` (`bench/outline_scroll.sh DIR <binary>`), same window.

| window | build | p50 | p95 | max |
|---|---|---|---|---|
| 1280×800 | before (3 runs) | 0.41–0.42 | 2.42–2.45 | 3.48–3.78 |
| 1280×800 | after, outline visible (3 runs) | 0.59 | **2.84–2.86** | 4.02–4.20 |
| 900×800 | before (2 runs) | 0.40 | 2.27–2.33 | 3.59–3.64 |
| 900×800 | after, outline hidden (2 runs) | 0.41–0.47 | 2.19–2.45 | 3.66–4.81 |

With the outline visible, scrolling costs ~0.17 ms more per frame (p50) and ~0.4 ms at p95: those are the ~120 rows that
Rune keeps mounted in the outline's virtual list (the view plus 2 views of overscan per side). Tests done: with
the column without rows the cost goes back to "before" (p50 0.43–0.45, p95 2.45–2.52); neither a `memo` per row nor rows
without hover and press change the number, nor does removing the placeholder that keeps the highlighted one in view.
The frame stays under 3 ms at p95 (8.3 ms budget at 120 Hz). With the narrow window nothing changes.

Fix round 1 (tree stable across the threshold, "at the bottom" from the page's `on_layout` with `live_only`,
headings computed only with the outline visible), same command, load average 17–21:

| window | build | p50 | p95 | max |
|---|---|---|---|---|
| 1280×800 | before (3 runs) | 0.43–0.44 | 2.39–2.51 | 3.38–3.57 |
| 1280×800 | after the fix (3 runs) | 0.60–0.64 | 2.80–3.00 | 4.21–5.12 |
| 900×800 | before (2 runs) | 0.42 | 2.32–2.36 | 3.55–3.74 |
| 900×800 | after the fix (2 runs) | 0.42–0.43 | 2.40–2.43 | 3.71–3.89 |

As before the fix: the cost is still the mounted outline rows; the page handler, called on
every scroll frame, does not show in the numbers.

## Headings on open and on every reload

`cargo test --profile perf outline_cost -- --ignored --nocapture` (`src/ui/outline.rs`, 20 rounds): the scan
runs once per document and per reload (`outline::Cache`, key `doc_view::rows_version`), on the UI
thread on the first view that asks for it.

| file | blocks | headings | p50 | max |
|---|---|---|---|---|
| big.md, 1 MB | 3041 | 435 | 0.10 ms | 0.13 ms |
| big.md repeated, 16 MB | 48,656 | 6960 | 1.81 ms | 2.34 ms |

Before the setext filter from the bottom and the ATX case without a parser, the 16 MB scan cost 4.7 ms (p50).

## Mermaid (task 26)

The rune-kit `mermaid` feature was already in `Cargo.toml` since task 1 (`perf` binary 33.26 MB with and without task 26: 33,264,560 against 33,264,032 bytes). The task changes the style (diagram colors from the tokens) and adds no rune-mermaid code to the binary. `normal.md` and `big.md` have no mermaid blocks: the diagram only costs where there is one.

| test | result |
|---|---|
| warm startup, 5 runs, load average 25 (`bench/startup.sh /tmp/runemd-bench 5 warm`) | normal min 244, p50 266, max 958 (one run off the scale because of the load); big min 238, p50 249, max 266. Threshold 350 on p50: met |
| idle memory, 3 runs (`bench/memory.sh`) | normal 91 MB, big 96–97 MB. Big threshold 100 MB: met, with a 3 MB margin |
| dropped refreshes while scrolling | not repeated: machine under load (see above, the measurement would not be valid) and the test files have no diagrams |
| a flowchart of 141 nodes and 140 edges (1934 bytes) | drawn in 15 ms on the first frame (`broken_and_huge_mermaid_never_panic`) |
| over 400 items (rune-mermaid `MAX_ITEMS`) | the code block stays, 5–20 ms, even with 3000 nodes (52 KB) |
