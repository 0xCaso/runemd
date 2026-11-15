#!/bin/sh
# Recenti su due righe e titolo dell'albero (record docs/decisions/2026-11-15-recenti-due-righe.md):
# i dati del mockup A (due error-context.md nello stesso repo, DESIGN.md in due repo, un nome
# lungo, branch feat/sito-forma-c), headless, tema chiaro e scuro, sidebar a 180 e 240 pt
# (`RUNEMD_SIDEBAR`), puntatore sulla seconda riga di Recenti (hover).
# uso: bench/recenti_due_righe.sh [WORKDIR]  → WORKDIR/recenti-due-righe-<tema>-<larghezza>.png
set -eu
cd "$(dirname "$0")/.."
cargo build --profile perf -q
W=${1:-$(mktemp -d)}; mkdir -p "$W"
W=$(cd "$W" && pwd -P)
R="$W/root"; rm -rf "$R"; mkdir -p "$R"
note() { mkdir -p "$(dirname "$1")"; printf '# %s\n\nNota di prova.\n' "$(basename "$1" .md)" > "$1"; }
repo() { mkdir -p "$R/$1/.git"; printf 'ref: refs/heads/%s\n' "$2" > "$R/$1/.git/HEAD"; }
repo my-portfolio feat/sito-forma-c
repo runemd main
p=$R/my-portfolio; r=$R/runemd
note "$r/CHOSEN.md"; touch -t "$(date -v-2H +%Y%m%d%H%M.%S)" "$r/CHOSEN.md"
note "$r/design/2026-11-14-contatto-mockup.md"; touch -t "$(date -v-61M +%Y%m%d%H%M.%S)" "$r/design/2026-11-14-contatto-mockup.md"
note "$r/DESIGN.md"; touch -t "$(date -v-62M +%Y%m%d%H%M.%S)" "$r/DESIGN.md"
note "$p/DESIGN.md"; touch -t "$(date -v-13M +%Y%m%d%H%M.%S)" "$p/DESIGN.md"
note "$p/test-results/contatto-desktop-2-colonne-chromium/error-context.md"; touch -t "$(date -v-2M +%Y%m%d%H%M.%S)" "$p/test-results/contatto-desktop-2-colonne-chromium/error-context.md"
note "$p/design/x.md"; note "$p/docs/y.md"; note "$p/src/z.md"
touch -t "$(date -v-3H +%Y%m%d%H%M.%S)" "$p/design/x.md" "$p/docs/y.md" "$p/src/z.md"
open="$p/test-results/ingress-nessun-pixel-bianco-chromium/error-context.md"; note "$open"
for theme in dark light; do
  for w in 180 240; do
    RUNEMD_THEME=$theme RUNEMD_SIDEBAR=$w RUNEMD_ROOTS="$R" RUNE_DIRS_ROOT="$W/dirs-$theme-$w" target/perf/runemd "$open" \
      --script tests/scripts/recenti_hover.json --size 1000x680 --scale 2 \
      --at mark:hover --snap "$W/recenti-due-righe-$theme-$w.png" --tree "$W/tree-$theme-$w.json" >/dev/null 2>&1
  done
done
echo "foto: $W/recenti-due-righe-*.png"
