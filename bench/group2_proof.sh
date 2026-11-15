#!/bin/sh
# Group 2 proofs that have no script of their own, headless: the shortcuts popover
# (record docs/decisions/2026-11-15-shortcuts-popover.md, click on the ⌘ button) and ⌘P on one
# line (record docs/decisions/2026-11-15-quick-open-one-line.md, "dec" typed, second row
# selected), with names as in the mockup and a long name for the cut. The ring: bench/worst_case.sh.
# uso: bench/group2_proof.sh [WORKDIR]  → WORKDIR/{keys-popover,quick-open}-{dark,light}.png
#   Exits 1 if the popover or the panel is missing.
set -eu
cd "$(dirname "$0")/.."
cargo build --profile perf -q
W=${1:-$(mktemp -d)}; mkdir -p "$W"
W=$(cd "$W" && pwd -P)
R="$W/root"; rm -rf "$R"; mkdir -p "$R"
note() { mkdir -p "$(dirname "$R/$1")"; printf '# %s\n\nTest note.\n' "$(basename "$1" .md)" > "$R/$1"; touch -t "$2" "$R/$1"; }
for r in rune-md-spike runemd my-portfolio notes; do mkdir -p "$R/$r/.git"; printf 'ref: refs/heads/main\n' > "$R/$r/.git/HEAD"; done
note rune-md-spike/docs/spike/DECISIONS.md 202611151600
note runemd/docs/decisions/2026-11-15-ordine-lavori-dopo-v1.md 202611151558
note runemd/docs/decisions/2026-11-15-recenti-due-righe.md 202611151500
note runemd/docs/decisions/2026-11-14-cerca-contrasto-scuro.md 202611141500
note my-portfolio/DECISIONS.md 202611131500
note notes/clients/acme/decisioni-cliente.md 202611101500
note runemd/docs/decisions/2026-11-15-decisione-con-un-nome-molto-lungo-che-non-ci-sta-nella-riga-del-pannello-di-apri-file.md 202611091500
open="$R/runemd/docs/decisions/2026-11-15-work-order-after-v1.md"
fail() { echo "group2_proof: $*" >&2; exit 1; }
has() { python3 -c 'import json,sys; e=json.load(open(sys.argv[1]))["elements"]; sys.exit(not any(x.get("key")==sys.argv[2] for x in e))' "$1" "$2"; }
for theme in dark light; do
  RUNEMD_ROOTS="$R" RUNE_DIRS_ROOT="$W/dirs-$theme" target/perf/runemd "$open" --script tests/scripts/keys_popover.json \
    --size 1280x800 --scale 2 --theme $theme --fps 30 --at mark:open+0.3 --snap "$W/keys-popover-$theme.png" \
    --crop main --tree "$W/keys-$theme.json" >/dev/null 2>&1
  has "$W/keys-$theme.json" keys-list || fail "$theme: popover chiuso"
  RUNEMD_ROOTS="$R" RUNE_DIRS_ROOT="$W/dirs-$theme" target/perf/runemd "$open" --script tests/scripts/quick_open_proof.json \
    --size 1280x800 --scale 2 --theme $theme --fps 30 --at mark:typed+0.3 --snap "$W/quick-open-$theme.png" \
    --tree "$W/quick-$theme.json" >/dev/null 2>&1
  has "$W/quick-$theme.json" quick-panel || fail "$theme: ⌘P chiuso"
done
echo "photos: $W/keys-popover-{dark,light}.png $W/quick-open-{dark,light}.png"
