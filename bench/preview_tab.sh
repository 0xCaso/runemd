#!/bin/sh
# Preview tab (record docs/decisions/2026-11-15-work-order-after-v1.md, group 1), headless:
# alfa.md opened from the command line (pinned); single click on beta.md and then on gamma.md in Recent
# (gamma takes the place of beta, name in italics); double click on gamma.md (pins it), click on
# delta.md (new preview next to it).
# usage: bench/preview_tab.sh [WORKDIR]  → WORKDIR/preview-tab-<theme>-<mark>.png; exits 1 if the tabs do not add up.
set -eu
cd "$(dirname "$0")/.."
cargo build --profile perf -q
W=${1:-$(mktemp -d)}; mkdir -p "$W"
W=$(cd "$W" && pwd -P)
R="$W/root"; rm -rf "$R"; mkdir -p "$R"
for n in alfa beta gamma delta; do printf '# %s\n\nTest note.\n' "$n" > "$R/$n.md"; done
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
echo "tabs: alfa.md gamma.md → alfa.md gamma.md delta.md; photos: $W/preview-tab-*.png"
