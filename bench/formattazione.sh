#!/bin/sh
# Formattazione Markdown (audit docs/audit/2026-11-16-formattazione-markdown.md), headless:
# il file di prova in chiaro e in scuro, ritagliato al documento.
# uso: bench/formattazione.sh DIR  (scrive DIR/formattazione-light.png e -dark.png)
set -eu
cd "$(dirname "$0")/.."
OUT=$1
cargo build --profile perf -q
W=$(mktemp -d); trap 'rm -rf "$W"' EXIT
mkdir -p "$W/root" "$OUT"
cp docs/audit/2026-11-16-formattazione-markdown/prova.md "$W/prova.md"
for t in light dark; do
  RUNEMD_THEME=$t RUNEMD_ROOTS="$W/root" RUNE_DIRS_ROOT="$W/dirs" target/perf/runemd "$W/prova.md" \
    --size 1240x2200 --scale 1 --at 1.0 --snap "$OUT/formattazione-$t.png" --crop main >/dev/null 2>&1
  echo "foto: $OUT/formattazione-$t.png"
done
