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

# Indice dei Markdown (task 14)

14/11 01:05, Mac con altre sessioni attive: **load average 95–110** (`uptime` prima e dopo). `~/Desktop/GitHub`:
37 voci, 29 con `.git`.

## Scansione e cache

`cargo test --profile perf scan_cost -- --ignored --nocapture` (`src/index/mod.rs`, 5 giri, cache in una cartella
temporanea).

| giro | voci | scansione | salvataggio cache | ricarica cache | `recent(20)` |
|---|---|---|---|---|---|
| 1 (il più vicino a freddo: page cache del disco non tutta calda) | 6916 | 2801 ms | 11,2 ms | 4,4 ms | 0,04 ms |
| 2–5 | 6916 | 574–624 ms | 6,0–9,8 ms | 2,2–3,1 ms | 0,02 ms |

Cache: 1059 KB di JSON. "Fredda" qui vuol dire senza cache dell'indice; la cache del disco non si svuota senza
`sudo purge`. Le voci in cartelle nascoste (entrano, spec "Osservare i file"): `.worktrees` 1299, `.claude` 587,
`.superpowers` 483, `.repos` 388, `.agents` 27, `.build` 26, `.venv` 16, altre ≤ 4.

## Indice pronto rispetto al primo frame (app vera)

`RUNE_DIRS_ROOT=<tmp>/dirs MDSPIKE_LOG=<tmp>/runN.log build/RuneMD.app/Contents/MacOS/runemd fixtures/normal.md`
lanciato da terminale (stessi permessi TCC), ucciso dopo 5 s; 4 giri, il primo senza cache.

| giro | cache letta prima del primo frame | voci dalla cache | scansione finita dopo il primo frame |
|---|---|---|---|
| 1 (senza cache) | 0,14 ms | 0 | 1710 ms |
| 2 | 3,13 ms | 6916 | 1668 ms |
| 3 | 3,22 ms | 6916 | 1548 ms |
| 4 | 3,01 ms | 6916 | 1579 ms |

Con la cache l'indice è pieno già al primo frame (la cache si legge in `init`, 3 ms): soglia "sidebar riempita ≤ 1 s
dopo il primo frame con l'indice salvato" rispettata per i dati; la sidebar che li mostra arriva col task 16. Senza
cache l'indice arriva con la scansione, 1,5–1,7 s dopo il primo frame a questo carico (non è la soglia, che vale con
l'indice salvato). Avvio al primo frame (`bench/startup.sh`) non rimisurato: a load average 100 il numero non vale.

## Thread della UI dopo il fix round 1 (rescan e cartelle spostate)

14/11 01:18, **load average 92** (`uptime`), `~/Desktop/GitHub`, 6918 voci.

| misura | comando | p50 | p95 | max |
|---|---|---|---|---|
| un rescan del watcher (`Core::rescan` + `drain`, radice ricorsiva) | `cargo test --profile perf rescan_ui_cost -- --ignored --nocapture` (40 giri) | 0,000 ms | 0,001 ms | 0,014 ms |
| raccolta della scansione intera (`Index::finish_scan`) | `cargo test --profile perf scan_pickup_cost -- --ignored --nocapture` (40 giri) | 1,40 ms | 1,63 ms | 1,73 ms |
| copia delle voci per il salvataggio (`Library::save`) | stesso comando | 0,14 ms | 0,32 ms | 0,56 ms |

Prima del fix il rescan percorreva `~/Desktop/GitHub` sul thread della UI (≈ 0,6 s a caldo, 2,8 s a freddo, più
≈ 6900 stat nel watcher e altrettante nell'indice). Ora una radice ricorsiva diventa un `FsEvent::Rescan` e la
scansione gira sul thread `runemd-index`; ordinamento e dedup stanno lì, il thread della UI fonde soltanto. App
vera dopo il fix (stesso comando di sopra, 2 giri): cache 3,08 ms prima del primo frame con 6919 voci; scansione
finita 1,1–1,2 s dopo il primo frame.

# Modifica del sorgente (task 22)

`bench/edit_big.sh` (headless, `bench/edit_big.json`): Cmd+E, scroll a metà del sorgente, un clic per il cursore,
200 caratteri uno per frame a 20 fps; p95 del tempo CPU per frame (`quality.cpu_ms` del `--trace`) fra i mark
`typing` e `typed`. Mac con altre sessioni attive; il load average è quello di `uptime` all'avvio di ogni corsa.

| sorgente | file | load average | p50 | p95 | max | soglia |
|---|---|---|---|---|---|---|
| un `text_input` per tutto il file (task 20) | big | 14,9 | 147,0 | **149,4** | 181,4 | 8 ms: no |
| un `text_input` per pezzo in `virtual_rows` (piano B) | big | 16,5 (la più bassa avuta) | 1,26 | **1,57** | 1,79 | 8 ms: sì |
| piano B, altre 3 corse | big | 36–37 | 1,20–1,24 | 1,47–1,53 | 1,74–4,07 | 8 ms: sì |
| piano B | normal | 36,1 | 0,89 | 1,10 | 2,97 | — |

Senza GPU né encode (`cargo test --profile perf source_typing_cost -- --ignored --nocapture`, un tasto e un
`Runtime::frame`, load average 21): big p50 0,56 / p95 0,87 ms (task 20: 148,9 / 164,3 ms), normal 0,57 / 0,80 ms.

Scroll nel sorgente di big con il campo a fuoco (la vista legge `Cx::caret` e rigira a ogni frame di scroll): 240
frame a 60 fps, p50 0,39 / p95 1,22 / max 2,35 ms (load average ~20, script ad hoc, non nel repo).

# Cerca nel file, Cmd+F (task 24)

`cargo test --profile perf find_cost -- --ignored --nocapture` (`src/ui/find.rs`): `fixtures/big.md` (1 MB, 3041
blocchi) sul runtime headless con la vista dell'app; 10 ricerche digitate lettera per lettera, 3 giri (164 tasti,
il primo tasto escluso). 14/11, tre corse, load average 19–26.

