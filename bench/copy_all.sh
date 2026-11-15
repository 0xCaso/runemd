#!/bin/sh
# Cmd+A, Cmd+C copy all the Markdown source, also with rows out of view.
# usage: bench/copy_all.sh [WORKDIR]
#
# 1. Binding check: the headless tests in `src/ui/doc_view.rs` (`copy_*`) open the document
#    in the app view and read Rune's test clipboard after Cmd+A, Cmd+C.
# 2. Real app: `--script` does Cmd+A, Cmd+C on `fixtures/big.md`, then `pbpaste | diff`. With the
#    screen locked the system clipboard may not change: the script says so.
# Exits 0 if tests and diff pass, 1 if a test or the diff fails, 2 if the app did not write
# the clipboard (not verified).
set -eu
cd "$(dirname "$0")/.."
cargo test --profile perf ui::doc_view::tests::copy_
cargo build --profile perf -q
W=${1:-$(mktemp -d)}
printf 'sentinella' | pbcopy
target/perf/runemd fixtures/big.md --script tests/scripts/copy_all.json --scale 1 --fps 30 \
  --at mark:copied+0.2 --tree "$W/tree.json" >/dev/null 2>&1
if [ "$(pbpaste | head -c 10)" = "sentinella" ]; then
  echo "app: not verified: system clipboard not written (screen locked?); the headless test counts" >&2
  exit 2
fi
pbpaste | diff - fixtures/big.md && echo "app: empty diff"
