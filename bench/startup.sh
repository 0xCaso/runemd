#!/bin/sh
# Avvio → primo frame con contenuto di RuneMD.
# uso: bench/startup.sh WORKDIR ROUNDS cold|warm
# WORKDIR contiene RuneMD.app (build/RuneMD.app), normal.md, big.md (fuori da Desktop/Documents: TCC blocca).
# cold = cache shader Metal cancellata prima di ogni lancio (non la cache disco: serve sudo purge).
set -eu
W=$1; N=$2; MODE=$3
CACHE=$(getconf DARWIN_USER_CACHE_DIR)
mkdir -p "$W/logs"
now() { python3 -c 'import time;print(f"{time.time()*1000:.1f}")'; }
clear_shaders() {
  id=dev.matteo.runemd
  rm -rf "$CACHE/com.apple.metal/$id" "$CACHE/$id/com.apple.metal" "$CACHE/$id/com.apple.metalfe" "$CACHE/$id/com.apple.gpuarchiver"
}
run() { # file
  log="$W/logs/startup_$1_$MODE.log"; tmp="$W/logs/.one"; rm -f "$tmp"
  [ "$MODE" = cold ] && clear_shaders
  t0=$(now)
  open -n -W "$W/RuneMD.app" --env MDSPIKE_LOG="$tmp" --env MDSPIKE_EXIT=1 --args "$W/$1.md"
  t1=$(sed -n 's/^first_content_epoch_ms=//p' "$tmp" | head -1)
  python3 -c "print(f'{$t1-$t0:.1f}')" >> "$log"
  sleep 1
}
for i in $(seq "$N"); do for f in normal big; do run $f; done; done
for f in "$W"/logs/startup_*_"$MODE".log; do
  printf '%s ' "$(basename "$f" .log)"; sort -n "$f" | awk '{a[NR]=$1} END {printf "n=%d min=%.0f p50=%.0f max=%.0f\n", NR, a[1], a[int((NR+1)/2)], a[NR]}'
done
