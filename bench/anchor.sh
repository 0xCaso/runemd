#!/bin/sh
# Ancora di lettura: il primo blocco visibile e il suo offset restano fermi quando il file cambia.
# uso: bench/anchor.sh [WORKDIR]
#
# 1. Verifica: i test headless di `src/ui/doc_view.rs` aprono un documento nella vista dell'app,
#    scorrono a metà e applicano `Document::update` nei tre casi (5 paragrafi in testa, blocco in
#    cima allungato, ultimo blocco allungato), più file vuoto, blocco sparito e 500 KB senza righe
#    vuote; confrontano id e offset del primo blocco visibile (±1 pt).
# 2. App vera: `--script` scorre `fixtures/big.md` a metà e `--tree` registra il primo blocco
#    visibile. Il "dopo" (modifica da shell, rilettura) aspetta il watcher del task 10: finché
#    l'app non ricarica il file, questo passo stampa solo il "prima".
set -eu
cd "$(dirname "$0")/.."
W=${1:-$(mktemp -d)}
cargo test --profile perf ui::doc_view -- --nocapture 2>&1 | grep -E '^(inserisci|allunga|500 KB|blocco sparito)|^test |test result'
cargo build --profile perf -q
cp fixtures/big.md "$W/anchor.md"
first_visible() { # tree.json → chiave e offset del primo blocco visibile
  python3 -c '
import json, sys
e = json.load(open(sys.argv[1]))["elements"]
top = next(x for x in e if x.get("key") == "doc")["rect"][1]
rows = [x for x in e if x.get("key") and x["path"].endswith("rows/" + x["key"])]
y, k = min((r["rect"][1], r["key"]) for r in rows if r["rect"][1] + r["rect"][3] > top)
print(f"{k} {y - top:.2f}")' "$1"
}
target/perf/runemd "$W/anchor.md" --script tests/scripts/anchor_middle.json --scale 1 --fps 30 \
  --at mark:middle+0.5 --tree "$W/before.json" >/dev/null 2>&1
echo "app, prima: $(first_visible "$W/before.json")"
echo "app, dopo: in attesa del watcher (task 10)"
