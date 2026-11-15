#!/bin/sh
# Percorsi nel testo (record docs/decisions/2026-11-16-percorsi-nome-e-link.md), headless:
# fixtures/percorsi.md letto dal repo (così i percorsi relativi si trovano), chiaro e scuro,
# ritagliato al documento. Da confrontare con il mockup p-mix.
# uso: bench/percorsi.sh DIR  (scrive DIR/percorsi-light.png e -dark.png)
set -eu
cd "$(dirname "$0")/.."
OUT=$1
cargo build --profile perf -q
W=$(mktemp -d); trap 'rm -rf "$W"' EXIT
mkdir -p "$W/root" "$OUT"
for t in light dark; do
  RUNEMD_THEME=$t RUNEMD_ROOTS="$W/root" RUNE_DIRS_ROOT="$W/dirs" target/perf/runemd "$PWD/fixtures/percorsi.md" \
    --size 1240x1300 --scale 1 --at 1.0 --snap "$OUT/percorsi-$t.png" --crop main >/dev/null 2>&1
  echo "foto: $OUT/percorsi-$t.png"
done
