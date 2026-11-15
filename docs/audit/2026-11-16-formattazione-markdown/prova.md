# Prova di formattazione

Il file `src/ui/tabs.rs:42` apre la scheda; vedi anche src/main.rs e ~/Desktop/x.md, oppure
docs/decisions/2026-11-15-ordine-lavori-dopo-v1.md e Cargo.toml. Una cartella: `design/mockups/`.

## Liste di compiti

- [ ] **Step 1: test** `reload::tests` in `src/live/reload.rs`
- [x] **Step 2:** implementare
- [ ] Step 3 con un testo lungo che va a capo su due righe per vedere come si allinea la seconda riga sotto la casella

## Liste annidate

1. Primo
   - annidato a
     - annidato b
2. Secondo
   1. sotto-numero

### Titolo 3
#### Titolo 4
##### Titolo 5
###### Titolo 6

| File | Righe | Stato |
|---|---:|---|
| `src/app.rs` | 1204 | ok |
| src/ui/doc_view.rs | 1252 | da vedere |

> Citazione semplice con **grassetto** e `codice`.

> [!NOTE]
> Una nota in stile GitHub.

> [!WARNING]
> Un avviso.

```rust
fn main() { println!("ciao"); }
```

```sh
make check && cargo test --profile perf
```

---

Una nota a piè di pagina[^1] e ~~barrato~~ e *corsivo*.

[^1]: Il testo della nota.

Link lungo: https://github.com/anomalyco/rune/blob/384c2991e582418ba529835279c5876ac3915e02/crates/rune-kit/src/markdown/render.rs

Riga uno del registro
Riga due del registro, a capo singolo
Task 1: BASE root (nessun commit), HEAD 0e0e3b6.
Task 2: BASE 0095282. Report BLOCKED.
