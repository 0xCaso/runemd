#!/bin/sh
# metal-tab (docs/decisions/2026-11-16-metal-and-rune-effects.md), headless and in real time: a repo
# with a few notes, two tabs open (the front one is the file an agent writes), ⌘P open on "paths".
# Calm: frozen edge on the active tab and the chosen ⌘P row. Writing: a shell appends to the open
# file and the edge moves on its tab, its ⌘P row, its Recent row and its tree row.
# usage: bench/metal_tab.sh [WORKDIR]  → WORKDIR/metal-tab-<calm|writing>-<theme>[-quick].png
set -eu
cd "$(dirname "$0")/.."
cargo build --profile perf -q
W=${1:-$(mktemp -d)}; mkdir -p "$W"
W=$(cd "$W" && pwd -P)
R="$W/root"; rm -rf "$R"; mkdir -p "$R/runemd/.git" "$R/runemd/docs/decisions" "$R/runemd/src"
printf 'ref: refs/heads/main\n' > "$R/runemd/.git/HEAD"
note() { printf '# %s\n\nA short note.\n' "$(basename "$1" .md)" > "$1"; }
note "$R/runemd/DESIGN.md"
note "$R/runemd/README.md"
note "$R/runemd/docs/decisions/2026-11-15-thick-ring.md"
note "$R/runemd/docs/decisions/2026-11-15-paths-in-recent.md"
note "$R/runemd/docs/decisions/2026-11-16-wrap-only-label-blocks.md"
note "$R/runemd/paths-test.md"
open="$R/runemd/docs/decisions/2026-11-16-paths-name-and-link.md"
printf '# RuneMD: paths, name and link\n\nA long path in Recent hides the name. Matteo chose to show the name first.\n\n## What changes\n\nThe file name is what you look for. The folders matter only when two names are the same, so they move to the tooltip and the link target.\n\nClicking the name opens the file in a new tab.\n' > "$open"
for theme in dark light; do
  for state in calm writing; do
    for quick in "" "-quick"; do
      script=tests/scripts/metal_tab.json
      [ -n "$quick" ] && script=tests/scripts/metal_tab_quick.json
      [ "$state" = calm ] && at=mark:ready+0.1 || at=mark:shot
      RUNEMD_THEME=$theme RUNEMD_ROOTS="$R" RUNE_DIRS_ROOT="$W/dirs-$theme-$state$quick" \
        target/perf/runemd "$R/runemd/DESIGN.md" "$open" --script "$script" --realtime --size 1280x760 \
        --scale 2 --fps 30 --at "$at" --snap "$W/metal-tab-$state-$theme$quick.png" 2>/dev/null |
      while read -r line; do
        case "$line" in
          "rune-mark ready"*) if [ "$state" = writing ]; then printf '\nRelative paths resolve from the repo root.\n' >> "$open"; fi ;;
          "rune-mark more"*) if [ "$state" = writing ]; then printf '\nAnother line from the agent.\n' >> "$open"; fi ;;
        esac
      done
    done
  done
done
echo "photos: $W/metal-tab-*.png"
