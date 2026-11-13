# Perf: compositor rune-fx a riposo (task 2)

Build `make app` (profilo perf), WORKDIR `/tmp/runemd-bench` (fuori da Desktop: TCC), Mac con schermo bloccato.

## Avvio, warm, 5 giri (`bench/startup.sh /tmp/runemd-bench 5 warm`)

| file | min | p50 | max | soglia | base task 1 (p50) |
|---|---|---|---|---|---|
| normal | 225 | 236 | 251 | 350 | 244 |
| big | 222 | 245 | 292 | 350 | 242 |

Soglia rispettata: nessun costo misurabile (il renderer rune-fx è creato al primo frame, nessuna pipeline è compilata senza layer).

## Scroll

| prova | stato |
|---|---|
| scroll | da misurare (schermo bloccato il 13/11), comando: `make app && mkdir -p /tmp/runemd-bench && cp -R build/RuneMD.app fixtures/normal.md fixtures/big.md /tmp/runemd-bench/ && bench/scroll.sh /tmp/runemd-bench 3` (soglia: refresh persi <= 3 %) |

Con lo schermo bloccato (`CGSSessionScreenIsLocked = true`) la finestra e' occlusa e ogni frame e' `Skipped`
(`frames=0`), anche senza compositor. A riposo `FxCompositor::render` senza layer chiama `Frame::draw_ui`
(frame di default di Rune); `animating()` e `draws_own()` sono falsi.

## Scelte

- Anelli delle schede da 100 (non da 1): gli id 1 e 2 sono di fuoco e dither.
- Nessun thread di warm-up: rune-fx non espone un warm per tipo; senza layer nessuna pipeline viene compilata.
  I task 11-13 decidono se scaldare al primo `set(Some)`.

# Ricarica del file aperto (task 10)

## Costo di una ricarica di `fixtures/big.md` (1 MB)

`cargo test --profile perf reload_cost -- --ignored --nocapture` (`src/live/reload.rs`, 40 giri per caso, Mac con
altri processi attivi: i p95 oscillano fra le corse; qui la corsa più quieta di tre).

| caso | tutto sul thread UI p50 / p95 | thread UI dopo il fix (snapshot + commit) p50 / p95 | thread a parte (`read`) p50 / p95 |
|---|---|---|---|
| append di un paragrafo | 4,96 / 19,23 ms | 0,03 / 0,17 ms | 6,12 / 13,04 ms |
| ogni blocco cambiato (Myers peggiore) | 13,42 / 26,15 ms | 0,03 / 0,24 ms | 13,96 / 25,82 ms |

Soglia 8 ms di p95 superata in tutte le corse: lettura, UTF-8, hash, divisione in blocchi e diff (`ids::assign`)
girano su un thread a parte (`live::reload::read`); sul thread della UI resta `Document::commit`, che sostituisce i
blocchi se la revisione è ancora quella dello snapshot (altrimenti si rilegge). Il thread sveglia l'app con il `Waker`.
