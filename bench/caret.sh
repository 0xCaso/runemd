#!/bin/sh
# Caret "sta scrivendo" (record docs/decisions/2026-11-15-caret-al-punto-modificato.md), headless e a
# tempo vero: una shell cambia una parola a metà file ("gatto" → "cane") e poi aggiunge una frase in
# fondo senza a capo. Due foto: "middle" (caret dopo «cane», a metà file) e "append" (caret dopo
# l'ultima parola). Nessuna riga a dither in fondo.
# uso: bench/caret.sh [WORKDIR]  → WORKDIR/caret-<tema>-<middle|append>.png
set -eu
cd "$(dirname "$0")/.."
cargo build --profile perf -q
W=${1:-$(mktemp -d)}; mkdir -p "$W"
W=$(cd "$W" && pwd -P)
R="$W/root"; mkdir -p "$R"
SRC='# Piano: sidebar con cartelle salvate

Un agente sta scrivendo questo file. Il gatto legge i primi paragrafi, che sono completi e restano fermi mentre arriva il resto.

## Passi

1. Leggere le cartelle salvate da `prefs.json`.
2. Tenere nell'"'"'albero solo le cartelle con Markdown sotto.

L'"'"'indice tiene per ogni cartella quanti file Markdown ha sotto, così l'"'"'albero si aggiorna'
for theme in dark light; do
  for m in middle append; do
    printf '%s\n' "$SRC" > "$R/piano.md"
    RUNEMD_THEME=$theme RUNEMD_ROOTS="$R" RUNE_DIRS_ROOT="$W/dirs-$theme" \
      target/perf/runemd "$R/piano.md" --script tests/scripts/caret.json --realtime --size 1100x620 \
      --scale 2 --fps 30 --at "mark:$m+0.1" --snap "$W/caret-$theme-$m.png" --crop main 2>/dev/null |
    while read -r line; do
      case "$line" in
        "rune-mark ready"*) python3 -I -c 'import sys; p=sys.argv[1]; s=open(p).read(); open(p,"w").write(s.replace("Il gatto", "Il cane"))' "$R/piano.md" ;;
        "rune-mark more"*) printf ' senza rileggere il disco quando' >> "$R/piano.md" ;;
      esac
    done
  done
done
echo "foto: $W/caret-*.png"
