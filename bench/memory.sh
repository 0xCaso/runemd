#!/bin/sh
# Steady-state memory: phys_footprint (`footprint`) 6 s after launch, app at rest.
# uso: bench/memory.sh WORKDIR ROUNDS
set -eu
W=$1; N=$2
mkdir -p "$W/logs"
one() { # file
  open -n "$W/RuneMD.app" --args "$W/$1.md"
  sleep 6
  pid=$(pgrep -nx runemd)
  mb=$(footprint -p "$pid" 2>/dev/null | sed -n 's/.*Footprint: *\([0-9.]*\) \([KMG]B\).*/\1 \2/p' | head -1)
  echo "$1 $mb" | tee -a "$W/logs/memory.log"
  kill "$pid"; sleep 1
}
for i in $(seq "$N"); do for f in normal big; do one $f; done; done
