#!/bin/sh
# Markdown formatting (audit docs/audit/2026-11-16-markdown-formatting.md), headless:
# the test file in light and dark, cropped to the document.
# usage: bench/formattazione.sh DIR  (writes DIR/formattazione-light.png and -dark.png)
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
  echo "photos: $OUT/formattazione-$t.png"
done
