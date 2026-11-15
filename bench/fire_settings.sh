#!/bin/sh
# Fire settings (record docs/decisions/2026-11-15-work-order-after-v1.md, group 1), headless
# with tests/scripts/fire_settings.json: Cmd+, opens the panel and scrolls it to the end (Intensity, Height,
# Speed, Colour); a click on a colour, Cmd+, closes; two paragraphs written at the end by a shell
# light the fire (real time). With RUNEMD_ROOTS the settings stay in memory.
# usage: bench/fire_settings.sh [WORKDIR]
#   → WORKDIR/fire-settings-{dark,light}.png (the panel, cropped) and WORKDIR/fire-<colour>.png
#   (the bottom of the document with the fire lit, dark). Exits 1 if a colour is not picked.
set -eu
cd "$(dirname "$0")/.."
cargo build --profile perf -q
W=${1:-$(mktemp -d)}; mkdir -p "$W"
W=$(cd "$W" && pwd -P)
mkdir -p "$W/root"
fail() { echo "fire_settings: $*" >&2; exit 1; }
for theme in dark light; do
  sed "s|COLOR|Fire|" tests/scripts/fire_settings.json > "$W/script.json"
  cp fixtures/normal.md "$W/doc.md"
  RUNEMD_ROOTS="$W/root" RUNE_DIRS_ROOT="$W/dirs" target/perf/runemd "$W/doc.md" --script "$W/script.json" \
    --size 1280x860 --scale 2 --theme $theme --fps 30 --at mark:panel+0.3 --snap "$W/fire-settings-$theme.png" \
    --crop settings --tree "$W/panel-$theme.json" >/dev/null 2>&1
  for t in Intensity Height Speed Color "System accent" "100 %" "110 pt" "1×"; do
    python3 -c 'import json,sys; sys.exit(not any(e.get("text")==sys.argv[2] for e in json.load(open(sys.argv[1]))["elements"]))' \
      "$W/panel-$theme.json" "$t" || fail "$theme: missing \"$t\""
  done
done
for c in Fire:fire Blue:blue Violet:violet Green:green "System accent:accent"; do
  label=${c%%:*}; name=${c##*:}
  sed "s|COLOR|$label|" tests/scripts/fire_settings.json > "$W/script-$name.json"
  cp fixtures/normal.md "$W/$name.md"
  RUNEMD_ROOTS="$W/root" RUNE_DIRS_ROOT="$W/dirs" target/perf/runemd "$W/$name.md" --script "$W/script-$name.json" \
    --realtime --size 1280x860 --scale 2 --theme dark --fps 30 --at mark:lit+0.3 --snap "$W/full-$name.png" --crop main \
    --tree "$W/lit-$name.json" 2>/dev/null |
  while read -r line; do
    case "$line" in "rune-mark picked"*) printf '\nA new paragraph at the end.\n\nAnd one more.\n' >> "$W/$name.md" ;; esac
  done
  python3 -c 'import json,sys; sys.exit(not any(e.get("key")=="fire" for e in json.load(open(sys.argv[1]))["elements"]))' \
    "$W/lit-$name.json" || fail "$label: no fire"
  ffmpeg -loglevel error -y -i "$W/full-$name.png" -vf "crop=iw:480:0:ih-480" "$W/fire-$name.png"
done
echo "photos: $W/fire-settings-{dark,light}.png $W/fire-{fire,blue,violet,green,accent}.png"
