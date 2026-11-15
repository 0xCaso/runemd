#!/bin/sh
# Constant-speed scroll driven by the app (no permission needed to inject events).
# usage: bench/scroll.sh WORKDIR ROUNDS — 4 s of scroll after 1 s; normal at 1500 pt/s, big at 3000 pt/s.
set -eu
W=$1; N=$2
mkdir -p "$W/logs"
run() { # file speed
  tmp="$W/logs/.scroll"; rm -f "$tmp"
  open -n -W "$W/RuneMD.app" --env MDSPIKE_LOG="$tmp" --env MDSPIKE_SCROLL=4 --env MDSPIKE_SPEED="$2" --args "$W/$1.md"
  grep '^scroll frames' "$tmp" | sed "s/^/$1 /" | tee -a "$W/logs/scroll_summary.log"
  grep '^scroll_dt_ms' "$tmp" >> "$W/logs/scroll_$1.dt"
  sleep 1
}
for i in $(seq "$N"); do run normal 1500; run big 3000; done
