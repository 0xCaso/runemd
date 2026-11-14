#!/bin/sh
# Impostazioni (task 27), headless con tests/scripts/settings.json: Cmd+, apre il pannello; American Typewriter
# e "+" cambiano il documento; "Chiaro" cambia il tema; "Aggiungi cartella…" (dialogo finto) e
# "Togli" cambiano le cartelle osservate e Recenti; Esc chiude. Tutto dal vivo, senza riavvio.
# Con RUNEMD_ROOTS le impostazioni restano in memoria: quelle vere non si toccano.
# uso: bench/settings.sh [WORKDIR]
#   con SNAP=docs/proof/settings salva SNAP.png (scuro, appena aperto) e SNAP-light.png (dopo font,
#   dimensione e tema), --scale 2, ritagliate al pannello (la sidebar mostra nomi di file), e
#   SNAP-fire-off.png: fuoco spento dal pannello, due paragrafi scritti in fondo da una shell, il
#   fondo del pannello con l'etichetta e senza fiamme (tempo vero, ritaglio con ffmpeg).
# Esce 1 al primo controllo che non passa.
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
mkdir -p "$W/uno" "$W/due"
printf '# Uno\n\nNella prima cartella.\n' > "$W/uno/uno.md"
printf '# Due\n\nNella seconda cartella.\n' > "$W/due/due.md"
cp fixtures/normal.md "$W/doc.md"
sed "s|ADDROOT|$W/due|" tests/scripts/settings.json > "$W/script.json"

# run MARK [altri flag]
run() {
  mark=$1
  shift
  RUNEMD_ROOTS="$W/uno" RUNE_DIRS_ROOT="$W/dirs" target/perf/runemd "$W/doc.md" --script "$W/script.json" \
    --size 1280x860 --fps 30 --at "mark:$mark+0.3" "$@" 2>/dev/null >/dev/null
}
texts() {
  python3 -c '
import json, sys
print("\n".join(x.get("text") or "" for x in json.load(open(sys.argv[1]))["elements"]))' "$W/$1.json"
}
has() { texts "$1" | grep -qxF -- "$2"; }
# Come il pannello scrive una cartella: la home diventa "~".
lbl() { case "$1" in "$HOME"/*) echo "~${1#"$HOME"}" ;; *) echo "$1" ;; esac; }

run open --scale 1 --tree "$W/open.json"
for t in "Cartelle osservate" Font Dimensione Tema Sistema Chiaro Scuro "Fuoco in fondo al pannello" \
  "Aggiungi cartella…" Inter "SF Pro" "American Typewriter" "15,5 pt" "$(lbl "$W/uno")"; do
  has open "$t" || fail "manca \"$t\" a pannello aperto"
done
has open "uno.md" || fail "Recenti senza uno.md"
echo "Cmd+,: pannello aperto, Recenti con uno.md"
run font --scale 1 --tree "$W/font.json"
has font "17 pt" || fail "la dimensione non è passata a 17"
echo "American Typewriter e +: 17 pt"
run added --scale 1 --tree "$W/added.json"
has added "$(lbl "$W/due")" || fail "la cartella aggiunta non c'è"
has added "due.md" || fail "Recenti senza due.md dopo l'aggiunta"
echo "Aggiungi cartella: due.md in Recenti"
run removed --scale 1 --tree "$W/removed.json"
! has removed "uno.md" || fail "uno.md resta in Recenti dopo Togli"
! has removed "$(lbl "$W/uno")" || fail "la cartella tolta resta nel pannello"
echo "Togli: uno.md esce da Recenti"
run closed --scale 1 --tree "$W/closed.json"
! has closed "Cartelle osservate" || fail "Esc non chiude"
echo "Esc: chiuso"

if [ -n "${SNAP:-}" ]; then
  run open --scale 2 --theme dark --snap "$W/open.png" --crop main
  run light --scale 2 --theme dark --snap "$W/light.png" --crop main
  cp "$W/open.png" "$SNAP.png"
  cp "$W/light.png" "$SNAP-light.png"
  cp fixtures/normal.md "$W/fire.md"
  RUNEMD_ROOTS="$W/uno" RUNE_DIRS_ROOT="$W/dirs" target/perf/runemd "$W/fire.md" --script tests/scripts/settings_fire_off.json \
    --realtime --size 1280x860 --scale 2 --theme dark --fps 30 --at mark:lit+0.3 --snap "$W/fire.png" --crop main \
    --tree "$W/fire.json" 2>/dev/null |
  while read -r line; do
    case "$line" in "rune-mark off"*) printf '\nUn paragrafo nuovo in fondo.\n\nE un altro ancora.\n' >> "$W/fire.md" ;; esac
  done
  has fire "2 paragrafi nuovi più in basso" || fail "fuoco spento: manca l'etichetta"
  python3 -c 'import json,sys; sys.exit(any(x.get("key") == "fire" for x in json.load(open(sys.argv[1]))["elements"]))' "$W/fire.json" ||
    fail "fuoco spento: c'è lo slot delle fiamme"
  ffmpeg -loglevel error -y -i "$W/fire.png" -vf "crop=iw:480:0:ih-480" "$SNAP-fire-off.png"
  echo "foto: $SNAP.png $SNAP-light.png $SNAP-fire-off.png"
fi
