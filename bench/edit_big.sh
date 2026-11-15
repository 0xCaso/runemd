#!/bin/sh
# Editing big.md in the app (headless): Cmd+E, scroll to the middle of the source, a click for the
# cursor, then 200 characters, one per frame (20 fps, one key every 0.05 s). From the `--trace`
# it takes the CPU time of each frame between the "typing" and "typed" marks and prints p50, p95 and max.
# usage: bench/edit_big.sh [WORKDIR] [FILE]   (FILE: fixtures/big.md by default)
# Exits 1 if p95 > 8 ms (the spec threshold).
set -eu
cd "$(dirname "$0")/.."
cargo build --profile perf -q
W=${1:-$(mktemp -d)}; mkdir -p "$W"
F=${2:-fixtures/big.md}
cp "$F" "$W/edit.md"
echo "load average: $(uptime | sed 's/.*load averages*: //')"
target/perf/runemd "$W/edit.md" --script bench/edit_big.json --scale 1 --fps 20 \
  --at mark:typed+0.2 --tree "$W/tree.json" --trace "$W/trace.jsonl" >/dev/null 2>&1
python3 -c '
import json, sys
script = json.load(open(sys.argv[2]))
mark = {e["name"]: e["t"] * 1000 for e in script if e.get("type") == "mark"}
ms = []
for line in open(sys.argv[1]):
    f = json.loads(line)
    if f.get("type") == "header" or "quality" not in f:
        continue
    if mark["typing"] <= f["t"] < mark["typed"]:
        ms.append(f["quality"]["cpu_ms"])
if not ms:
    sys.exit("edit_big: no frames between the typing and typed marks (is the trace empty or did the script change?)")
ms.sort()
pct = lambda p: ms[round((len(ms) - 1) * p)]
print(f"frame cpu ms: n={len(ms)} p50={pct(0.5):.2f} p95={pct(0.95):.2f} max={pct(1.0):.2f}")
sys.exit(0 if pct(0.95) <= 8.0 else 1)' "$W/trace.jsonl" bench/edit_big.json
# The text went into the file being edited (the field has focus).
python3 -c '
import json, sys
t = " ".join(x.get("text") or "" for x in json.load(open(sys.argv[1]))["elements"])
sys.exit(0 if "to measure the cost of each key. A sentence" in t else 1)' "$W/tree.json" ||
  { echo "edit_big: the typed text is not in the source" >&2; exit 1; }
