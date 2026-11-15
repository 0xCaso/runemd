#!/bin/sh
# Sidebar and tree with worse but realistic data (task 28, `break-ui`): 28 repos, long,
# short, non-Latin and repeated names, a long repo and branch, nested folders, and 12 files that a
# shell writes together (12 rings in Recent). Headless, real time, no real names: the
# watched folders are only the test one (`RUNEMD_ROOTS`).
# usage: bench/worst_case.sh [WORKDIR] [SIZE]   (SIZE: 1280x800 by default; 1280x600 the short window)
#   saves WORKDIR/worst-dark.png and WORKDIR/worst-light.png (--scale 2) at the "burst" mark,
#   WORKDIR/worst-tree.json for the rects.
set -eu
cd "$(dirname "$0")/.."
cargo build --profile perf -q
W=${1:-$(mktemp -d)}; mkdir -p "$W"
W=$(cd "$W" && pwd -P)
SIZE=${2:-1280x800}
R="$W/root"; rm -rf "$R"; mkdir -p "$R"
note() { mkdir -p "$(dirname "$1")"; printf '# %s\n\nTest note.\n' "$(basename "$1" .md)" > "$1"; }
for i in $(seq -w 1 28); do
  repo="$R/service-$i"
  mkdir -p "$repo/.git"; printf 'ref: refs/heads/main\n' > "$repo/.git/HEAD"
  note "$repo/README.md"
  note "$repo/docs/plan.md"
done
big="$R/northwind-industries-holdings-piattaforma-monorepo"
mkdir -p "$big/.git"
printf 'ref: refs/heads/feature/aleksandra/offline-synchronization-with-conflict-resolution\n' > "$big/.git/HEAD"
note "$big/README.md"
note "$big/a.md"
note "$big/設計メモ.md"
note "$big/Привет-заметки.md"
note "$big/docs/architecture/synchronization/2026-11-14-architecture-of-the-offline-synchronization-system-v2-final.md"
note "$big/docs/architecture/synchronization/decisions-made-with-bartholomew.fitzgerald@northwind-industries-holdings.example.com.md"
note "$big/packages/desktop-client-for-macos-with-offline-support/src/components/sidebar/NOTE.md"
for k in $(seq 1 40); do note "$big/docs/adr/adr-$k-decision-on-a-fairly-long-topic.md"; done
open="$big/docs/architecture/synchronization/2026-11-14-architecture-of-the-offline-synchronization-system-v2-final.md"
# 12 files written together: README and plan of 6 repos.
burst() {
  for i in 01 02 03 04 05 06; do
    for f in README.md docs/plan.md; do printf '\nA new line.\n' >> "$R/service-$i/$f"; done
  done
  printf '\nA new line.\n' >> "$open"
}
for theme in dark light; do
  RUNEMD_THEME=$theme RUNEMD_ROOTS="$R" RUNE_DIRS_ROOT="$W/dirs-$theme" target/perf/runemd "$open" \
    --script tests/scripts/worst_case.json --realtime --size "$SIZE" --scale 2 --fps 30 \
    --at mark:burst+0.5 --snap "$W/worst-$theme.png" --tree "$W/worst-tree.json" 2>/dev/null |
  while read -r line; do
    case "$line" in "rune-mark ready"*) burst ;; esac
  done
done
echo "photos: $W/worst-dark.png $W/worst-light.png"
