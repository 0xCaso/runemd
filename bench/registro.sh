#!/bin/sh
# Registri e piani (record docs/decisions/2026-11-16-tasks-sezioni-e-colonne.md e
# 2026-11-16-a-capo-solo-blocchi-etichetta.md), headless con tests/scripts/registro{,_stretta}.json (scroll fino
# alla sezione Tasks): .superpowers/sdd/plan/progress.md larga (due colonne) e stretta (una colonna),
# e i titoli Task 12-13 di docs/plan.md (numero e avanzamento), chiaro e scuro, ritagliati al documento.
# Da confrontare con i mockup t-mix-wide, t-mix-narrow e t-sezioni.
# uso: bench/registro.sh DIR  (scrive DIR/registro-{larga,stretta}-<tema>.png e DIR/task-titoli-<tema>.png)
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
  echo "foto: $OUT/$4-$1.png"
}
for t in light dark; do
  shot $t "$PWD/.superpowers/sdd/plan/progress.md" 1240x1100 registro-larga tests/scripts/registro.json
  shot $t "$PWD/.superpowers/sdd/plan/progress.md" 640x1100 registro-stretta tests/scripts/registro_stretta.json
  shot $t "$W/plan.md" 1240x1100 task-titoli
done
