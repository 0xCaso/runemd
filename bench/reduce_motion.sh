#!/bin/sh
# "Riduci movimento" (task 28), headless e a tempo vero: una shell scrive in fondo al file una
# frase senza a capo, così si accendono l'anello (scheda e Recenti) e il dither (ultimo blocco a
# metà), e su un file lungo anche il fuoco (testo nuovo più in basso). Due foto a 0,8 s di
# distanza ("lit" e "later"), su `fixtures/normal.md` (anello e fuoco) e su un file corto (anello
# e dither, che sul file lungo è fuori vista): con RUNEMD_REDUCE_MOTION=1 sono uguali pixel per
# pixel (anello e dither fermi, fuoco a piena altezza da subito); senza, cambiano.
# uso: bench/reduce_motion.sh [WORKDIR]
#   con SNAP=docs/proof/reduce-motion salva SNAP.png (scuro, ridotto, --scale 2, ritagliata al
#   pannello: la sidebar mostra nomi di file), SNAP-moving.png (lo stesso istante senza riduzione)
#   e SNAP-dither.png (file corto, ridotto).
# Esce 1 se le foto ridotte cambiano o se quelle normali non cambiano.
set -eu
cd "$(dirname "$0")/.."
cargo build --profile perf -q
if [ -n "${1:-}" ]; then
  W=$1; mkdir -p "$W"
else
  W=$(mktemp -d); trap 'rm -rf "$W"' EXIT
fi
W=$(cd "$W" && pwd -P)
mkdir -p "$W/root"
printf '# Nota corta\n\nUn file corto: il dither sta sotto questo paragrafo.\n' > "$W/short.md"

# shot SRC REDUCE MARK OUT: una foto del pannello a MARK+0,3 con RUNEMD_REDUCE_MOTION=REDUCE.
shot() {
  src=$1
  shift
  cp "$src" "$W/root/doc.md"
  RUNEMD_REDUCE_MOTION=$1 RUNEMD_THEME=dark RUNEMD_ROOTS="$W/root" RUNE_DIRS_ROOT="$W/dirs" \
    target/perf/runemd "$W/root/doc.md" --script tests/scripts/reduce_motion.json --realtime --size 1100x900 \
    --scale 2 --fps 30 --at "mark:$2+0.3" --snap "$3" --crop main 2>/dev/null |
  while read -r line; do
    case "$line" in "rune-mark ready"*) printf '\nUn agente sta scrivendo questa frase e non l' >> "$W/root/doc.md" ;; esac
  done
}
same() { cmp -s "$1" "$2"; }

for f in normal short; do
  case $f in normal) src=fixtures/normal.md ;; short) src=$W/short.md ;; esac
  shot "$src" 1 lit "$W/$f-r1.png"; shot "$src" 1 later "$W/$f-r2.png"
  same "$W/$f-r1.png" "$W/$f-r2.png" || { echo "reduce_motion: $f, con la riduzione le foto cambiano" >&2; exit 1; }
  shot "$src" 0 lit "$W/$f-m1.png"; shot "$src" 0 later "$W/$f-m2.png"
  ! same "$W/$f-m1.png" "$W/$f-m2.png" || { echo "reduce_motion: $f, senza riduzione le foto non cambiano" >&2; exit 1; }
  echo "$f: ridotto, lit e later uguali pixel per pixel; normale, diverse"
done
if [ -n "${SNAP:-}" ]; then
  cp "$W/normal-r1.png" "$SNAP.png"
  cp "$W/normal-m1.png" "$SNAP-moving.png"
  cp "$W/short-r1.png" "$SNAP-dither.png"
  echo "foto: $SNAP.png $SNAP-moving.png $SNAP-dither.png"
fi
