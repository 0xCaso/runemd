# Runner unfollow: report

Il progetto sta in `src/live/`. Il profilo di prova è in `bench/edit_big.json`, i passi in `bench/scroll.sh`.

## File toccati

- `src/live/below.rs:24`: il calcolo del margine, spostato qui
- `src/live/mod.rs` e `src/ui/mod.rs`: solo gli export
- [la decisione](../docs/decisions/2026-11-23-t3-style-paths.md)
- `docs/proof/caret-dark-append.png` e `Cargo.toml`

Il vecchio `src/old/gone.rs` non c'è più: niente icona, niente clic.

## Esito

| File | Righe | Esito |
|---|--:|---|
| `src/live/below.rs` | 412 | test verdi |
| `bench/scroll.sh` | 38 | nuovo |
| `bench/edit_big.json` | 12 | tetto 200 al giorno |

## Come si usa

```bash
# solo la prima volta
npm install
npx playwright install chromium

# prova senza togliere nessuno
npm run dry
```

```src/live/below.rs
pub fn below(view: &View) -> Option<usize> {
    view.last_read().map(|b| view.blocks().len() - b) // paragrafi nuovi, contati dall'ultimo blocco letto fino alla fine
}
```

Il kit Rune viene da [anomalyco/rune](https://github.com/anomalyco/rune); la guida di Playwright è su [playwright.dev](https://playwright.dev).
