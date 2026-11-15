#!/bin/sh
# Paths in the text (record docs/decisions/2026-11-16-paths-name-and-link.md), headless:
# fixtures/percorsi.md read from the repo (so relative paths are found), light and dark,
# cropped to the document. To compare with the p-mix mockup.
# usage: bench/percorsi.sh DIR  (writes DIR/percorsi-light.png and -dark.png)
set -eu
cd "$(dirname "$0")/.."
OUT=$1
cargo build --profile perf -q
W=$(mktemp -d); trap 'rm -rf "$W"' EXIT
mkdir -p "$W/root" "$OUT"
for t in light dark; do
  RUNEMD_THEME=$t RUNEMD_ROOTS="$W/root" RUNE_DIRS_ROOT="$W/dirs" target/perf/runemd "$PWD/fixtures/percorsi.md" \
    --size 1240x1300 --scale 1 --at 1.0 --snap "$OUT/percorsi-$t.png" --crop main >/dev/null 2>&1
  echo "photos: $OUT/percorsi-$t.png"
done
