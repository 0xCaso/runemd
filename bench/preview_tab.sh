#!/bin/sh
# Scheda di anteprima (record docs/decisions/2026-11-15-ordine-lavori-dopo-v1.md, gruppo 1), headless:
# alfa.md aperto dalla riga di comando (fisso); clic singolo su beta.md e poi su gamma.md in Recenti
# (gamma prende il posto di beta, nome in corsivo); doppio clic su gamma.md (fissa), clic su
# delta.md (nuova anteprima accanto).
# uso: bench/preview_tab.sh [WORKDIR]  → WORKDIR/preview-tab-<tema>-<mark>.png; esce 1 se le schede non tornano.
set -eu
cd "$(dirname "$0")/.."
cargo build --profile perf -q
W=${1:-$(mktemp -d)}; mkdir -p "$W"
W=$(cd "$W" && pwd -P)
R="$W/root"; rm -rf "$R"; mkdir -p "$R"
for n in alfa beta gamma delta; do printf '# %s\n\nNota di prova.\n' "$n" > "$R/$n.md"; done
sed "s|ROOT|$R|g" tests/scripts/preview_tab.json > "$W/script.json"
fail() { echo "preview_tab: $*" >&2; exit 1; }
tabs() {
  python3 -c '
import json, sys
els = json.load(open(sys.argv[1]))["elements"]
print(" ".join(e.get("text") or "" for e in els if (e.get("key") or "").endswith("-label") and (e.get("key") or "").startswith("t-")))' "$1"
}
for theme in dark light; do
  for m in preview pinned; do
    RUNEMD_THEME=$theme RUNEMD_ROOTS="$R" RUNE_DIRS_ROOT="$W/dirs-$theme" target/perf/runemd "$R/alfa.md" \
      --script "$W/script.json" --size 1000x420 --scale 2 \
      --at "mark:$m" --snap "$W/preview-tab-$theme-$m.png" --tree "$W/tree-$theme-$m.json" >/dev/null 2>&1
  done
  got=$(tabs "$W/tree-$theme-preview.json"); [ "$got" = "alfa.md gamma.md" ] || fail "$theme anteprima: $got"
  got=$(tabs "$W/tree-$theme-pinned.json"); [ "$got" = "alfa.md gamma.md delta.md" ] || fail "$theme fissata: $got"
done
echo "schede: alfa.md gamma.md → alfa.md gamma.md delta.md; foto: $W/preview-tab-*.png"
