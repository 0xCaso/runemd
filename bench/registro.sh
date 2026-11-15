#!/bin/sh
# Logs and plans (record docs/decisions/2026-11-16-tasks-sections-and-columns.md and
# 2026-11-16-wrap-only-label-blocks.md), headless with tests/scripts/registro{,_stretta}.json (scroll down
# up to the Tasks section): .superpowers/sdd/plan/progress.md wide (two columns) and narrow (one column),
# and the Task 12-13 headings of docs/plan.md (number and progress), light and dark, cropped to the document.
# To compare with the mockups t-mix-wide, t-mix-narrow and t-sezioni.
# usage: bench/registro.sh DIR  (writes DIR/registro-{larga,stretta}-<theme>.png and DIR/task-titoli-<theme>.png)
set -eu
cd "$(dirname "$0")/.."
OUT=$1
cargo build --profile perf -q
W=$(mktemp -d); trap 'rm -rf "$W"' EXIT
mkdir -p "$W/root" "$OUT"
sed -n '/^### Task 12:/,/^### Task 14:/p' docs/plan.md | sed '$d' > "$W/plan.md"
shot() { # tema file dimensione foto [script]
  RUNEMD_THEME=$1 RUNEMD_ROOTS="$W/root" RUNE_DIRS_ROOT="$W/dirs" target/perf/runemd "$2" ${5:+--script "$5"} \
    --size "$3" --scale 2 --at "${5:+mark:tasks+}2.5" --snap "$OUT/$4-$1.png" --crop main >/dev/null 2>&1
  echo "photos: $OUT/$4-$1.png"
}
for t in light dark; do
  shot $t "$PWD/.superpowers/sdd/plan/progress.md" 1240x1100 registro-larga tests/scripts/registro.json
  shot $t "$PWD/.superpowers/sdd/plan/progress.md" 640x1100 registro-stretta tests/scripts/registro_stretta.json
  shot $t "$W/plan.md" 1240x1100 task-titoli
done
