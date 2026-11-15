#!/bin/sh
# Saved folders instead of the tree (record docs/decisions/2026-11-15-folders-instead-of-tree.md),
# headless: three repos and "Note" saved (`RUNEMD_SAVED`), one tab in runemd and the front one in
# Downloads, not saved: runemd stays expanded, Downloads expands by itself with "Save".
# usage: bench/sidebar_cartelle.sh [WORKDIR]  → WORKDIR/sidebar-cartelle-<theme>.png
set -eu
cd "$(dirname "$0")/.."
cargo build --profile perf -q
W=${1:-$(mktemp -d)}; mkdir -p "$W"
W=$(cd "$W" && pwd -P)
R="$W/root"; rm -rf "$R"; mkdir -p "$R"
repo() { mkdir -p "$R/$1/.git"; printf 'ref: refs/heads/%s\n' "$2" > "$R/$1/.git/HEAD"; }
put() { mkdir -p "$(dirname "$R/$1")"; printf '# %s\n\nTest note.\n' "$(basename "$1" .md)" > "$R/$1"; touch -t "$2" "$R/$1"; }
repo runemd main; repo my-portfolio feat/sito-form; repo notes main
put runemd/docs/decisions/2026-11-15-work-order-after-v1.md 202611151200
put runemd/docs/decisions/2026-11-15-recent-two-lines.md 202611150900
put runemd/DESIGN.md 202611151158
put runemd/progress.md 202611151155
put runemd/design/mockups/README.md 202611140900
put runemd/docs/proof/README.md 202611140800
put runemd/.claude/nascosto.md 202611140700
mkdir -p "$R/runemd/target/empty" "$R/runemd/src"; printf 'fn main() {}\n' > "$R/runemd/src/main.rs"
put my-portfolio/README.md 202611130900
put notes/clienti.md 202611120900
put Note/idee.md 202611110900
put Downloads/lista-spesa.md 202611151201
for theme in dark light; do
  RUNEMD_THEME=$theme RUNEMD_ROOTS="$R" RUNEMD_SAVED="$R/runemd:$R/my-portfolio:$R/notes:$R/Note" \
    RUNE_DIRS_ROOT="$W/dirs-$theme" target/perf/runemd "$R/runemd/docs/decisions/2026-11-15-work-order-after-v1.md" "$R/Downloads/shopping-list.md" \
    --script tests/scripts/sidebar_cartelle.json --size 960x860 --scale 2 \
    --at "mark:ready" --snap "$W/sidebar-cartelle-$theme.png" --tree "$W/tree-$theme.json" >/dev/null 2>&1
done
echo "photos: $W/sidebar-cartelle-*.png"
