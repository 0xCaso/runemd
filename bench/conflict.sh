#!/bin/sh
# Conflict with the disk in the real app: Cmd+E, typing, a shell rewrites the file, Cmd+S →
# the row "The file changed on disk" shows and the disk keeps the shell's text; "Keep
# mine" → the disk has the local text.
# usage: bench/conflict.sh [WORKDIR]   (with SNAP=docs/proof/conflict.png saves the row)
# Exits 1 if the save writes over the shell or if "Keep mine" does not write.
set -eu
cd "$(dirname "$0")/.."
cargo build --profile perf -q
W=${1:-$(mktemp -d)}
start='# Conflitto\n\nPrimo paragrafo.\n\nSecondo paragrafo.\n'
agent='# Conflitto\n\nPrimo paragrafo.\n\nScritto da un agente.\n'
printf "$start" > "$W/c.md"
# First part: up to the row, with the photo if asked.
if [ -n "${SNAP:-}" ]; then
  snap="--snap $SNAP"
else
  snap=""
fi
target/perf/runemd "$W/c.md" --script tests/scripts/conflict.json --realtime --scale 2 --fps 30 \
  --at mark:row+0.3 $snap --tree "$W/row.json" 2>/dev/null |
while read -r line; do
  case "$line" in "rune-mark typed"*) printf "$agent" > "$W/c.md" ;; esac
done
printf "$agent" | cmp -s - "$W/c.md" || { echo "conflict: the save wrote over the shell" >&2; exit 1; }
python3 -c '
import json, sys
t = {x.get("text") for x in json.load(open(sys.argv[1]))["elements"]}
want = {"The file changed on disk", "Reload", "Keep mine"}
sys.exit(0 if want <= t else 1)' "$W/row.json" || { echo "conflict: the row is missing" >&2; exit 1; }
echo "row: ok, disk: the shell's"
# Second part: up to "Keep mine".
printf "$start" > "$W/c.md"
target/perf/runemd "$W/c.md" --script tests/scripts/conflict.json --realtime --scale 1 --fps 30 \
  --at mark:kept+0.3 --tree "$W/kept.json" 2>/dev/null |
while read -r line; do
  case "$line" in "rune-mark typed"*) printf "$agent" > "$W/c.md" ;; esac
done
printf "My note. $start" | cmp -s - "$W/c.md" || { echo "Keep mine: the disk does not have the local text" >&2; cat "$W/c.md" >&2; exit 1; }
echo "Keep mine: the disk has the local text"
