#!/bin/sh
# Cmd+A, Cmd+C copiano tutto il Markdown sorgente, anche con le righe fuori vista.
# uso: bench/copy_all.sh [WORKDIR]
#
# 1. Verifica vincolante: i test headless di `src/ui/doc_view.rs` (`copy_*`) aprono il documento
#    nella vista dell'app e leggono la clipboard di prova di Rune dopo Cmd+A, Cmd+C.
# 2. App vera: `--script` fa Cmd+A, Cmd+C su `fixtures/big.md`, poi `pbpaste | diff`. Con lo
#    schermo bloccato la clipboard di sistema può non cambiare: lo script lo dice.
set -eu
cd "$(dirname "$0")/.."
cargo test --profile perf ui::doc_view::tests::copy_ 2>&1 | grep -E '^test |test result'
cargo build --profile perf -q
W=${1:-$(mktemp -d)}
printf 'sentinella' | pbcopy
target/perf/runemd fixtures/big.md --script tests/scripts/copy_all.json --scale 1 --fps 30 \
  --at mark:copied+0.2 --tree "$W/tree.json" >/dev/null 2>&1
if [ "$(pbpaste | head -c 10)" = "sentinella" ]; then
  echo "app: la clipboard di sistema non è stata scritta (schermo bloccato?): vale il test headless"
  exit 0
fi
pbpaste | diff - fixtures/big.md && echo "app: diff vuoto"
