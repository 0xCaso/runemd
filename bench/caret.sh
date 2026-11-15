#!/bin/sh
# "Writing" caret (record docs/decisions/2026-11-15-caret-at-the-edit-point.md), headless and in
# real time: a shell changes a word in the middle of the file ("cat" → "dog") and then appends a sentence at
# the end without a newline. Two photos: "middle" (caret after «dog», mid-file) and "append" (caret after
# the last word). No dither line at the bottom.
# usage: bench/caret.sh [WORKDIR]  → WORKDIR/caret-<theme>-<middle|append>.png
set -eu
cd "$(dirname "$0")/.."
cargo build --profile perf -q
W=${1:-$(mktemp -d)}; mkdir -p "$W"
W=$(cd "$W" && pwd -P)
R="$W/root"; mkdir -p "$R"
SRC='# Plan: sidebar with saved folders

An agent is writing this file. The cat reads the first paragraphs, which are complete and stay still while the rest arrives.

## Steps

1. Read the saved folders from `prefs.json`.
2. Keep in the tree only the folders with Markdown below.

The index keeps for each folder how many Markdown files it has below, so the tree updates'
for theme in dark light; do
  for m in middle append; do
    printf '%s\n' "$SRC" > "$R/plan.md"
    RUNEMD_THEME=$theme RUNEMD_ROOTS="$R" RUNE_DIRS_ROOT="$W/dirs-$theme" \
      target/perf/runemd "$R/plan.md" --script tests/scripts/caret.json --realtime --size 1100x620 \
      --scale 2 --fps 30 --at "mark:$m+0.1" --snap "$W/caret-$theme-$m.png" --crop main 2>/dev/null |
    while read -r line; do
      case "$line" in
        "rune-mark ready"*) python3 -I -c 'import sys; p=sys.argv[1]; s=open(p).read(); open(p,"w").write(s.replace("The cat", "The dog"))' "$R/plan.md" ;;
        "rune-mark more"*) printf ' without reading the disk again when' >> "$R/plan.md" ;;
      esac
    done
  done
done
echo "photos: $W/caret-*.png"
