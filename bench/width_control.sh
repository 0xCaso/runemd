#!/bin/sh
# Width control in the tab bar (group 1, item 4), headless with
# tests/scripts/width_control.json: a click on the ↔ button opens the popover, ⌘⌥= twice brings the
# column from 680 to 840 pt. With RUNEMD_ROOTS the settings stay in memory.
# uso: bench/width_control.sh [WORKDIR]
#   → WORKDIR/width-control-{dark,light}.png (the panel with the popover open). Exits 1 if the popover
#   is missing or the value is not 840 pt.
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
    "$W/tree-$theme.json" || fail "$theme: value other than 840 pt"
done
echo "photos: $W/width-control-{dark,light}.png"
