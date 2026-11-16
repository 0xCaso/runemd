#!/bin/sh
# Group 3 proofs (docs/decisions/2026-11-16-feedback-group-3.md), headless, dark and light, into OUT
# (default docs/proof/gruppo-3): one or more shots per visible change.
#   forge-calm-{a,b}: the calm wordmark fire at two times (it moves), with the new tagline (items 1, 13)
#   quick-open: ⌘P centred on the main panel (2) · favourites: star off and on (3) · toolbar: gear, ↔,
#   ⌘E source (4) · width: the popover slider between the old steps (5) · tab-{preview,pinned}: double
#   click on the preview tab pins it (6) · recent-collapsed (7) · tree-sticky (8) · settings-{open,
#   middle,end,search}: sections, fonts in their face and search, bottom room (9, 10, 12) ·
#   fire-high: the resting fire at the top Height step (11).
# usage: bench/group3_proof.sh [OUT]
set -eu
cd "$(dirname "$0")/.."
cargo build --profile perf -q
OUT=${1:-docs/proof/gruppo-3}; mkdir -p "$OUT"
W=$(mktemp -d); W=$(cd "$W" && pwd -P)
R="$W/root"; mkdir -p "$R"
note() { mkdir -p "$(dirname "$R/$1")"; printf '# %s\n\nA short note.\n' "$(basename "$1" .md)" > "$R/$1"; touch -t "$2" "$R/$1"; }
for r in runemd usage-island my-portfolio; do mkdir -p "$R/$r/.git"; printf 'ref: refs/heads/main\n' > "$R/$r/.git/HEAD"; done
note runemd/README.md 202611161500
note runemd/DECISIONS.md 202611161450
note usage-island/docs/decisions/2026-11-16-red-account-rule.md 202611161400
note my-portfolio/DECISIONS.md 202611131500
i=10; while [ $i -le 45 ]; do note "runemd/docs/decisions/2026-11-$i-decision.md" 202611101000; i=$((i+1)); done
note runemd/docs/app/overview.md 202611101000
note runemd/design/mockups/notes.md 202611101000
open="$R/runemd/docs/decisions/2026-11-40-decision.md"
cp fixtures/normal.md "$R/runemd/DESIGN.md"
run() { # NAME THEME SCRIPT AT [extra args...] — env passed by the caller
  name=$1 theme=$2 script=$3 at=$4; shift 4
  RUNEMD_ROOTS="$R" RUNE_DIRS_ROOT="$W/dirs-$name-$theme" target/perf/runemd "$@" --script "$script" --size 1280x800 \
    --scale 2 --theme "$theme" --fps 30 --at "$at" --snap "$OUT/$name-$theme.png" --tree "$W/$name-$theme.json" >/dev/null 2>&1
}
crop() { # NAME THEME H W Y X (pixels)
  ffmpeg -loglevel error -y -i "$OUT/$1-$2.png" -vf "crop=$4:$3:$6:$5" "$W/crop.png" && mv "$W/crop.png" "$OUT/$1-$2.png"
}
sed "s|ROOT|$R|" tests/scripts/g3_tab_pin.json > "$W/tab.json"
for t in dark light; do
  if [ -z "${ONLY_SETTINGS:-}" ]; then
  # 1, 13: no file open; real time so the paced wake-ups move the calm fire between a and b.
  for m in a b; do
    RUNEMD_THEME=$t RUNEMD_ROOTS="$R" RUNE_DIRS_ROOT="$W/dirs-forge-$m-$t" target/perf/runemd --script tests/scripts/g3_forge_calm.json \
      --realtime --size 1280x800 --scale 2 --fps 30 --at "mark:$m" --snap "$OUT/forge-calm-$m-$t.png" >/dev/null 2>&1
    crop forge-calm-$m $t 640 1200 440 930
  done
  [ -n "${ONLY_FORGE:-}" ] && continue
  # 2
  run quick-open $t tests/scripts/quick_open_proof.json mark:typed+0.3 "$open"
  # 3: runemd is a favourite (filled star), usage-island is the open file's root (empty star).
  RUNEMD_SAVED="$R/runemd" run favourites $t tests/scripts/g3_sidebar.json mark:open "$R/usage-island/docs/decisions/2026-11-16-red-account-rule.md"
  crop favourites $t 1000 520 260 0
  # 4
  run toolbar $t tests/scripts/g3_sidebar.json mark:open "$open"
  crop toolbar $t 110 520 0 2040
  # 5
  run width $t tests/scripts/g3_width.json mark:open+0.3 "$open"
  crop width $t 330 760 0 1800
  # 6: the preview tab's rect from a first run, then a double click on it.
  sed "s|\"key\": \"TABKEY\"|\"x\": XX, \"y\": YY|" "$W/tab.json" > "$W/tab-$t.json"
  run tab-preview $t "$W/tab.json" mark:preview "$open"
  xy=$(python3 -c 'import json,sys; e=[x for x in json.load(open(sys.argv[1]))["elements"] if x.get("role")=="tab" and x.get("label")=="README.md"][0]["rect"]; print(e[0]+e[2]/2, e[1]+e[3]/2)' "$W/tab-preview-$t.json")
  sed -i '' "s|XX|${xy% *}|; s|YY|${xy#* }|" "$W/tab-$t.json"
  run tab-pinned $t "$W/tab-$t.json" mark:pinned+0.3 "$open"
  crop tab-preview $t 110 1000 0 500; crop tab-pinned $t 110 1000 0 500
  # 7, 8
  run recent-collapsed $t tests/scripts/g3_sidebar.json mark:collapsed "$open"
  run tree-sticky $t tests/scripts/g3_sidebar.json mark:scrolled+0.3 "$open"
  crop recent-collapsed $t 1600 520 0 0; crop tree-sticky $t 1600 520 0 0
  fi
  # 9, 10, 11, 12
  for m in open middle end search; do
    run settings-$m $t tests/scripts/g3_settings.json mark:$m+0.3 "$open"
    crop settings-$m $t 1600 560 0 2000
  done
  run fire-high $t tests/scripts/g3_settings.json mark:high+0.3 "$open"
done
rm -rf "$W"
echo "photos: $OUT"
