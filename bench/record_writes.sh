#!/bin/sh
# Usage: bench/record_writes.sh DIR SECONDS [AGENT]
# Records writes to *.md under DIR into bench/results/writes-<AGENT>.log
# with the polling recorder (no sudo). See docs/agent-writes.md for its limits.
set -eu
DIR=$1; SECS=$2; AGENT=${3:-agent}
HERE=$(cd "$(dirname "$0")" && pwd)
mkdir -p "$HERE/results"
exec python3 -I "$HERE/record_writes.py" "$DIR" "$SECS" "$HERE/results/writes-$AGENT.log"
