#!/bin/sh
# Long words (record docs/decisions/2026-11-15-work-order-after-v1.md, group 1), headless:
# fixtures/long-title.md (path-like heading, paragraph, hash without separators, list, cells) in a
# narrow window, dark, --scale 2, cropped to the document.
# usage: bench/long_title.sh OUT.png [BINARY]  (BINARY: another build, for the "before" photo).
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
echo "photos: $OUT"
