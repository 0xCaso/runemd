# Task 12: report

Ho spostato il calcolo in src/live/below.rs:88 e aggiornato `src/ui/doc_view.rs`. Il record è docs/decisions/2026-11-15-ordine-lavori-dopo-v1.md; la prova è in ~/Desktop/GitHub/runemd/docs/proof/fire-accent.png.

Il vecchio src/old/gone.rs non c'è più: niente linea, niente clic.

**Files:**

- Create: `src/live/below.rs`, `src/fx/fire.rs`
- Modify: `src/ui/doc_view.rs`, `Cargo.toml`

| File | Righe | Esito |
|---|--:|---|
| src/fx/fire.rs | 412 | test verdi |
| bench/fire_settings.sh | 38 | nuovo |

```
cargo test --profile perf below::
cat src/live/below.rs | head -n 40
```

Restano testo: ecc., v1.2, 0.5 s, Node.js e https://github.com/anomalyco/rune (già un link).
