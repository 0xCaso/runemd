#!/bin/sh
# Reading anchor: the first visible block and its offset stay still when the file changes.
# usage: bench/anchor.sh [WORKDIR]
#
# 1. Check: the headless tests in `src/ui/doc_view.rs` open a document in the app view,
#    scroll halfway and apply `Document::update` in the three cases (5 paragraphs at the top, top
#    block grown, last block grown), plus empty file, block gone and 500 KB with no empty
#    lines; they compare id and offset of the first visible block (±1 pt).
# 2. Real app, append mode: `--script` scrolls a copy of `fixtures/big.md` halfway; "before"
#    is the first visible block at that point. Then, with `--realtime` (frames at clock pace),
#    a shell appends a paragraph every 200 ms for 5 s while the app watches the file: "after" is the
#    first visible block at the end. Exits 1 if key or offset (±1 pt) change or if the app has not
#    reloaded.
# 3. Latency (`MDSPIKE_LATENCY`): for each reload, ms from the file mtime to the reload and to the
#    first frame after. Headless, frames are offscreen at 30 fps: this is not the latency of a frame
#    presented on screen, which must be measured with the app in a window (see `latency` below).
set -eu
cd "$(dirname "$0")/.."
W=${1:-$(mktemp -d)}
cargo test --profile perf ui::doc_view -- --nocapture 2>&1 | grep -E '^(inserisci|allunga|500 KB|blocco sparito)|^test |test result'
cargo build --profile perf -q
first_visible() { # tree.json → chiave e offset del primo blocco visibile
  python3 -c '
import json, sys
e = json.load(open(sys.argv[1]))["elements"]
top = next(x for x in e if x.get("key") == "doc")["rect"][1]
rows = [x for x in e if x.get("key") and x["path"].endswith("rows/" + x["key"])]
y, k = min((r["rect"][1], r["key"]) for r in rows if r["rect"][1] + r["rect"][3] > top)
print(f"{k} {y - top:.2f}")' "$1"
}
latency() { # lat.log → n, p50, p95 di una colonna
  python3 -c '
import re, sys
v = sorted(float(m) for m in re.findall(sys.argv[2] + r"=([0-9.]+)", open(sys.argv[1]).read()))
pct = lambda p: v[round((len(v) - 1) * p)] if v else float("nan")
print(f"n={len(v)} p50={pct(0.5):.1f} p95={pct(0.95):.1f} max={pct(1.0):.1f}")' "$1" "$2"
}

cp fixtures/big.md "$W/anchor.md"
target/perf/runemd "$W/anchor.md" --script tests/scripts/anchor_append.json --scale 1 --fps 30 \
  --at mark:middle+0.5 --tree "$W/before.json" >/dev/null 2>&1
before=$(first_visible "$W/before.json")
echo "app, before: $before"

: > "$W/latency.log"
MDSPIKE_LATENCY="$W/latency.log" target/perf/runemd "$W/anchor.md" --script tests/scripts/anchor_append.json \
  --realtime --scale 1 --fps 30 --at mark:after+0.3 --tree "$W/after.json" 2>/dev/null |
while read -r line; do
  case "$line" in
    "rune-mark middle"*)
      # 25 paragraphs, one every 200 ms: 5 s.
      (for i in $(seq 25); do
        printf '\n\nAdded paragraph %s: the text being read stays still while the file grows.\n' "$i" >> "$W/anchor.md"
        sleep 0.2
      done) &
      ;;
  esac
done
after=$(first_visible "$W/after.json")
echo "app, after 25 appends: $after"
echo "ricariche: $(grep -c '^reload' "$W/latency.log" || true)"
echo "headless, scrittura→ricarica ms: $(latency "$W/latency.log" write_to_reload_ms)"
echo "headless, scrittura→frame offscreen ms: $(latency "$W/latency.log" write_to_frame_ms)"
python3 -c '
import sys
(k0, y0), (k1, y1) = (s.split() for s in sys.argv[1:3])
reloads = int(sys.argv[3])
ok = k0 == k1 and abs(float(y0) - float(y1)) <= 1.0 and reloads > 0
print("anchor: " + ("still" if ok else "JUMPS or no reload"))
sys.exit(0 if ok else 1)' "$before" "$after" "$(grep -c '^reload' "$W/latency.log" || true)"