Prima, con la ricerca sul thread della UI: `markdown::find` su tutti i blocchi costa ~5 ms per MB anche senza
risultati ("zzzq": 5,3 ms), ~10 ms per "e" (97.692 risultati); `Find::search` da solo p50 6,2 / p95 8,7 ms,
tasto + frame p95 12,5 ms. Soglia (8 ms) superata. Per questo la ricerca gira su un thread a parte.

| misura | p50 | p95 | max |
|---|---|---|---|
| tasto + frame (campo, richiesta al thread) | 0,68–0,72 | 1,27–1,30 | 1,7–2,2 |
| risultati arrivati + frame (segni, vista all'attivo) | 1,32–1,35 | 2,69–2,98 | 13,3–13,5 |
| thread della UI per tasto, il peggiore dei due | 1,32–1,35 | **2,69–2,98** | 13,3–13,5 |
| risposta del thread della ricerca | 5,6 | 8,2–8,6 | 10–12,4 |
| riferimento: salto della vista su una riga non montata, senza Cerca | 6,6–6,9 | 10,6–11,1 | 11,6–25,3 |

Il primo tasto parsa tutti i blocchi sul thread della ricerca (15–17 ms, fuori dal thread della UI). I massimi
oltre 8 ms sono i salti della vista a un risultato lontano: costano come un salto qualsiasi su big.md (ultima riga).

Dopo il fix round 1 (anello `line` sui risultati, passi in coda, risultati legati al documento), 14/11 13:00, load
average 34: tasto + frame p95 1,32 ms; risultati + frame p95 2,84 ms (max 13,4, un salto); thread della UI per tasto
**p95 2,84 ms**; risposta del thread p95 8,4 ms.

# Indice dei titoli (task 25)

14/11, Mac con altre sessioni attive, load average 8–12 (`uptime` stampato da ogni corsa).

## Scroll di big.md, prima e dopo

`bench/outline_scroll.sh` (headless, `tests/scripts/outline_scroll.json`): `fixtures/big.md` (1 MB, 3041 blocchi,
435 titoli), finestra 1280×800 (l'indice si vede), 16 swipe da 4000 pt ogni 0,5 s su `doc`, 60 fps; tempo CPU
per frame (`quality.cpu_ms` del `--trace`) fra i mark `scrolling` e `scrolled`, 504 frame per corsa. "Prima" è il
binario di `247c1ce` (`bench/outline_scroll.sh DIR <binario>`), stessa finestra.

| finestra | build | p50 | p95 | max |
|---|---|---|---|---|
| 1280×800 | prima (3 corse) | 0,41–0,42 | 2,42–2,45 | 3,48–3,78 |
| 1280×800 | dopo, indice visibile (3 corse) | 0,59 | **2,84–2,86** | 4,02–4,20 |
| 900×800 | prima (2 corse) | 0,40 | 2,27–2,33 | 3,59–3,64 |
| 900×800 | dopo, indice nascosto (2 corse) | 0,41–0,47 | 2,19–2,45 | 3,66–4,81 |

Con l'indice visibile lo scroll costa ~0,17 ms in più a frame (p50) e ~0,4 ms al p95: sono le ~120 righe che
Rune tiene montate nella lista virtuale dell'indice (la vista più 2 viste di overscan per lato). Prove fatte: con
la colonna senza righe il costo torna al "prima" (p50 0,43–0,45, p95 2,45–2,52); né una `memo` per riga né righe
senza hover e pressione cambiano il numero, e nemmeno togliere il segnaposto che tiene in vista l'evidenziato.
Il frame resta sotto 3 ms al p95 (budget 8,3 ms a 120 Hz). Con la finestra stretta non cambia niente.

Fix round 1 (albero stabile attraverso la soglia, "in fondo" da `on_layout` della pagina con `live_only`,
titoli calcolati solo con l'indice visibile), stesso comando, load average 17–21:

| finestra | build | p50 | p95 | max |
|---|---|---|---|---|
| 1280×800 | prima (3 corse) | 0,43–0,44 | 2,39–2,51 | 3,38–3,57 |
| 1280×800 | dopo il fix (3 corse) | 0,60–0,64 | 2,80–3,00 | 4,21–5,12 |
| 900×800 | prima (2 corse) | 0,42 | 2,32–2,36 | 3,55–3,74 |
| 900×800 | dopo il fix (2 corse) | 0,42–0,43 | 2,40–2,43 | 3,71–3,89 |

Come prima del fix: il costo resta quello delle righe montate dell'indice; il gestore della pagina, chiamato a
ogni frame di scroll, non si vede nei numeri.

## Titoli all'apertura e a ogni ricarica

`cargo test --profile perf outline_cost -- --ignored --nocapture` (`src/ui/outline.rs`, 20 giri): la scansione
gira una volta per documento e per ricarica (`outline::Cache`, chiave `doc_view::rows_version`), sul thread della
UI alla prima vista che la chiede.

| file | blocchi | titoli | p50 | max |
|---|---|---|---|---|
| big.md, 1 MB | 3041 | 435 | 0,10 ms | 0,13 ms |
| big.md ripetuto, 16 MB | 48 656 | 6960 | 1,81 ms | 2,34 ms |

Prima del filtro setext dal fondo e del caso ATX senza parser la scansione dei 16 MB costava 4,7 ms (p50).
