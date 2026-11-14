#!/bin/sh
# Cmd+W con modifiche non salvate nell'app vera (record 2026-11-14-cmdw-modifiche-non-salvate):
# 1. senza modifiche Cmd+W chiude subito;
# 2. con modifiche compare "Salvare le modifiche a c.md?" e niente si scrive; un secondo Cmd+W
#    non chiude; Esc toglie la riga e lascia le modifiche; Cmd+W e Invio salvano e chiudono;
# 3. "Non salvare" chiude senza scrivere;
# 4. "Salva" con il file cambiato da una shell: la scheda resta con la riga del conflitto.
# uso: bench/close_ask.sh [WORKDIR]
#   con SNAP=docs/proof/close-ask salva la riga in SNAP-dark.png e SNAP-light.png (--scale 2,
#   ritagliate al pannello: la sidebar mostra nomi di file reali).
# Esce 1 al primo controllo che non passa.
set -eu
cd "$(dirname "$0")/.."
cargo build --profile perf -q
W=${1:-$(mktemp -d)}
start='# Chiusura\n\nPrimo paragrafo.\n\nSecondo paragrafo.\n'
agent='# Chiusura\n\nPrimo paragrafo.\n\nScritto da un agente.\n'
fail() { echo "close_ask: $*" >&2; exit 1; }

# run SCRIPT MARK TREE [altri flag]: gira lo script su un c.md nuovo fino a MARK+0.3.
run() {
  script=$1 mark=$2 tree=$3
  shift 3
  printf "$start" > "$W/c.md"
  target/perf/runemd "$W/c.md" --script "tests/scripts/$script.json" --scale 1 --fps 30 \
    --at "mark:$mark+0.3" --tree "$W/$tree.json" "$@" 2>/dev/null >/dev/null
}

# has TREE TESTO...: vero se ogni testo è nell'albero.
has() {
  tree=$1
  shift
  python3 -c '
import json, sys
t = {x.get("text") for x in json.load(open(sys.argv[1]))["elements"]}
sys.exit(0 if set(sys.argv[2:]) <= t else 1)' "$W/$tree.json" "$@"
}
# tabs TREE: quante schede ci sono.
tabs() {
  python3 -c '
import json, sys
print(sum(1 for x in json.load(open(sys.argv[1]))["elements"] if x.get("role") == "tab"))' "$W/$1.json"
}
disk_is() { printf "$1" | cmp -s - "$W/c.md"; }

ask='Salvare le modifiche a c.md?'

run close_clean closed clean
[ "$(tabs clean)" = 0 ] || fail "senza modifiche Cmd+W non chiude"
disk_is "$start" || fail "senza modifiche il disco è cambiato"
echo "senza modifiche: chiude subito"

run close_ask asked asked
has asked "$ask" Salva "Non salvare" Annulla || fail "la riga non c'è"
[ "$(tabs asked)" = 1 ] || fail "Cmd+W con modifiche ha chiuso"
disk_is "$start" || fail "Cmd+W ha scritto"
run close_ask again again
has again "$ask" && [ "$(tabs again)" = 1 ] || fail "il secondo Cmd+W ha chiuso o tolto la riga"
run close_ask cancelled cancelled
! has cancelled "$ask" && [ "$(tabs cancelled)" = 1 ] || fail "Esc: la riga resta o la scheda è chiusa"
disk_is "$start" || fail "Esc ha scritto"
echo "con modifiche: chiede, il secondo Cmd+W non chiude, Esc annulla"
run close_ask saved saved
[ "$(tabs saved)" = 0 ] || fail "Invio non chiude"
disk_is "Nota mia. $start" || fail "Invio non salva"
echo "Invio: salva e chiude"

run close_discard discarded discarded
[ "$(tabs discarded)" = 0 ] || fail "Non salvare non chiude"
disk_is "$start" || fail "Non salvare ha scritto"
echo "Non salvare: chiude, disco invariato"

# Il conflitto: una shell riscrive il file quando la riga è aperta (tempo vero).
printf "$start" > "$W/c.md"
target/perf/runemd "$W/c.md" --script tests/scripts/close_conflict.json --realtime --scale 1 --fps 30 \
  --at mark:conflict+0.3 --tree "$W/conflict.json" 2>/dev/null |
while read -r line; do
  case "$line" in "rune-mark asked"*) printf "$agent" > "$W/c.md" ;; esac
done
disk_is "$agent" || fail "Salva ha scritto sopra la shell"
has conflict "Il file è cambiato su disco" Ricarica "Tieni il mio" && ! has conflict "$ask" || fail "Salva con conflitto: niente riga del conflitto"
[ "$(tabs conflict)" = 1 ] || fail "Salva con conflitto ha chiuso"
echo "Salva con conflitto: resta aperta con la riga del conflitto"

if [ -n "${SNAP:-}" ]; then
  for theme in dark light; do
    printf "$start" > "$W/c.md"
    RUNEMD_THEME=$theme target/perf/runemd "$W/c.md" --script tests/scripts/close_ask.json --scale 2 --fps 30 \
      --at mark:asked+0.3 --snap "$SNAP-$theme.png" --crop main 2>/dev/null >/dev/null
  done
  echo "foto: $SNAP-dark.png $SNAP-light.png"
fi
