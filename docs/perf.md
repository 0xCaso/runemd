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
| scroll, 13/11 23:50, load average 72–79 (altre sessioni attive) | refresh persi: normal 3,0 % [1,3–4,3], big 5,3 % [4,0–6,5]; dt p95 normal 12,9–14,6 ms, big 14,2–16,1 ms (6 run per file, `bench/results/scroll-2026-11-13-sotto-carico.log`) |
| scroll, soglia | **non valida**: misura sotto carico; lo spike dava 2,4 % / 2,6 % a macchina scarica. Da ripetere a load average basso prima della fine della fase 2 (stesso comando: `make app && mkdir -p /tmp/runemd-bench && cp -R build/RuneMD.app fixtures/normal.md fixtures/big.md /tmp/runemd-bench/ && bench/scroll.sh /tmp/runemd-bench 3`) |

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

## Scrittura → frame (finestra vera)

13/11 23:48, schermo sbloccato, `MDSPIKE_LATENCY`, 25 append a big.md ogni 200 ms (24 campioni): p50 **91,6 ms**,
p95 106,7 ms, max 106,7 ms. Soglia ≤ 150 ms rispettata (stesso carico della misura scroll).
