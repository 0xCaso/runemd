#!/bin/sh
# Impostazioni del fuoco (record docs/decisions/2026-11-15-ordine-lavori-dopo-v1.md, gruppo 1), headless
# con tests/scripts/fire_settings.json: Cmd+, apre il pannello e lo scorre in fondo (Intensità, Altezza,
# Velocità, Colore); un clic su un colore, Cmd+, chiude; due paragrafi scritti in fondo da una shell
# accendono il fuoco (tempo vero). Con RUNEMD_ROOTS le impostazioni restano in memoria.
# uso: bench/fire_settings.sh [WORKDIR]
#   → WORKDIR/fire-settings-{dark,light}.png (il pannello, ritagliato) e WORKDIR/fire-<colore>.png
#   (il fondo del documento col fuoco acceso, scuro). Esce 1 se un colore non viene scelto.
set -eu
cd "$(dirname "$0")/.."
cargo build --profile perf -q
W=${1:-$(mktemp -d)}; mkdir -p "$W"
W=$(cd "$W" && pwd -P)
mkdir -p "$W/root"
fail() { echo "fire_settings: $*" >&2; exit 1; }
for theme in dark light; do
  sed "s|COLOR|Fuoco|" tests/scripts/fire_settings.json > "$W/script.json"
  cp fixtures/normal.md "$W/doc.md"
  RUNEMD_ROOTS="$W/root" RUNE_DIRS_ROOT="$W/dirs" target/perf/runemd "$W/doc.md" --script "$W/script.json" \
    --size 1280x860 --scale 2 --theme $theme --fps 30 --at mark:panel+0.3 --snap "$W/fire-settings-$theme.png" \
    --crop settings --tree "$W/panel-$theme.json" >/dev/null 2>&1
  for t in Intensità Altezza Velocità Colore "Accento di sistema" "100 %" "110 pt" "1×"; do
    python3 -c 'import json,sys; sys.exit(not any(e.get("text")==sys.argv[2] for e in json.load(open(sys.argv[1]))["elements"]))' \
      "$W/panel-$theme.json" "$t" || fail "$theme: manca \"$t\""
  done
done
for c in Fuoco:fire Blu:blue Viola:violet Verde:green "Accento di sistema:accent"; do
  label=${c%%:*}; name=${c##*:}
  sed "s|COLOR|$label|" tests/scripts/fire_settings.json > "$W/script-$name.json"
  cp fixtures/normal.md "$W/$name.md"
  RUNEMD_ROOTS="$W/root" RUNE_DIRS_ROOT="$W/dirs" target/perf/runemd "$W/$name.md" --script "$W/script-$name.json" \
    --realtime --size 1280x860 --scale 2 --theme dark --fps 30 --at mark:lit+0.3 --snap "$W/full-$name.png" --crop main \
    --tree "$W/lit-$name.json" 2>/dev/null |
  while read -r line; do
    case "$line" in "rune-mark picked"*) printf '\nUn paragrafo nuovo in fondo.\n\nE un altro ancora.\n' >> "$W/$name.md" ;; esac
  done
  python3 -c 'import json,sys; sys.exit(not any(e.get("key")=="fire" for e in json.load(open(sys.argv[1]))["elements"]))' \
    "$W/lit-$name.json" || fail "$label: niente fuoco"
  ffmpeg -loglevel error -y -i "$W/full-$name.png" -vf "crop=iw:480:0:ih-480" "$W/fire-$name.png"
done
echo "foto: $W/fire-settings-{dark,light}.png $W/fire-{fire,blue,violet,green,accent}.png"
