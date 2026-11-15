#!/bin/sh
# Scrolling big.md with the heading outline (task 25), headless: 1280×800 window (the outline
# shows), 16 swipes of 4000 pt every 0.5 s on "doc" at 60 fps. From the `--trace` it takes the CPU time of
# each frame between the "scrolling" and "scrolled" marks and prints p50, p95 and max; from the final `--tree`
# it checks that the outline is there ("In this file").
# usage: bench/outline_scroll.sh [WORKDIR] [BINARY]   (BINARY: target/perf/runemd by default; for
# the "before" pass the binary of another revision, and the outline check is skipped)
# SIZE=900x800 measures the narrow window, where the outline is not shown (check skipped there too).
set -eu
cd "$(dirname "$0")/.."
# Without WORKDIR the temporary folder is deleted on exit.
if [ -n "${1:-}" ]; then
  W=$1; mkdir -p "$W"
else
  W=$(mktemp -d); trap 'rm -rf "$W"' EXIT
fi
B=${2:-target/perf/runemd}
[ "$B" = target/perf/runemd ] && cargo build --profile perf -q
cp fixtures/big.md "$W/big.md"
echo "load average: $(uptime | sed 's/.*load averages*: //')"
"$B" "$W/big.md" --script tests/scripts/outline_scroll.json --size "${SIZE:-1280x800}" --scale 1 --fps 60 \
  --at mark:scrolled+0.2 --tree "$W/tree.json" --trace "$W/trace.jsonl" >/dev/null 2>&1
python3 -c '
import json, sys
script = json.load(open(sys.argv[2]))
mark = {e["name"]: e["t"] * 1000 for e in script if e.get("type") == "mark"}
ms = []
for line in open(sys.argv[1]):
    f = json.loads(line)
    if f.get("type") == "header" or "quality" not in f:
        continue
    if mark["scrolling"] <= f["t"] < mark["scrolled"]:
        ms.append(f["quality"]["cpu_ms"])
if not ms:
    sys.exit("outline_scroll: no frames between the marks")
ms.sort()
pct = lambda p: ms[round((len(ms) - 1) * p)]
print(f"frame cpu ms: n={len(ms)} p50={pct(0.5):.2f} p95={pct(0.95):.2f} max={pct(1.0):.2f}")' "$W/trace.jsonl" tests/scripts/outline_scroll.json
{ [ "$B" != target/perf/runemd ] || [ -n "${SIZE:-}" ]; } && exit 0
python3 -c '
import json, sys
t = {x.get("text") for x in json.load(open(sys.argv[1]))["elements"]}
sys.exit(0 if "In this file" in t else 1)' "$W/tree.json" || { echo "outline_scroll: the outline is missing" >&2; exit 1; }
