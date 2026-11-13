#!/bin/sh
# Usage: bench/record_writes.sh DIR SECONDS [AGENT]
# Records writes to *.md under DIR into bench/results/writes-<AGENT>.log.
# Default: polling recorder (no sudo). With `sudo -n` available: fs_usage.
set -eu
DIR=$1; SECS=$2; AGENT=${3:-agent}
HERE=$(cd "$(dirname "$0")" && pwd)
mkdir -p "$HERE/results"
OUT="$HERE/results/writes-$AGENT.log"
if sudo -n true 2>/dev/null; then
  sudo fs_usage -w -f filesys 2>/dev/null | grep --line-buffered '\.md' > "$OUT" &
  P=$!; sleep "$SECS"; sudo kill "$P" 2>/dev/null || true
else
  exec python3 -I "$HERE/record_writes.py" "$DIR" "$SECS" "$OUT"
fi
