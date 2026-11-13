#!/bin/sh
# Ancora di lettura: il primo blocco visibile e il suo offset restano fermi quando il file cambia.
# uso: bench/anchor.sh [WORKDIR]
#
# 1. Verifica: i test headless di `src/ui/doc_view.rs` aprono un documento nella vista dell'app,
#    scorrono a metà e applicano `Document::update` nei tre casi (5 paragrafi in testa, blocco in
#    cima allungato, ultimo blocco allungato), più file vuoto, blocco sparito e 500 KB senza righe
#    vuote; confrontano id e offset del primo blocco visibile (±1 pt).
# 2. App vera, modalità append: `--script` scorre una copia di `fixtures/big.md` a metà; "prima"
#    è il primo blocco visibile a quel punto. Poi, con `--realtime` (frame al passo dell'orologio),
#    una shell appende un paragrafo ogni 200 ms per 5 s mentre l'app guarda il file: "dopo" è il
#    primo blocco visibile alla fine. Esce 1 se chiave o offset (±1 pt) cambiano o se l'app non ha
#    ricaricato.
# 3. Latenza (`MDSPIKE_LATENCY`): per ogni ricarica, ms dall'mtime del file alla ricarica e al
#    primo frame dopo. Headless i frame sono offscreen a 30 fps: non è la latenza di un frame
#    presentato a schermo, che va misurata con l'app in finestra (vedi `latency` sotto).
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
echo "app, prima: $before"

: > "$W/latency.log"
MDSPIKE_LATENCY="$W/latency.log" target/perf/runemd "$W/anchor.md" --script tests/scripts/anchor_append.json \
  --realtime --scale 1 --fps 30 --at mark:after+0.3 --tree "$W/after.json" 2>/dev/null |
while read -r line; do
  case "$line" in
    "rune-mark middle"*)
      # 25 paragrafi, uno ogni 200 ms: 5 s.
      (for i in $(seq 25); do
        printf '\n\nParagrafo aggiunto %s: il testo in lettura resta fermo mentre il file cresce.\n' "$i" >> "$W/anchor.md"
        sleep 0.2
      done) &
      ;;
  esac
done
after=$(first_visible "$W/after.json")
echo "app, dopo 25 append: $after"
echo "ricariche: $(grep -c '^reload' "$W/latency.log" || true)"
echo "headless, scrittura→ricarica ms: $(latency "$W/latency.log" write_to_reload_ms)"
echo "headless, scrittura→frame offscreen ms: $(latency "$W/latency.log" write_to_frame_ms)"
python3 -c '
import sys
(k0, y0), (k1, y1) = (s.split() for s in sys.argv[1:3])
reloads = int(sys.argv[3])
ok = k0 == k1 and abs(float(y0) - float(y1)) <= 1.0 and reloads > 0
print("ancora: " + ("ferma" if ok else "SALTA o nessuna ricarica"))
sys.exit(0 if ok else 1)' "$before" "$after" "$(grep -c '^reload' "$W/latency.log" || true)"
