#!/bin/sh
# forge-empty frame cost (constraint 6 of docs/decisions/2026-11-16-metal-and-rune-effects.md), in a real window
# (the screen must be unlocked: a locked screen skips every frame), no file open. 3 s calm, then a shell appends
# to a watched note every 0.4 s for 6 s (writing), then 12 s more (the ring goes out after 10 s: the fall).
# `MDSPIKE_FRAMES` logs each frame; per phase it prints the drawn frames per second, the skipped ones, and the CPU
# update + encode time per drawn frame (p50, p95, max). GPU time is not exposed by Rune: a GPU over budget shows
# as skipped frames and a lower drawn rate while the display link runs.
# usage: bench/forge_frame.sh [WORKDIR] [BINARY]   (BINARY: target/perf/runemd by default; a binary of another
# revision gives the "before" numbers if it has the same probe)
set -eu
cd "$(dirname "$0")/.."
W=${1:-$(mktemp -d)}; mkdir -p "$W"; W=$(cd "$W" && pwd -P)
B=${2:-target/perf/runemd}
[ "$B" = target/perf/runemd ] && cargo build --profile perf -q
R="$W/root"; rm -rf "$R"; mkdir -p "$R/runemd/.git" "$R/runemd/docs"
printf 'ref: refs/heads/main\n' > "$R/runemd/.git/HEAD"
note="$R/runemd/docs/notes.md"; printf '# Notes\n\nA short note.\n' > "$note"
log="$W/frames.log"; rm -f "$log"
now() { python3 -c 'import time; print(f"{time.time()*1000:.1f}")'; }
echo "load average: $(uptime | sed 's/.*load averages*: //')"
RUNEMD_ROOTS="$R" RUNE_DIRS_ROOT="$W/dirs" MDSPIKE_FRAMES="$log" "$B" >/dev/null 2>&1 &
pid=$!
sleep 3; echo "phase calm $(now)" >> "$log"
sleep 3; echo "phase writing $(now)" >> "$log"
i=0; while [ $i -lt 15 ]; do printf '\nAnother line from the agent.\n' >> "$note"; sleep 0.4; i=$((i + 1)); done
echo "phase stopped $(now)" >> "$log"
sleep 12; echo "phase end $(now)" >> "$log"
kill "$pid"; wait "$pid" 2>/dev/null || true
python3 - "$log" <<'PY'
import sys
phases, frames = [], []
for line in open(sys.argv[1]):
    w = line.split()
    if w[0] == "phase":
        phases.append((w[1], float(w[2])))
    elif w[0] == "frame":
        frames.append({k: float(v) for k, v in (x.split("=") for x in w[1:])})
for (name, a), (_, b) in zip(phases, phases[1:]):
    fs = [f for f in frames if a <= f["epoch_ms"] < b]
    drawn = sorted(f["update_ms"] + f["encode_ms"] for f in fs if f["drawn"])
    skipped = sum(1 for f in fs if not f["drawn"])
    secs = (b - a) / 1000
    pct = lambda p: drawn[round((len(drawn) - 1) * p)] if drawn else 0.0
    hz = max((f["hz"] for f in fs), default=0)
    print(f"{name:8} {secs:4.1f} s: drawn {len(drawn) / secs:6.1f}/s (display {hz:.0f} Hz), skipped {skipped}, "
          f"cpu ms/frame p50 {pct(.5):.2f} p95 {pct(.95):.2f} max {pct(1):.2f}")
PY
