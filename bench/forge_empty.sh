#!/bin/sh
# forge-empty (docs/decisions/2026-11-16-wordmark-plate.md), headless and in real time: no file open, a repo
# with a few notes in the watched folders. Calm: the wordmark with frozen fire. Writing: a shell appends to a
# note (not open), the fire in the letters rises and moves and "An agent is writing <file> · Open" shows under
# the buttons. Also "reduce motion" while writing: still. The agent-line shots are the writing ones, cropped.
# usage: bench/forge_empty.sh [WORKDIR]  → WORKDIR/forge-empty-<calm|writing|writing-reduced|agent-line>-<theme>.png
set -eu
cd "$(dirname "$0")/.."
cargo build --profile perf -q
W=${1:-$(mktemp -d)}; mkdir -p "$W"
W=$(cd "$W" && pwd -P)
R="$W/root"; rm -rf "$R"; mkdir -p "$R/runemd/.git" "$R/runemd/docs/decisions"
printf 'ref: refs/heads/main\n' > "$R/runemd/.git/HEAD"
note() { printf '# %s\n\nA short note.\n' "$(basename "$1" .md)" > "$1"; }
note "$R/runemd/DESIGN.md"
note "$R/runemd/README.md"
written="$R/runemd/docs/decisions/2026-11-16-paths-name-and-link.md"
note "$written"
for theme in dark light; do
  for state in calm writing writing-reduced; do
    [ "$state" = calm ] && at=mark:ready+0.1 || at=mark:shot
    reduce=0; [ "$state" = writing-reduced ] && reduce=1
    RUNEMD_THEME=$theme RUNEMD_ROOTS="$R" RUNEMD_REDUCE_MOTION=$reduce RUNE_DIRS_ROOT="$W/dirs-$theme-$state" \
      target/perf/runemd --script tests/scripts/forge_empty.json --realtime --size 1280x760 \
      --scale 2 --fps 30 --at "$at" --snap "$W/forge-empty-$state-$theme.png" 2>/dev/null |
    while read -r line; do
      case "$line" in
        "rune-mark ready"*|"rune-mark more"*) if [ "$state" != calm ]; then printf '\nAnother line from the agent.\n' >> "$written"; fi ;;
      esac
    done
  done
  sips -c 900 1600 --cropOffset 300 900 "$W/forge-empty-writing-$theme.png" --out "$W/forge-empty-agent-line-$theme.png" >/dev/null
done
echo "photos: $W/forge-empty-*.png"
