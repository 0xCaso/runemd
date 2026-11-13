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
