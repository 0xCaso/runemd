#!/bin/sh
# Parole lunghe (record docs/decisions/2026-11-15-ordine-lavori-dopo-v1.md, gruppo 1), headless:
# fixtures/long-title.md (titolo-percorso, paragrafo, hash senza separatori, lista, celle) in una
# finestra stretta, scuro, --scale 2, ritagliata al documento.
# uso: bench/long_title.sh OUT.png [BINARIO]  (BINARIO: un altro build, per la foto "prima").
set -eu
cd "$(dirname "$0")/.."
OUT=$1
BIN=${2:-target/perf/runemd}
[ -n "${2:-}" ] || cargo build --profile perf -q
W=$(mktemp -d); trap 'rm -rf "$W"' EXIT
mkdir -p "$W/root"
cp fixtures/long-title.md "$W/long-title.md"
RUNEMD_THEME=dark RUNEMD_ROOTS="$W/root" RUNE_DIRS_ROOT="$W/dirs" "$BIN" "$W/long-title.md" \
  --size 1000x760 --scale 2 --at 1.0 --snap "$OUT" --crop main >/dev/null 2>&1
echo "foto: $OUT"
