#!/bin/sh
# "Reduce motion" (task 28), headless and in real time: a shell writes a sentence at the end of the file
# without a newline, so the ring (tab and Recent) and the caret (after the sentence in the
# middle) light up, and on a long file also the fire (new text further down). Two photos 0.8 s
# apart ("lit" and "later"), on `fixtures/normal.md` (ring and fire) and on a short file (ring
# and caret, which is out of view on the long file): with RUNEMD_REDUCE_MOTION=1 they are equal pixel by
# pixel (ring and caret still, fire at full height from the start); without it, they change.
# uso: bench/reduce_motion.sh [WORKDIR]
#   with SNAP=docs/proof/reduce-motion saves SNAP.png (dark, reduced, --scale 2, cropped to the
#   panel: the sidebar shows file names), SNAP-moving.png (the same instant without reduction)
#   and SNAP-caret.png (short file, reduced).
# Exits 1 if the reduced photos change or if the normal ones do not change.
set -eu
cd "$(dirname "$0")/.."
cargo build --profile perf -q
if [ -n "${1:-}" ]; then
  W=$1; mkdir -p "$W"
else
  W=$(mktemp -d); trap 'rm -rf "$W"' EXIT
fi
W=$(cd "$W" && pwd -P)
mkdir -p "$W/root"
printf '# Short note\n\nA short file: the caret sits after the new sentence.\n' > "$W/short.md"

# shot SRC REDUCE MARK OUT: a photo of the panel at MARK+0.3 with RUNEMD_REDUCE_MOTION=REDUCE.
shot() {
  src=$1
  shift
  cp "$src" "$W/root/doc.md"
  RUNEMD_REDUCE_MOTION=$1 RUNEMD_THEME=dark RUNEMD_ROOTS="$W/root" RUNE_DIRS_ROOT="$W/dirs" \
    target/perf/runemd "$W/root/doc.md" --script tests/scripts/reduce_motion.json --realtime --size 1100x900 \
    --scale 2 --fps 30 --at "mark:$2+0.3" --snap "$3" --crop main 2>/dev/null |
  while read -r line; do
    case "$line" in "rune-mark ready"*) printf '\nAn agent is writing this sentence and has not' >> "$W/root/doc.md" ;; esac
  done
}
same() { cmp -s "$1" "$2"; }

for f in normal short; do
  case $f in normal) src=fixtures/normal.md ;; short) src=$W/short.md ;; esac
  shot "$src" 1 lit "$W/$f-r1.png"; shot "$src" 1 later "$W/$f-r2.png"
  same "$W/$f-r1.png" "$W/$f-r2.png" || { echo "reduce_motion: $f, with reduction the photos change" >&2; exit 1; }
  shot "$src" 0 lit "$W/$f-m1.png"; shot "$src" 0 later "$W/$f-m2.png"
  ! same "$W/$f-m1.png" "$W/$f-m2.png" || { echo "reduce_motion: $f, without reduction the photos do not change" >&2; exit 1; }
  echo "$f: reduced, lit and later equal pixel by pixel; normal, different"
done
if [ -n "${SNAP:-}" ]; then
  cp "$W/normal-r1.png" "$SNAP.png"
  cp "$W/normal-m1.png" "$SNAP-moving.png"
  cp "$W/short-r1.png" "$SNAP-caret.png"
  echo "photos: $SNAP.png $SNAP-moving.png $SNAP-caret.png"
fi
