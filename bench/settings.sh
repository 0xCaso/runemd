#!/bin/sh
# Settings (task 27), headless with tests/scripts/settings.json: Cmd+, opens the panel; American Typewriter
# and "+" change the document; "Light" changes the theme; "Add folder…" (fake dialog) and
# "Remove" change the watched folders and Recent; Esc closes. All live, without a restart.
# With RUNEMD_ROOTS the settings stay in memory: the real ones are not touched.
# usage: bench/settings.sh [WORKDIR]
#   with SNAP=docs/proof/settings saves SNAP.png (dark, just opened) and SNAP-light.png (after font,
#   size and theme), --scale 2, cropped to the panel (the sidebar shows file names), and
#   SNAP-fire-off.png: fire turned off from the panel, two paragraphs written at the end by a shell, the
#   bottom of the panel with the label and no flames (real time, cropped with ffmpeg).
# Exits 1 at the first check that fails.
set -eu
cd "$(dirname "$0")/.."
cargo build --profile perf -q
if [ -n "${1:-}" ]; then
  W=$1; mkdir -p "$W"
else
  W=$(mktemp -d); trap 'rm -rf "$W"' EXIT
fi
W=$(cd "$W" && pwd -P)
fail() { echo "settings: $*" >&2; exit 1; }
mkdir -p "$W/one" "$W/two"
printf '# One\n\nIn the first folder.\n' > "$W/one/one.md"
printf '# Two\n\nIn the second folder.\n' > "$W/two/two.md"
cp fixtures/normal.md "$W/doc.md"
sed "s|ADDROOT|$W/two|" tests/scripts/settings.json > "$W/script.json"

# run MARK [more flags]
run() {
  mark=$1
  shift
  RUNEMD_ROOTS="$W/one" RUNE_DIRS_ROOT="$W/dirs" target/perf/runemd "$W/doc.md" --script "$W/script.json" \
    --size 1280x860 --fps 30 --at "mark:$mark+0.3" "$@" 2>/dev/null >/dev/null
}
texts() {
  python3 -c '
import json, sys
print("\n".join(x.get("text") or "" for x in json.load(open(sys.argv[1]))["elements"]))' "$W/$1.json"
}
has() { texts "$1" | grep -qxF -- "$2"; }
# How the panel writes a folder: the home becomes "~".
lbl() { case "$1" in "$HOME"/*) echo "~${1#"$HOME"}" ;; *) echo "$1" ;; esac; }

run open --scale 1 --tree "$W/open.json"
for t in "Watched folders" Font Size Theme System Light Dark "Fire at the bottom of the panel" \
  "Add folder…" Inter "SF Pro" "American Typewriter" "15.5 pt" "$(lbl "$W/one")"; do
  has open "$t" || fail "missing \"$t\" with the panel open"
done
has open "one.md" || fail "Recent without one.md"
echo "Cmd+,: panel open, Recent with one.md"
run font --scale 1 --tree "$W/font.json"
has font "17 pt" || fail "the size did not go to 17"
echo "American Typewriter and +: 17 pt"
run added --scale 1 --tree "$W/added.json"
has added "$(lbl "$W/two")" || fail "the added folder is missing"
has added "two.md" || fail "Recent without two.md after adding"
echo "Add folder: two.md in Recent"
run removed --scale 1 --tree "$W/removed.json"
! has removed "one.md" || fail "one.md stays in Recent after Remove"
! has removed "$(lbl "$W/one")" || fail "the removed folder stays in the panel"
echo "Remove: one.md leaves Recent"
run closed --scale 1 --tree "$W/closed.json"
! has closed "Watched folders" || fail "Esc does not close"
echo "Esc: closed"

if [ -n "${SNAP:-}" ]; then
  run open --scale 2 --theme dark --snap "$W/open.png" --crop main
  run light --scale 2 --theme dark --snap "$W/light.png" --crop main
  cp "$W/open.png" "$SNAP.png"
  cp "$W/light.png" "$SNAP-light.png"
  cp fixtures/normal.md "$W/fire.md"
  RUNEMD_ROOTS="$W/one" RUNE_DIRS_ROOT="$W/dirs" target/perf/runemd "$W/fire.md" --script tests/scripts/settings_fire_off.json \
    --realtime --size 1280x860 --scale 2 --theme dark --fps 30 --at mark:lit+0.3 --snap "$W/fire.png" --crop main \
    --tree "$W/fire.json" 2>/dev/null |
  while read -r line; do
    case "$line" in "rune-mark off"*) printf '\nA new paragraph at the end.\n\nAnd one more.\n' >> "$W/fire.md" ;; esac
  done
  has fire "2 new paragraphs below" || fail "fire off: the label is missing"
  python3 -c 'import json,sys; sys.exit(any(x.get("key") == "fire" for x in json.load(open(sys.argv[1]))["elements"]))' "$W/fire.json" ||
    fail "fire off: the flame slot is there"
  ffmpeg -loglevel error -y -i "$W/fire.png" -vf "crop=iw:480:0:ih-480" "$SNAP-fire-off.png"
  echo "photos: $SNAP.png $SNAP-light.png $SNAP-fire-off.png"
fi
