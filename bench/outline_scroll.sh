#!/bin/sh
# Scroll di big.md con l'indice dei titoli (task 25), headless: finestra 1280×800 (l'indice si
# vede), 16 swipe da 4000 pt ogni 0,5 s su "doc" a 60 fps. Dal `--trace` prende il tempo CPU di
# ogni frame fra i mark "scrolling" e "scrolled" e stampa p50, p95 e max; dal `--tree` finale
# controlla che l'indice ci sia ("In questo file").
# uso: bench/outline_scroll.sh [WORKDIR] [BINARIO]   (BINARIO: target/perf/runemd di default; per
# il "prima" si passa il binario di un'altra revisione, e il controllo dell'indice si salta)
# SIZE=900x800 misura la finestra stretta, dove l'indice non c'è (controllo saltato anche lì).
set -eu
cd "$(dirname "$0")/.."
# Senza WORKDIR la cartella temporanea si cancella all'uscita.
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
    sys.exit("outline_scroll: nessun frame fra i mark")
ms.sort()
pct = lambda p: ms[round((len(ms) - 1) * p)]
print(f"frame cpu ms: n={len(ms)} p50={pct(0.5):.2f} p95={pct(0.95):.2f} max={pct(1.0):.2f}")' "$W/trace.jsonl" tests/scripts/outline_scroll.json
{ [ "$B" != target/perf/runemd ] || [ -n "${SIZE:-}" ]; } && exit 0
python3 -c '
import json, sys
t = {x.get("text") for x in json.load(open(sys.argv[1]))["elements"]}
sys.exit(0 if "In questo file" in t else 1)' "$W/tree.json" || { echo "outline_scroll: l'\''indice non c'\''è" >&2; exit 1; }
