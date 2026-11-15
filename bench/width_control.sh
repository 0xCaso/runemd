#!/bin/sh
# Controllo della larghezza nella barra delle schede (gruppo 1, item 4), headless con
# tests/scripts/width_control.json: un clic sul pulsante ↔ apre il popover, ⌘⌥= due volte porta la
# colonna da 680 a 840 pt. Con RUNEMD_ROOTS le impostazioni restano in memoria.
# uso: bench/width_control.sh [WORKDIR]
#   → WORKDIR/width-control-{dark,light}.png (il pannello col popover aperto). Esce 1 se il popover
#   non c'è o il valore non è 840 pt.
set -eu
cd "$(dirname "$0")/.."
cargo build --profile perf -q
W=${1:-$(mktemp -d)}; mkdir -p "$W"
W=$(cd "$W" && pwd -P)
mkdir -p "$W/root"
fail() { echo "width_control: $*" >&2; exit 1; }
for theme in dark light; do
  cp fixtures/normal.md "$W/doc.md"
  RUNEMD_ROOTS="$W/root" RUNE_DIRS_ROOT="$W/dirs" target/perf/runemd "$W/doc.md" --script tests/scripts/width_control.json \
    --size 1280x860 --scale 2 --theme $theme --fps 30 --at mark:open+0.3 --snap "$W/width-control-$theme.png" \
    --crop main --tree "$W/tree-$theme.json" >/dev/null 2>&1
  python3 -c 'import json,sys; e=json.load(open(sys.argv[1]))["elements"]; sys.exit(not any(x.get("key")=="width-popover" for x in e))' \
    "$W/tree-$theme.json" || fail "$theme: popover chiuso"
  python3 -c 'import json,sys; e=json.load(open(sys.argv[1]))["elements"]; sys.exit(not any(x.get("key")=="width-slider-value" and x.get("text")=="840 pt" for x in e))' \
    "$W/tree-$theme.json" || fail "$theme: valore diverso da 840 pt"
done
echo "foto: $W/width-control-{dark,light}.png"
