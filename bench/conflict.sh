#!/bin/sh
# Conflitto con il disco nell'app vera: Cmd+E, si scrive, una shell riscrive il file, Cmd+S →
# compare la riga "Il file è cambiato su disco" e il disco resta quello della shell; "Tieni il
# mio" → il disco ha il testo locale.
# uso: bench/conflict.sh [WORKDIR]   (con SNAP=docs/proof/conflict.png salva la riga)
# Esce 1 se il salvataggio scrive sopra la shell o se "Tieni il mio" non scrive.
set -eu
cd "$(dirname "$0")/.."
cargo build --profile perf -q
W=${1:-$(mktemp -d)}
start='# Conflitto\n\nPrimo paragrafo.\n\nSecondo paragrafo.\n'
agent='# Conflitto\n\nPrimo paragrafo.\n\nScritto da un agente.\n'
printf "$start" > "$W/c.md"
# Prima parte: fino alla riga, con la foto se chiesta.
if [ -n "${SNAP:-}" ]; then
  snap="--snap $SNAP"
else
  snap=""
fi
target/perf/runemd "$W/c.md" --script tests/scripts/conflict.json --realtime --scale 2 --fps 30 \
  --at mark:row+0.3 $snap --tree "$W/row.json" 2>/dev/null |
while read -r line; do
  case "$line" in "rune-mark typed"*) printf "$agent" > "$W/c.md" ;; esac
done
printf "$agent" | cmp -s - "$W/c.md" || { echo "conflitto: il salvataggio ha scritto sopra la shell" >&2; exit 1; }
python3 -c '
import json, sys
t = {x.get("text") for x in json.load(open(sys.argv[1]))["elements"]}
want = {"Il file è cambiato su disco", "Ricarica", "Tieni il mio"}
sys.exit(0 if want <= t else 1)' "$W/row.json" || { echo "conflitto: la riga non c'è" >&2; exit 1; }
echo "riga: ok, disco: della shell"
# Seconda parte: fino a "Tieni il mio".
printf "$start" > "$W/c.md"
target/perf/runemd "$W/c.md" --script tests/scripts/conflict.json --realtime --scale 1 --fps 30 \
  --at mark:kept+0.3 --tree "$W/kept.json" 2>/dev/null |
while read -r line; do
  case "$line" in "rune-mark typed"*) printf "$agent" > "$W/c.md" ;; esac
done
printf "Nota mia. $start" | cmp -s - "$W/c.md" || { echo "Tieni il mio: il disco non ha il testo locale" >&2; cat "$W/c.md" >&2; exit 1; }
echo "Tieni il mio: il disco ha il testo locale"
