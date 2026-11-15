#!/bin/sh
# Cmd+W with unsaved changes in the real app (record 2026-11-14-cmdw-unsaved-changes):
# 1. without changes Cmd+W closes at once;
# 2. with changes "Save changes to c.md?" shows and nothing is written; a second Cmd+W
#    does not close; Esc removes the row and keeps the changes; Cmd+W and Return save and close;
# 3. "Don't save" closes without writing;
# 4. "Save" with the file changed by a shell: the tab stays with the conflict row.
# usage: bench/close_ask.sh [WORKDIR]
#   with SNAP=docs/proof/close-ask saves the row in SNAP-dark.png and SNAP-light.png (--scale 2,
#   cropped to the panel: the sidebar shows real file names).
# Exits 1 at the first check that fails.
set -eu
cd "$(dirname "$0")/.."
cargo build --profile perf -q
W=${1:-$(mktemp -d)}
start='# Chiusura\n\nPrimo paragrafo.\n\nSecondo paragrafo.\n'
agent='# Chiusura\n\nPrimo paragrafo.\n\nScritto da un agente.\n'
fail() { echo "close_ask: $*" >&2; exit 1; }

# run SCRIPT MARK TREE [more flags]: runs the script on a new c.md up to MARK+0.3.
run() {
  script=$1 mark=$2 tree=$3
  shift 3
  printf "$start" > "$W/c.md"
  target/perf/runemd "$W/c.md" --script "tests/scripts/$script.json" --scale 1 --fps 30 \
    --at "mark:$mark+0.3" --tree "$W/$tree.json" "$@" 2>/dev/null >/dev/null
}

# has TREE TEXT...: true if each text is in the tree.
has() {
  tree=$1
  shift
  python3 -c '
import json, sys
t = {x.get("text") for x in json.load(open(sys.argv[1]))["elements"]}
sys.exit(0 if set(sys.argv[2:]) <= t else 1)' "$W/$tree.json" "$@"
}
# tabs TREE: how many tabs there are.
tabs() {
  python3 -c '
import json, sys
print(sum(1 for x in json.load(open(sys.argv[1]))["elements"] if x.get("role") == "tab"))' "$W/$1.json"
}
disk_is() { printf "$1" | cmp -s - "$W/c.md"; }

ask='Save changes to c.md?'

run close_clean closed clean
[ "$(tabs clean)" = 0 ] || fail "without changes Cmd+W does not close"
disk_is "$start" || fail "without changes the disk changed"
echo "without changes: closes at once"

run close_ask asked asked
has asked "$ask" Save "Don't save" Cancel || fail "the row is missing"
[ "$(tabs asked)" = 1 ] || fail "Cmd+W with changes closed"
disk_is "$start" || fail "Cmd+W wrote"
run close_ask again again
has again "$ask" && [ "$(tabs again)" = 1 ] || fail "the second Cmd+W closed or removed the row"
run close_ask cancelled cancelled
! has cancelled "$ask" && [ "$(tabs cancelled)" = 1 ] || fail "Esc: the row stays or the tab is closed"
disk_is "$start" || fail "Esc wrote"
echo "with changes: asks, the second Cmd+W does not close, Esc cancels"
run close_ask saved saved
[ "$(tabs saved)" = 0 ] || fail "Return does not close"
disk_is "My note. $start" || fail "Return does not save"
echo "Return: saves and closes"

run close_discard discarded discarded
[ "$(tabs discarded)" = 0 ] || fail "Don't save does not close"
disk_is "$start" || fail "Don't save wrote"
echo "Don't save: closes, disk unchanged"

# The conflict: a shell rewrites the file while the row is open (real time).
printf "$start" > "$W/c.md"
target/perf/runemd "$W/c.md" --script tests/scripts/close_conflict.json --realtime --scale 1 --fps 30 \
  --at mark:conflict+0.3 --tree "$W/conflict.json" 2>/dev/null |
while read -r line; do
  case "$line" in "rune-mark asked"*) printf "$agent" > "$W/c.md" ;; esac
done
disk_is "$agent" || fail "Save wrote over the shell"
has conflict "The file changed on disk" Reload "Keep mine" && ! has conflict "$ask" || fail "Save with conflict: no conflict row"
[ "$(tabs conflict)" = 1 ] || fail "Save with conflict closed"
echo "Save with conflict: stays open with the conflict row"

if [ -n "${SNAP:-}" ]; then
  for theme in dark light; do
    printf "$start" > "$W/c.md"
    RUNEMD_THEME=$theme target/perf/runemd "$W/c.md" --script tests/scripts/close_ask.json --scale 2 --fps 30 \
      --at mark:asked+0.3 --snap "$SNAP-$theme.png" --crop main 2>/dev/null >/dev/null
  done
  echo "photos: $SNAP-dark.png $SNAP-light.png"
fi
