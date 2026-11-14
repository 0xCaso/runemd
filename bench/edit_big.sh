#!/bin/sh
# Modifica di big.md nell'app (headless): Cmd+E, scroll a metà del sorgente, un clic per il
# cursore, poi 200 caratteri, uno per frame (20 fps, un tasto ogni 0,05 s). Dal `--trace`
# prende il tempo CPU di ogni frame fra i mark "typing" e "typed" e stampa p50, p95 e max.
# uso: bench/edit_big.sh [WORKDIR] [FILE]   (FILE: fixtures/big.md di default)
# Esce 1 se p95 > 8 ms (la soglia della spec).
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
    sys.exit("edit_big: nessun frame fra i mark typing e typed (il trace è vuoto o lo script è cambiato?)")
ms.sort()
pct = lambda p: ms[round((len(ms) - 1) * p)]
print(f"frame cpu ms: n={len(ms)} p50={pct(0.5):.2f} p95={pct(0.95):.2f} max={pct(1.0):.2f}")
sys.exit(0 if pct(0.95) <= 8.0 else 1)' "$W/trace.jsonl" bench/edit_big.json
# Il testo è entrato nel file in modifica (il campo ha il fuoco).
python3 -c '
import json, sys
t = " ".join(x.get("text") or "" for x in json.load(open(sys.argv[1]))["elements"])
sys.exit(0 if "per misurare il costo di ogni tasto. Una frase" in t else 1)' "$W/tree.json" ||
  { echo "edit_big: il testo scritto non è nel sorgente" >&2; exit 1; }
