#!/bin/sh
# Sidebar e albero con dati peggiori ma realistici (task 28, `break-ui`): 28 repo, nomi lunghi,
# corti, non latini e doppi, un repo e un branch lunghi, cartelle annidate, e 12 file che una
# shell scrive insieme (12 anelli in Recenti). Headless, tempo vero, nessun nome vero: le
# cartelle osservate sono solo quella di prova (`RUNEMD_ROOTS`).
# uso: bench/worst_case.sh [WORKDIR] [SIZE]   (SIZE: 1280x800 di default; 1280x600 la finestra bassa)
#   salva WORKDIR/worst-dark.png e WORKDIR/worst-light.png (--scale 2) al mark "burst",
#   WORKDIR/worst-tree.json per i rect.
set -eu
cd "$(dirname "$0")/.."
cargo build --profile perf -q
W=${1:-$(mktemp -d)}; mkdir -p "$W"
W=$(cd "$W" && pwd -P)
SIZE=${2:-1280x800}
R="$W/root"; rm -rf "$R"; mkdir -p "$R"
note() { mkdir -p "$(dirname "$1")"; printf '# %s\n\nNota di prova.\n' "$(basename "$1" .md)" > "$1"; }
for i in $(seq -w 1 28); do
  repo="$R/servizio-$i"
  mkdir -p "$repo/.git"; printf 'ref: refs/heads/main\n' > "$repo/.git/HEAD"
  note "$repo/README.md"
  note "$repo/docs/piano.md"
done
big="$R/northwind-industries-holdings-piattaforma-monorepo"
mkdir -p "$big/.git"
printf 'ref: refs/heads/feature/aleksandra/sincronizzazione-offline-con-risoluzione-dei-conflitti\n' > "$big/.git/HEAD"
note "$big/README.md"
note "$big/a.md"
note "$big/設計メモ.md"
note "$big/Привет-заметки.md"
note "$big/docs/architettura/sincronizzazione/2026-11-14-architettura-del-sistema-di-sincronizzazione-offline-v2-definitiva.md"
note "$big/docs/architettura/sincronizzazione/decisioni-prese-con-bartholomew.fitzgerald@northwind-industries-holdings.example.com.md"
note "$big/packages/client-desktop-per-macos-con-supporto-offline/src/componenti/barra-laterale/NOTE.md"
for k in $(seq 1 40); do note "$big/docs/adr/adr-$k-decisione-su-un-argomento-abbastanza-lungo.md"; done
open="$big/docs/architettura/sincronizzazione/2026-11-14-architettura-del-sistema-di-sincronizzazione-offline-v2-definitiva.md"
# 12 file scritti insieme: README e piano di 6 repo.
burst() {
  for i in 01 02 03 04 05 06; do
    for f in README.md docs/piano.md; do printf '\nUna riga nuova.\n' >> "$R/servizio-$i/$f"; done
  done
  printf '\nUna riga nuova.\n' >> "$open"
}
for theme in dark light; do
  RUNEMD_THEME=$theme RUNEMD_ROOTS="$R" RUNE_DIRS_ROOT="$W/dirs-$theme" target/perf/runemd "$open" \
    --script tests/scripts/worst_case.json --realtime --size "$SIZE" --scale 2 --fps 30 \
    --at mark:burst+0.5 --snap "$W/worst-$theme.png" --tree "$W/worst-tree.json" 2>/dev/null |
  while read -r line; do
    case "$line" in "rune-mark ready"*) burst ;; esac
  done
done
echo "foto: $W/worst-dark.png $W/worst-light.png"
