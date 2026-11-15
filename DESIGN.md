---
name: RuneMD
description: Lettore Markdown per Mac, in Rune, per i file che gli agenti scrivono nei repo.
colors:
  panel-light: "oklch(0.995 0.001 270)"
  panel-dark: "oklch(0.155 0.004 270)"
  fg-light: "oklch(0.22 0.006 270)"
  fg-dark: "oklch(0.95 0.003 270)"
  fg2-light: "oklch(0.36 0.008 270)"
  fg2-dark: "oklch(0.82 0.006 270)"
  fg3-light: "oklch(0.45 0.008 270)"
  fg3-dark: "oklch(0.72 0.008 270)"
  fg2-glass-light: "oklch(0.25 0.008 270)"
  fg2-glass-dark: "oklch(0.90 0.006 270)"
  fg3-glass-light: "oklch(0.28 0.008 270)"
  fg3-glass-dark: "oklch(0.86 0.008 270)"
  body-light: "oklch(0.28 0.006 270)"
  body-dark: "oklch(0.90 0.004 270)"
  glass-light: "oklch(0.965 0.004 260 / 0.60)"
  glass-dark: "oklch(0.18 0.01 260 / 0.60)"
  line-light: "oklch(0 0 0 / 0.09)"
  line-dark: "oklch(1 0 0 / 0.14)"
  sel-light: "oklch(0 0 0 / 0.06)"
  sel-dark: "oklch(1 0 0 / 0.12)"
  field-light: "oklch(1 0 0 / 0.55)"
  field-dark: "oklch(0 0 0 / 0.22)"
  mark-light: "oklch(0.45 0.008 270 / 0.70)"
  mark-dark: "oklch(0.72 0.008 270 / 0.65)"
typography:
  doc-title:
    fontFamily: "Inter"
    fontSize: "28px"
    fontWeight: 680
    lineHeight: 1.2
    letterSpacing: "-0.015em"
  doc-heading:
    fontFamily: "Inter"
    fontSize: "19px"
    fontWeight: 640
    lineHeight: 1.3
    letterSpacing: "-0.005em"
  doc-body:
    fontFamily: "Inter"
    fontSize: "15.5px"
    fontWeight: 400
    lineHeight: 1.62
  doc-table:
    fontFamily: "Inter"
    fontSize: "14px"
    fontWeight: 400
    fontFeature: "tnum"
  doc-code:
    fontFamily: "ui-monospace, SF Mono, Menlo"
    fontSize: "13.5px"
  ui:
    fontFamily: "Inter"
    fontSize: "13px"
    fontWeight: 400
  outline:
    fontFamily: "Inter"
    fontSize: "12.5px"
    fontWeight: 400
    lineHeight: 2
  ui-small:
    fontFamily: "Inter"
    fontSize: "12px"
    fontWeight: 500
  kbd:
    fontFamily: "Inter"
    fontSize: "11px"
    fontWeight: 400
rounded:
  kbd: "4px"
  row: "6px"
  field: "7px"
  panel: "10px"
spacing:
  frame: "8px"
  sidebar: "248px"
  column: "680px"
  row: "28px"
  tree-row: "26px"
  tree-indent: "16px"
  sidebar-top: "44px"
  sidebar-pad: "8px"
  sidebar-bottom: "10px"
  field: "30px"
  field-margin: "0 2px 6px"
  row-pad: "10px"
  tree-row-pad: "8px"
  row-gap: "8px"
  qualifier-gap: "6px"
  icon: "14px"
  chevron: "10px"
  section-pad: "14px 10px 6px"
  kbd-pad: "1px 5px"
  hairline: "0.5px"
  tab-bar: "44px"
  tab-bar-pad: "0 10px"
  tab-gap: "2px"
  tab-max: "240px"
  recent-gap: "3px"
  recent-row: "44px"
  recent-share: "40%"
  find-field: "240px"
  find-ring: "1px"
  find-ring-active: "2px"
  outline: "196px"
  outline-pad: "31px 10px 24px"
  outline-row: "25px"
  outline-row-pad: "0 6px"
  outline-indent: "10px"
  outline-label-gap: "4px"
  outline-min-window: "1128px"
motion:
  press: "scale 0.98, 160ms ease-out"
  hover: "120ms ease"
  ease-out: "cubic-bezier(0.23, 1, 0.32, 1)"
  mode: "lettura ↔ sorgente: dissolvenza incrociata, molla critica 0.22 s entra (95 % a ~165 ms), 0.17 s esce (< 5 % a ~130 ms)"
components:
  sidebar-row:
    height: "{spacing.row}"
    rounded: "{rounded.row}"
    padding: "0 10px"
    typography: "{typography.ui}"
  sidebar-row-selected:
    backgroundColor: "{colors.sel-dark}"
  open-file-field:
    backgroundColor: "{colors.field-dark}"
    textColor: "{colors.fg2-dark}"
    rounded: "{rounded.field}"
    height: "30px"
  doc-panel:
    backgroundColor: "{colors.panel-dark}"
    rounded: "{rounded.panel}"
---

# Design System: RuneMD

## Overview

**Creative North Star: "Il foglio fermo"**

RuneMD serve a leggere i file Markdown che gli agenti scrivono, mentre li scrivono. Il testo sta fermo in un
pannello pieno (nero nel tema scuro, bianco nel chiaro). Tutto il resto, cioè sidebar, schede e cornice, è vetro
traslucido attorno al pannello, come in Current. L'app è calma: nessun colore d'accento, nessuna pillola di stato,
nessuna statistica. Gli unici elementi che si muovono sono tre effetti GPU di Rune, e ognuno dice una sola cosa.

Schermata di riferimento: variante `scheda` in `design/mockups/2026-11-13-runemd-main/index.html`
(decisione: `docs/decisions/2026-11-13-runemd-schermata-principale.md`).

**Key Characteristics:**
- Due materiali: vetro (cornice, sidebar, barra delle schede) e pannello pieno (documento).
- Grigi neutri con una punta di blu (hue 260–270); nessun accento.
- Tre segnali live, ognuno con un solo significato: metallo, fuoco, caret.
- Densità da app Mac: righe da 28 px, testo UI 13 px; il documento respira (15,5 px, interlinea 1,62).

## Colors

Una scala di grigi OKLCH a bassissimo croma, in due temi. In Rune si scrivono uguali con `oklcha(l, c, h, a)`.

| Ruolo | Chiaro | Scuro | Uso |
|---|---|---|---|
| `panel` | 0.995 | 0.155 | Pannello del documento |
| `fg` | 0.22 | 0.95 | Titoli, nomi file, testo forte |
| `body` | 0.28 | 0.90 | Corpo del documento |
| `fg-2` | 0.36 | 0.82 | Intestazioni di tabella, campo "Apri file" |
| `fg-3` | 0.45 | 0.72 | Branch, etichette di sezione, meta del file (fuori dalle righe con hover: lì `fg-2`) |
| `glass` | 0.965 / 60 % | 0.18 / 60 % | Tinta sopra il vetro di sistema, su cornice e sidebar (scelta di Matteo, 14/11/2026) |
| `line` | nero 9 % | bianco 14 % | Linee solo decorative: righe di tabella, linee guida dell'albero, bordi dei campi |
| `sel` | nero 6 % | bianco 12 % | Riga selezionata, hover, selezione, riempimento dei risultati di Cerca |
| `mark` | `fg-3` chiaro al 70 % | `fg-3` scuro al 65 % | Segni con un significato: anello dei risultati di Cerca, segno "cambiato" |

**Contrasto.** I rapporti di ogni token sono nella sezione "Contrasto" qui sotto. Con la tinta al 60 % i grigi del
pannello sul vetro scendono sotto i minimi (misure sotto, "Vetro vero al 60 %"): sul vetro, cioè in tutta la sidebar
(`Theme::on_glass`), `fg-2` e `fg-3` sono i token `-glass`, più forti; il pannello del documento tiene i suoi.

Nell'app il vetro è `El::system_material` (Liquid Glass su macOS 26+) in una finestra trasparente: Rune accende il
vetro di sistema solo lì, e quando c'è non dipinge il riempimento dell'elemento. La tinta `glass` sta quindi in uno
strato sopra il vetro, a tutta finestra. Senza vetro di sistema (headless, materiale spento) la radice è la tinta
piena (`Theme::glass_fallback`): dark `0f1216`, light `f2f3f6`, come prima. `RUNEMD_GLASS=<percento>` cambia la
tinta per le foto. Se il contrasto di `fg-3` scende sotto 4,5:1 sul materiale vero, si alza `fg-3`, non si toglie
il vetro.

**Vetro vero al 60 %** (14/11/2026, finestra vera, fondo della sidebar misurato su 40×40 px e rapporti WCAG con i
token): sfondo bianco nel tema scuro (vetro `3c3e40`): `fg` 9,28, `fg-2` 6,15, `fg-3` 4,33, `fg-2` su vetro + `sel`
4,27. Sfondo nero nel tema chiaro (`999a9c`): `fg` 6,15, `fg-2` 3,86, `fg-3` 2,64, `fg-2` su vetro + `sel` 3,44.
Sfondo Sonoma: scuro (`354416`) `fg-3` 4,25, `fg-2` + `sel` 4,23; chiaro (`becb9d`) `fg-3` 4,33, `fg-2` + `sel` 5,57.
Questi erano i grigi del pannello. Misurato una seconda volta, il vetro varia di qualche punto (scuro `3e3f40` e
`374618`, chiaro `9e9fa1` e `bfcd9f`): il test usa la misura più difficile di ogni caso. Con i token `-glass`
(scuro `fg-2` 0.90, `fg-3` 0.86; chiaro `fg-2` 0.25, `fg-3` 0.28):

| Fondo misurato | `fg` | `fg-2` | `fg-3` | `fg-2` + `sel` | `fg-3` + `sel` | Fuoco |
|---|---|---|---|---|---|---|
| Scuro, scrivania bianca (`3e3f40`) | 9,12 | 7,83 | 6,89 | 5,45 | 4,80 | 3,66 |
| Scuro, Sonoma (`374618`) | 8,84 | 7,59 | 6,69 | 5,34 | 4,70 | 3,55 |
| Chiaro, scrivania nera (`999a9c`) | 6,15 | 5,68 | 5,18 | 5,06 | 4,61 | 3,17 |
| Chiaro, Sonoma (`becb9d`) | 10,08 | 9,31 | 8,49 | 8,21 | 7,49 | 5,19 |

### Contrasto

Rapporti WCAG calcolati da `src/ui/contrast.rs` (`cargo test --profile perf contrast_table -- --nocapture`), con
il colore composto sullo sfondo, alpha inclusa, in sRGB codificato come lo compone la finestra. "Vetro" è il fondo
della sidebar misurato nella finestra vera sul fondo peggiore della scrivania (bianco nel tema scuro, `3e3f40`; nero
nel chiaro, `999a9c`; `contrast::GLASS_GROUNDS`, che misura anche Sonoma), con il testo in `Theme::on_glass`. Minimi: 4,5:1 per il testo, 3:1 per i segni con un significato in tutti e due i temi, nessuno per le
linee solo decorative e i riempimenti. I test falliscono se un minimo non è rispettato o se il tema chiaro peggiora.
"Prima" è il 13/11/2026, "dopo" il fix del 14/11/2026 (`docs/decisions/2026-11-14-cerca-contrasto-scuro.md`).

| Cosa | Scuro prima | Scuro dopo | Chiaro prima | Chiaro dopo | Minimo |
|---|---|---|---|---|---|
| `fg` su panel | 16,89 | 16,89 | 17,07 | 17,07 | 4,5 |
| `body` su panel | 14,50 | 14,50 | 14,39 | 14,39 | 4,5 |
| `fg-2` su panel | 10,46 | 11,19 | 10,71 | 10,71 | 4,5 |
| `fg-3` su panel | 7,04 | 7,88 | 7,34 | 7,34 | 4,5 |
| `fg-3` su vetro (`fg-3-glass` dopo) | 4,26 | 6,89 | 4,89 | 5,18 | 4,5 |
| `fg-2` su vetro (`fg-2-glass` dopo) | 6,32 | 7,83 | 7,13 | 5,68 | 4,5 |
| `fg-3-glass` su vetro + `sel` (hover) | — | 4,80 | — | 4,61 | 4,5 |
| `fg-2-glass` su vetro + `sel` (hover) | — | 5,45 | — | 5,06 | 4,5 |
| Nome doppio della riga scelta su vetro + `sel` (`fg-3` prima, `fg-2-glass` dopo) | 3,22 | 5,45 | 4,30 | 5,06 | 4,5 |
| Testo piccolo delle righe premibili (tempo di Recenti, nome doppio, "Apri Impostazioni di Sistema") su vetro + `sel` (hover; `fg-3` prima, `fg-2-glass` dopo) | 3,22 | 5,45 | 4,30 | 5,06 | 4,5 |
| Titolo evidenziato dell'indice (`fg`) su panel + `sel` (hover) | — | 12,53 | — | 14,94 | 4,5 |
| Titoli dell'indice (`fg-2`) su panel + `sel` (hover) | — | 8,30 | — | 9,38 | 4,5 |
| `mark` su panel (anello dei risultati di Cerca, segno "cambiato") | 1,19 | 3,89 | 1,22 | 3,52 | 3 |
| `mark` su `sel` (anello di Cerca, lato interno) | 1,27 | 3,34 | 1,22 | 3,27 | 3 |
| Anello del risultato attivo su `sel` (`fg-3` prima, `fg-2` dopo) | 5,73 | 8,30 | 6,42 | 9,38 | 3 |
| Anello "sta scrivendo" sulla barra delle schede (pixel, minimo, banda 5 px; vedi sotto) | — | 5,01 | 3,17 | 4,74 | 3 |
| Anello "sta scrivendo" sul riempimento `sel` della scheda (pixel, minimo, banda 5 px) | — | 3,73 | 2,78 | 4,16 | 3 |
| Fuoco della tastiera su vetro | 1,77 | 3,66 | 1,68 | 3,17 | 3 |
| Fuoco della tastiera su panel | 2,13 | 6,78 | 1,97 | 8,80 | 3 |
| Impostazioni: scelta fatta e sua spunta (`fg`) su panel + `sel` | — | 12,53 | — | 14,94 | 4,5 |
| Impostazioni: scelte e bottoni (`fg-2`) su panel + `sel` (hover) | — | 8,30 | — | 9,38 | 4,5 |
| Impostazioni: casella spenta, bordo `fg-3` su panel | — | 7,88 | — | 7,34 | 3 |
| Impostazioni: casella accesa (`fg-2`) e sua spunta (`panel`) | — | 11,19 | — | 10,71 | 3 |
| Impostazioni: casella spenta, bordo `fg-3` su panel + `sel` (hover) | — | 5,84 | — | 6,42 | 3 |
| Impostazioni: casella accesa (`fg-2`) su panel + `sel` (hover) | — | 8,30 | — | 9,38 | 3 |
| Impostazioni: binario della larghezza (`fg-3`) e pomello (`fg-2`) su panel | — | 7,88 | — | 7,34 | 3 |
| Impostazioni: riga del fuoco (`fg-2`) su panel + `sel` (hover) | — | 8,30 | — | 9,38 | 4,5 |
| Impostazioni: "Già osservata" (`fg-2`) su panel | — | 11,19 | — | 10,71 | 4,5 |
| Etichetta del fuoco spento (`fg-2`) su panel | — | 11,19 | — | 10,71 | 4,5 |
| Frontmatter (`fg-2`) su panel + `sel` a metà | — | 9,89 | — | 10,03 | 4,5 |
| × della scheda (`fg-2`) su panel + `sel` | — | 8,30 | — | 9,38 | 3 |
| × della scheda (`fg-2`) in hover su panel + `sel` + `sel` | — | 5,67 | — | 8,22 | 3 |
| Pulsante ↔ della larghezza (`fg-2`) su panel | — | 11,19 | — | 10,71 | 3 |
| Pulsante ↔ della larghezza (`fg-2`) su panel + `sel` (hover, popover aperto) | — | 8,30 | — | 9,38 | 3 |
| Popover della larghezza: titolo (`fg-2`) e valore (`fg`) su panel | — | 11,19 / 16,89 | — | 10,71 / 17,07 | 4,5 |
| Popover della larghezza: scorciatoie e binario (`fg-3`) su panel | — | 7,88 | — | 7,34 | 4,5 |
| Pulsante ⌘ delle scorciatoie (`fg-2`) su panel / panel + `sel` | — | 11,19 / 8,30 | — | 10,71 / 9,38 | 3 |
| Popover delle scorciatoie: righe, tasti (`fg-2`) e gruppi (`fg-3`) su panel | — | 11,19 / 7,88 | — | 10,71 / 7,34 | 4,5 |
| ⌘P: nome (`fg`) e repo o cartella e ora (`fg-2`) su panel + `sel` | — | 12,53 / 8,30 | — | 14,94 / 9,38 | 4,5 |
| `line` su panel (linee decorative) | 1,19 | 1,44 | 1,22 | 1,22 | — |
| `line` su vetro (bordo dei campi) | 1,28 | 1,52 | 1,21 | 1,19 | — |
| `sel` su panel | 1,23 | 1,35 | 1,14 | 1,14 | — |
| `sel` accanto al codice in linea | 1,04 | 1,15 | 1,03 | 1,03 | — |

- Il fuoco della tastiera è il colore del sistema (`Cx::focus_color`, AppKit lo dà al 50 %), 3 pt, pieno in
  tutti e due i temi; nel chiaro è anche scurito del 68 % verso il nero e nello scuro schiarito del 10 % verso il
  bianco (in luce lineare), perché sul vetro vero al 60 % il blu scurito del 20 % dava 1,70:1 nel chiaro e il blu
  pieno 2,94:1 nello scuro. Misurato con il blu predefinito.
- I bordi dei campi restano in `line`: il campo si riconosce dall'icona e dal segnaposto in `fg-2`, non dal bordo.
- L'anello "sta scrivendo" è metallo liquido di Rune (`LiquidMetalParams::chrome`), non un token: non ha un
  colore nostro da regolare. Misurato sui pixel di `docs/proof/contrast-dark-after.png` (scala 2, un fotogramma):
  per ogni punto dei lati dritti attorno alla scheda si prende il pixel più contrastato sui 3 px di spessore; il
  minimo è 4,62:1 sul pannello e 3,44:1 sul `sel` della scheda (mediane 11,37 e 8,46), cioè ≥ 3:1 su tutto il
  perimetro. Lo studio riflesso gira, quindi un altro fotogramma dà valori un po' diversi.
- Nel tema chiaro (review finale I5) lo stesso metodo (`bench/ring_contrast.py`, scala 2, `RUNEMD_THEME=light`,
  scheda di `bench/worst_case.sh` a rect 258,16,240×28 pt) sul cromo di Rune dava minimo 3,17:1 sulla barra e
  2,78:1 sul `sel` (mediane 8,30 e 7,28): sotto 3 sul `sel`. Nel chiaro il metallo è più scuro (`fx::ring::metal`:
  tinta `d8dbe0` e luci `e6e7ea` al posto di `e9ecf0` e `ffffff`); ora minimo 4,86:1 sulla barra e 4,26:1 sul
  `sel` (mediane 11,01 e 9,65), e su un secondo fotogramma 1,5 s dopo 4,77 e 4,19 (mediane 10,54 e 9,24). Prove:
  `docs/proof/ring-light-before.png`, `docs/proof/ring-light-after.png`. Nello scuro il metallo resta il cromo;
  con lo stesso script sulla scheda di `bench/worst_case.sh` dà minimo 4,43 sulla barra e 3,30 sul `sel`
  (mediane 10,66 e 7,93).
- I numeri sopra sono dell'anello di 1,5 px (banda di 3 px a scala 2). Con l'anello di 2,5 px (`fx::ring::RING_W`,
  commit 94c9fa9) la banda è 5 px (`bench/ring_contrast.py`, default; 15/11/2026, `bench/worst_case.sh`, scheda a
  258,16,240×28 pt, due run con lo stesso fotogramma): scuro minimo 5,01 sulla barra e 3,73 sul `sel` (mediane 13,92
  e 10,36), chiaro 4,74 e 4,16 (mediane 13,87 e 12,16); ≥ 3:1 su tutto il perimetro in tutti e due i temi. Rispetto
  ai valori di prima lo scuro sale (4,62 → 5,01, 3,44 → 3,73), il chiaro scende di poco (4,86 → 4,74, 4,26 → 4,16),
  sempre sopra 3. Sullo stesso fotogramma la banda di 3 px darebbe scuro 3,15/2,34 e chiaro 2,43/2,13: vede solo i 3 px
  interni dell'anello, non tutto il suo spessore. Prove: `docs/proof/ring-2-5px-dark.png`,
  `docs/proof/ring-2-5px-light.png`.
- Il nome doppio (repo o cartella) è in `fg-2` sulla riga scelta di ⌘P: in `fg-3` sul `sel` scendeva a 3,2:1.
- Ogni testo piccolo dentro una riga che prende `sel` in hover sul vetro (seconda riga di Recenti, cioè cartella o
  repo e tempo; nome doppio di ⌘P; link "Apri Impostazioni di Sistema") è in `fg-2` (`sidebar::row_meta`): con i grigi del pannello `fg-3` su vetro + `sel`
  dava 3,28:1 nello scuro e 4,30:1 nel chiaro (review finale I4); oggi anche `fg-3-glass` + `sel` passa (4,80 / 4,61). `fg-3` resta solo fuori dalle righe premibili:
  etichette di sezione, branch e icone del titolo dell'albero (che non ha hover), "⌘E sorgente" sul pannello.

## Typography

Inter per tutto, `ui-monospace` per il codice (in Rune: `families: ("Inter", "ui-monospace, Menlo")`, altrimenti
il codice esce in Inter). Cifre tabellari ovunque (`tnum`).

| Ruolo | Dimensione | Peso | Interlinea |
|---|---|---|---|
| Titolo del file (H1) | 28 | 680 | 1,2, tracking −0,015em |
| H2 | 19 | 640 | 1,3 |
| Corpo | 15,5 | 400 | 1,62 |
| Tabella | 14 (intestazioni 13, peso 560) | 400 | — |
| Codice | 13,5 mono | 400 | — |
| UI (righe, schede) | 13 | 400; repo 600 | — |
| UI piccola (tempi, sezioni, meta) | 12 | 500 | — |

Il grassetto nel corpo è peso 620 in `fg`. I titoli non sono enormi: la gerarchia viene dal peso e dal colore.

Una parola più lunga della riga (un percorso, un nome di file) va a capo dopo `/`, `-`, `_` o `.`, così la riga
finisce con il separatore e la seguente comincia con il pezzo dopo; si spezza in un punto qualsiasi solo un tratto
senza separatori più largo di una riga (un hash). Nessuna riga esce dalla colonna. Un URL nudo lungo va a capo
allo stesso modo e resta un link solo, con l'indirizzo intatto.

**Callout** (`> [!NOTE]`, `[!TIP]`, `[!IMPORTANT]`, `[!WARNING]`, `[!CAUTION]`): la citazione con sopra
l'etichetta in parole (Nota, Suggerimento, Importante, Attenzione, Pericolo), 13/600 in `fg`, con l'icona da 14 in
`fg-2` 6 prima; 6 sotto l'etichetta, sulla stessa linea della citazione. Il segno `[!NOTE]` non si vede. Nessun
colore: il tipo lo dicono parola e icona.

**Note a piè di pagina** (`[^1]`): nel testo il numero in apice, in ordine di primo riferimento; le definizioni
non si disegnano dove sono scritte ma in fondo al file, sotto una linea di 0,5 in `line` e 12 di spazio, come lista
numerata a 13,5 in `fg-2`. Un riferimento senza definizione resta scritto com'è.

## Layout

- Finestra: sidebar di vetro larga 248 px a sinistra; il pannello del documento sta a 8 px dai bordi alto,
  destro e basso, e il vetro lo circonda su tutti e quattro i lati.
- Sidebar, dall'alto: semafori; campo "Apri file ⌘P"; sezione "Recenti" (righe su due righe: nome del file
  sopra, cartella o repo e tempo relativo sotto); una linea; il titolo dell'albero (icona repo, nome del repo e
  sotto il branch con la sua icona), poi i file indentati di 16 px con linea guida verticale. Nessuna etichetta
  sopra l'albero: il nome del repo fa da titolo. (`docs/decisions/2026-11-15-recenti-due-righe.md`, mockup A
  in `design/mockups/2026-11-14-recenti-leggibili/`.)
- Recenti prende al massimo il 40 % dell'altezza della sidebar (`recent-share`) e poi scorre, con la barra del
  sistema (sottile, a scomparsa); 3 px fra le righe (`recent-gap`), con o senza anelli. Con righe da 44 ci
  stanno circa 5 file (6 in una sidebar alta 800) invece di 7–8; l'ordine resta quello d'apertura. Aprendo un file l'albero
  scorre quanto basta per mostrarne la riga. (D1, D2: `docs/decisions/2026-11-14-sidebar-schede-nomi-lunghi.md`.)
- Nomi lunghi di file (Recenti, albero, schede, ⌘P) e il branch si tagliano in mezzo: restano la fine e
  l'estensione (`2026-11-14-arc…definitiva.md`). Il nome del repo si taglia in fondo. (D4)
- Pannello: barra delle schede in alto (nomi dei file aperti, "⌘E sorgente" a destra); colonna del documento
  larga al massimo 680 px e centrata; indice "In questo file" a destra, separato da una linea.
- L'indice "In questo file" si nasconde quando la finestra è più stretta di sidebar + colonna + 200 (1128 pt,
  `outline-min-window`), e nel sorgente (⌘E).
- Sotto il titolo, una riga meta: cartella, stato ("modificato 2 min fa" / "si aggiorna"), parole. Gli elementi
  si separano con spazio (16 px), mai con " · ".

## Elevation & Depth

Piatto. L'unica profondità è il pannello sul vetro: bordo di 0,5 px a `ink` 10 % e un'ombra morbida
(`0 10px 30px` nero 35 %). Niente ombre su righe, schede o campi.

## Shapes

Raggi piccoli e costanti: pannello 10, campo 7, riga 6, tasto ⌘ 4. Le linee sono di 0,5 px.

## Components

**Riga della sidebar.** Alta 28 px (26 nell'albero), raggio 6. Hover `sel`, solo con puntatore fine. Premuta:
scala 0,98 in 160 ms ease-out. Selezionata: riempimento `sel`, testo `fg`.
Misure della sidebar (dal mockup `scheda`, ora token in `spacing`): spazio sopra di 44 per i semafori,
margini 8 ai lati e 10 in fondo, margine interno delle righe 10 (8 nell'albero), 8 fra icona, nome e tempo,
6 fra nome e repo/cartella di un nome doppio, icone da 14 e freccia delle cartelle da 10, etichette di sezione
con 14 sopra e 6 sotto. Il riempimento `sel` dell'hover entra ed esce in 120 ms (`ease`); la scala della
riga premuta va e torna in 160 ms con la curva `motion.ease-out`. Con "riduci movimento" tutti e due sono
immediati.

**Riga di Recenti.** Alta 44 (`recent-row`), raggio 6, margine 10 ai lati, hover `sel` e scala premuta come le
altre righe. Prima riga: il nome del file (UI 13/400, interlinea 16) in `fg`, largo quanto la riga; si taglia in
mezzo solo se non ci sta. Seconda riga, 2 sotto (interlinea 15): a sinistra dove sta il file, 12/400, tagliato
in fondo; a destra il tempo relativo, 12/500 cifre tabellari; 8 fra i due. Tutti e due in `fg-2` (`fg-2-glass`,
`sidebar::row_meta`), anche in hover. Dove sta il file: il repo (la cartella del file fuori da un repo); per un
nome che ritorna, il repo o la cartella che lo distingue (regola dei nomi doppi). (Record
`2026-11-15-recenti-due-righe`, vincoli 1–2.)

**Titolo dell'albero (repo).** Al posto dell'etichetta "Repo del file", sotto una linea 0,5 in `line` (12 sopra,
8 sotto, 10 ai lati). Icona repo (un libro, 14) in `fg-3` a sinistra, centrata sulle due righe; 8 dopo, il nome
13/600 in `fg` (interlinea 16, tagliato in fondo) e 1 sotto l'icona branch (12) e il branch, 12/400 in `fg-3`
(interlinea 15, tagliato in mezzo, senza cifre tabellari: i trattini restano stretti), 4 fra icona e branch. Alta
almeno 44, 4 sopra e sotto. La riga non ha hover, quindi `fg-3` sta sul vetro (`fg-3-glass`, 6,89:1 / 5,18:1 sul
fondo peggiore misurato). Le due icone sono disegnate come quelle del kit (viewBox 16, tratto 1, giunti tondi;
quella del branch, più piccola, con tratto 1,2 per lo stesso spessore). Fuori da un repo il titolo è la cartella
del file: icona cartella e nome, niente branch, alta almeno 30. (D3; record `2026-11-15-recenti-due-righe`,
vincoli 4–5.)

**Campo "Apri file".** Alto 30 px, raggio 7, riempimento `field`, bordo interno 0,5 px in `line`. Icona,
testo e "⌘P" in `fg-2` (deve leggersi: commento di Matteo). Il tasto "⌘P" è testo `kbd` (11 px, 400) con
bordo 0,5 px in `line`, raggio 4 e margine interno `kbd-pad`; il campo ha margine `field-margin`.

**Apri veloce (⌘P).** Il campo "Apri file" è un campo di testo: ⌘P gli dà il fuoco. Finché la ricerca non è
vuota, la lista dei risultati prende il posto di Recenti e dell'albero. Le righe sono quelle della sidebar (28 px,
raggio 6, hover `sel`, scala 0,98 premuta); la riga scelta con le frecce ha riempimento `sel` e testo `fg`. Un nome
doppio mostra repo o cartella in `fg-2`, come in Recenti. Invio apre, Esc svuota e chiude, nessuna animazione di
apertura (azione da tastiera). Nessun accento: caret `fg`, selezione `sel`. Nessun risultato: "Nessun file" in `fg-3`.

**Scheda.** Una riga come quelle della sidebar, larga quanto il nome del file fino a 240 (`tab-max`, D5; oltre,
il nome si taglia in mezzo): alta 28 (`row`), raggio 6, testo UI
13 px con 10 ai lati (`row-pad`), hover `sel`, scala 0,98 premuta. La scheda del file attivo ha riempimento `sel` e
testo `fg`; le altre testo `fg-2`. Barra alta 44 (`tab-bar`), margine 10 ai lati (`tab-bar-pad`), 2 fra le schede
(`tab-gap`), linea di 0,5 in `line` sotto; nessun separatore né icona. In fondo alla scheda una "×" (glifo 14 in
`fg-2`, area di 16, raggio 4, hover `sel`, 4 dopo il nome e 6 dal bordo destro): si vede sempre sulla scheda
davanti e sulle altre solo in hover (o col fuoco della tastiera), ma il suo posto c'è sempre, così il nome non si
sposta. Etichetta "Chiudi <nome>"; un clic chiude come ⌘W (stessa domanda con modifiche non salvate); il clic
centrale resta. Contrasto del glifo: 8,30 / 9,38 su `sel`, 5,67 / 8,22 in hover sulla ×. Scheda di anteprima (clic
singolo nella sidebar, come VS Code): stesso aspetto, nome in corsivo; al massimo una, il clic singolo dopo la
sostituisce nello stesso posto. Troppe schede: la riga scorre di lato. "⌘E
sorgente" a destra come il tasto "⌘P", in `fg-3`.

**Larghezza del testo (pulsante ↔).** Nella barra delle schede, subito a sinistra di "⌘E sorgente": 28 × 28
(`row`), raggio 6, glifo "↔" 15 in `fg-2`, hover `sel`; col popover aperto ha il riempimento `sel`. Etichetta
"Larghezza del testo". Un clic apre sotto il pulsante, allineato al suo bordo destro e 4 più in basso, un popover
largo 248 (`sidebar`): riempimento `panel`, raggio 10, bordo 0,5 a `ink` 10 % e l'ombra del pannello (le stesse
della sola profondità che c'è); dentro "Larghezza del testo" (UI 13 in `fg-2`), lo stesso cursore a passi delle
impostazioni e "⌘⌥+ e ⌘⌥− un passo alla volta" (12 in `fg-3`). Il valore è uno solo, `settings.column`: cursore,
impostazioni e scorciatoie lo cambiano insieme e si salva. Esc, un clic fuori o il pulsante lo chiudono.
⌘⌥+ (anche ⌘⌥=) e ⌘⌥− fanno un passo anche col popover chiuso. Contrasto: glifo 11,19 / 10,71 su panel e
8,30 / 9,38 su `sel`; scorciatoie 7,88 / 7,34.

**Scorciatoie (pulsante ⌘).** Subito a sinistra di ↔, uguale a lui: 28 × 28, glifo "⌘" 15 in `fg-2`, hover
`sel`, `sel` pieno col popover aperto, tooltip ed etichetta "Scorciatoie" (anche ↔ ha il suo tooltip, "Larghezza
del testo"). Un clic apre sotto il pulsante il popover della larghezza con un altro contenuto: largo 300, margini
12 / 14, "Scorciatoie" (13 in `fg-2`), i gruppi File, Schede, Lettura, App (12 peso 500 in `fg-3`) e righe da 24
con l'azione (13 in `fg-2`) e i tasti a destra (11,5 in `fg-2`, bordo `line` 0,5, raggio 4). Esc, un clic fuori o
il pulsante lo chiudono; aprire ⌘ chiude ↔ e viceversa. L'elenco è `ui::shortcuts::GROUPS`
(docs/decisions/2026-11-15-scorciatoie-popover.md).

**Apri file (⌘P).** Un pannello largo 560 (meno con la finestra stretta, 16 dai bordi), centrato nella finestra,
20 sotto la barra delle schede, sopra un velo `panel` al 60 %: non sposta niente. Riempimento `panel`, raggio 10,
bordo e ombra del popover. In alto il campo (48, testo 15, icona in `fg-2`), poi le righe su una riga sola (40: icona
del file, nome in `fg`, subito dopo repo o cartella in `fg-2`, l'ora in `fg-2` in fondo a destra; se lo spazio non
basta si accorcia prima repo o cartella, fino a 40, poi il nome con il taglio in mezzo), scelta e hover `sel`, otto in vista; in fondo
(36) ↑ ↓ scegli, ↵ apri, Esc chiudi (tasti in `fg-2`, parole in `fg-3`) e "N file". Con la ricerca vuota le righe
sono Recenti. Il campo "Apri file ⌘P" della sidebar è un pulsante che apre il pannello. Clic: anteprima, il pannello
resta; doppio clic: scheda fissa e il pannello si chiude; Invio apre in anteprima e chiude. Esc o un clic fuori
chiudono (pannello modale). Record docs/decisions/2026-11-15-quick-open-compatto.md e
docs/decisions/2026-11-15-quick-open-una-riga.md (una riga sola; i Recenti nella sidebar restano su due righe).

**Cerca nel file (⌘F).** Solo in lettura. Il campo sta nella barra delle schede, a destra delle schede e prima
di "⌘E sorgente", così il documento non si sposta: largo 240 (`find-field`), alto 30, raggio 7, riempimento
`field`, bordo interno 0,5 px in `line`, 8 a destra (`row-gap`); icona lente e segnaposto "Cerca nel file" in
`fg-2`, testo `fg`. Il contatore a destra è UI piccola in `fg-3`: "3 di 12", oppure "Nessun risultato"; con la
ricerca vuota non c'è. Nel testo i risultati hanno il riempimento
`sel` e un anello interno di 1 px in `mark` (`find-ring`), che li stacca dal codice in linea e dalla selezione;
quello attivo ha l'anello in `fg-2` e di 2 px (`find-ring-active`), più scuro e spesso il doppio, così si
riconosce a colpo d'occhio in tutti e due i temi. L'anello `mark` è a 3,9:1 sul pannello nel tema scuro e a 3,5:1 nel chiaro. Nessun accento, nessun giallo. Invio va al risultato dopo, Maiusc+Invio a
quello prima, in giro; la vista lo porta al centro. Esc chiude e toglie i segni. Il campo di ⌘P, quello di ⌘N e
questo si chiudono a vicenda.

**Indice dei titoli ("In questo file").** Dal mockup `scheda`: colonna larga 196 (`outline`) a destra del
documento, separata da una linea di 0,5 in `line` alta quanto il documento. Etichetta "In questo file" in UI
piccola `fg-3`, 4 sopra il primo titolo (`outline-label-gap`), allineata alla prima riga del documento (31 dal
bordo alto, cioè il margine del documento più quello della riga). I titoli H1–H3 del file, uno per riga: 12,5 px,
interlinea 2 (righe alte 25, `outline-row`), testo `fg-2` troncato con "…"; il testo sta a 16 dalla linea (10 di
margine della colonna più 6 dentro la riga, `outline-row-pad`, che lascia spazio all'hover). Ogni livello sotto il
più alto del file rientra di 10 (`outline-indent`). Il titolo della sezione che si legge (l'ultimo sopra il primo
blocco visibile) è in `fg` e peso 600: colore e peso insieme, perché `fg` e `fg-2` da soli si distinguono poco nel
tema scuro. Nessun segno né accento. Hover `sel`, scala 0,98 premuta, come le righe della sidebar. Un clic porta il
titolo in cima alla vista, senza planata (come Cerca: la riga può non essere montata), e quel titolo resta
evidenziato anche se non arriva in cima, finché si vede e non si scorre a mano. Con lo scroll in fondo è evidenziato
l'ultimo titolo che si vede: le sezioni corte in fondo non arrivano mai in cima. Se l'indice è più alto
della finestra scorre da solo e l'evidenziato resta in vista. Un file senza titoli non ha la colonna. Nel sorgente
(⌘E) l'indice non c'è: i blocchi a cui porta non esistono lì e i titoli cambiano mentre si scrive.

**Frontmatter.** Un file che comincia al byte 0 con una riga `---` e ha una riga di chiusura `---` o `...`
(`doc::frontmatter`) mostra in lettura il YAML fra le due righe come un solo blocco di metadati: mono 12 (UI
piccola), testo `fg-2`, riempimento `sel` a metà, raggio 6, margine interno 12 × 10, nella colonna come un
blocco di codice ma più quieto. L'indice dei titoli non lo elenca; il sorgente (⌘E) resta il file com'è. Senza
riga di chiusura è Markdown normale (riga orizzontale, paragrafi).

**Lettura ↔ sorgente (⌘E).** Dissolvenza incrociata: la vista che entra arriva al 95 % in ~165 ms, quella che
esce scende sotto il 5 % in ~130 ms (molle criticamente smorzate di 0,22 e 0,17 s, `motion.mode`: Rune anima
entrate e uscite solo con molle, e una molla critica ha già la coda di una ease-out forte). Prima era la
dissolvenza di default di Rune (0,30 / 0,24 s: 95 % a ~230 ms). Con "riduci movimento" è un taglio netto.

**Riga di avviso (sorgente).** Sotto la barra delle schede, sopra il sorgente: alta 44 (`tab-bar`), testo UI
13 px in `fg` allineato a quello del sorgente, bottoni a destra come schede in secondo piano (alti 28, `row-pad`
ai lati, testo `fg-2`, hover `sel`, scala 0,98 premuta), linea di 0,5 in `line` sotto. È annunciata come avviso
(`alert`). Dice "Il file è cambiato su disco" ("Ricarica", "Tieni il mio") o "Salvataggio non riuscito" quando un
salvataggio non scrive. Alla chiusura (⌘W o clic centrale su una scheda con modifiche non salvate) la scheda viene
davanti e la stessa riga chiede "Salvare le modifiche a <nome file>?" con "Salva" (anche Invio), "Non salvare" e
"Annulla" (anche Esc); il campo tiene il fuoco. Senza modifiche ⌘W chiude subito. Chiudere la finestra o uscire
salva tutto, senza chiedere (Rune non può annullare la chiusura). Nessun accento.

**Impostazioni (⌘,).** Nessun mockup: solo token. Colonna larga come la sidebar (248, `sidebar`) a destra del
documento, nel pannello, al posto dell'indice dei titoli, separata da una linea di 0,5 in `line`; ⌘, la apre e la
chiude, Esc la chiude. Titolo "Impostazioni" (UI, 600, `fg`), poi sezioni con etichetta come quelle della sidebar:
"Cartelle osservate" (icona cartella in `fg-3`, percorso in `fg-2` con la home come "~", "Togli" a destra;
"Aggiungi cartella…" apre il dialogo delle cartelle; una cartella già osservata, o sotto una osservata, non si
aggiunge e sotto compare "Già osservata" in `fg-2` per 3 s o fino alla prossima azione), "Font" (Inter, SF Pro, New York se installato col suo nome,
poi le famiglie installate; lista virtuale alta 8 righe e mezza, così la nona tagliata dice che scorre), "Dimensione" ("−", "15,5 pt", "+"; titoli, tabelle e
codice scalano col corpo), "Larghezza del testo" (cursore a passi da 680 a 1240 di 80 in 80, l'ultimo "Intera": la colonna segue il pannello, i margini delle righe restano; binario di 2 in `fg-3`, parte scelta e pomello di 12 in `fg-2`, valore a destra in `fg` con cifre tabulari; frecce un passo alla volta; il blocco in cima alla vista resta in cima), "Tema" ("Sistema", "Chiaro", "Scuro"), la casella "Fuoco in fondo al pannello" (accende e spegne il fuoco, a riposo e col testo nuovo) e sotto
l'aspetto del fuoco: "Intensità" (40–175 %), "Altezza" (55–176 pt col fuoco acceso, 110 il default), "Velocità"
(0,25–3×), tre cursori a passi uguali a quello della larghezza con l'etichetta 13 in `fg-2` sopra, e "Colore"
("Fuoco", "Blu", "Viola", "Verde", "Accento di sistema", scelte come quelle del tema). I default sono il fuoco di
sempre. La parte calda di ogni colore resta chiara, così l'etichetta del testo nuovo sopra le fiamme tiene 4,5:1.
Con il fuoco spento questi controlli sono al 40 % e disabilitati (fuori dal giro del Tab, senza hover né clic):
un controllo disabilitato non ha obbligo di contrasto. Le
righe sono quelle della sidebar (28, raggio 6, hover `sel`, scala 0,98). La scelta fatta ha riempimento `sel`,
testo `fg` e una spunta in `fg`; le altre testo `fg-2`. La casella ha il bordo di 1 in `fg-3`; accesa è piena in
`fg-2` con la spunta in `panel`. Con il fuoco spento l'etichetta "N paragrafi nuovi più in basso" resta, in `fg-2`
su una fascia `panel` alta 44 con la linea `line` sopra. Tutto cambia subito, senza riavvio.

**Larghezza di sidebar e indice.** Il bordo destro della sidebar e quello sinistro dell'indice si trascinano: una
fascia invisibile di 8 nel margine del pannello, cursore ↔. Sidebar 180–420 (default 248), indice 150–360 (default
196); la soglia dell'indice si sposta con le due larghezze, e il trascinamento si ferma prima che l'indice si
nasconda (senza indice, il pannello resta largo almeno 400). Lo scatto impara come Snappy: lasciando il bordo la
larghezza si ricorda (le ultime 4); passando a meno di 8 da una ricordata, o dal default, il bordo si ferma lì.
Doppio clic: il default. Esc durante il trascinamento: com'era. Le larghezze restano dopo il riavvio.

**Segnali live (effetti GPU di Rune).** Ognuno ha un solo significato, e nessun altro elemento lo usa.

| Effetto | Significato | Dove | Inizio | Fine |
|---|---|---|---|---|
| Metallo liquido (anello su 4 lati, 2,5 px, giro di 3 s) | Un agente sta scrivendo questo file | Scheda del file, sua riga in "Recenti", sua riga nell'albero | Appare in 220 ms ease-out alla prima modifica | Si ferma quando il file non cambia più (ipotesi: 3 s senza scritture) |
| Fuoco di Rune (fiamme e braci) | A riposo, basso (26 px): è il fondo del pannello. Alto 110 px: c'è testo nuovo sotto la parte che stai leggendo | Fondo del pannello, sempre in lettura; alto, con l'etichetta "N paragrafi nuovi più in basso" | Da riposo sale a 110 in 420 ms ease-out | Torna a riposo in 300 ms ease-out quando scorri fino al testo nuovo |
| Caret (2 px, `fg`, pulsa a 4 passi in 1,6 s: opacità 100/81/62/81 %) | Qui un agente ha appena scritto | Nel punto dell'ultima modifica, anche a metà file: dopo l'ultima parola cambiata (o aggiunta in fondo), alto quanto la lettera; fuori dal flusso, il testo non si sposta | Alla prima ricarica da fuori | Con l'anello (il file non cambia più); se è fuori vista la vista non salta |

Nel tema chiaro il fuoco usa la luminanza come maschera (sul bianco lo screen sparirebbe). Il caret al passo
più tenue resta ≥ 4,5:1 sul pannello e sul `sel` (`ui::contrast`): il minimo è 62 % e non il 50 % del
record `caret-piu-forte`, perché al 50 % fa 3,32:1 nel chiaro (4,30:1 nello scuro su `sel`); al 62 %, pannello e `sel`: chiaro
4,83/4,60, scuro 6,80/5,78 (minimo). Con "riduci movimento" l'anello e il caret
non si animano (restano fermi e visibili, il caret pieno) e il fuoco
è fermo, a riposo o alto, senza salire né scendere; con la finestra non attiva è fermo allo stesso modo. A riposo
le fiamme non tengono il display link: si ridisegnano 12 volte al secondo (CPU a riposo ~3,5 % contro ~11 % a
120 Hz, misure in `.superpowers/sdd/plan/v11-batch-a-report.md`).

## Do's and Don'ts

**Do**
- Tenere fermo il pannello del testo: niente effetti sul documento, salvo il caret nel punto dell'ultima modifica.
- Dare a ogni effetto un solo significato; un nuovo stato prende un segnale nuovo o nessuno.
- Scrivere i numeri con cifre tabellari, allineati a destra in tabella.
- Misurare il contrasto sul materiale vero ogni volta che cambia un grigio o l'opacità del vetro.

**Don't** (base anti-slop di `app-design`, più le regole di RuneMD)
- Niente " · " o "—" per unire meta informazioni: spazio, campi con nome o parole.
- Niente etichette sopra un titolo che ripetono il contesto; niente etichette MAIUSCOLE spaziate.
- Niente card dentro card, niente griglie di card uguali, niente sfumature decorative.
- Un solo accento al massimo, solo per azioni, selezione e stato. RuneMD oggi non ne ha.
- Niente etichette riempitive, niente "→" in coda ai bottoni, niente testo che spiega ciò che la UI già mostra.
- Niente bagliori fluo, pillole di stato, pallini luminosi o barre arancioni (commento di Matteo).
- Niente righe di statistiche decorative ("Oggi 165 file…").
- Il repo non sta allo stesso livello dei suoi file.

**Eccezioni volute alle regole delle skill**
- Vetro su cornice e sidebar: Impeccable lo vieta, qui è l'identità (Current) ed è materiale di sistema.
- Effetti GPU (metallo, fuoco, caret): Impeccable e il "no decorazione" li sospetterebbero; qui ognuno porta
  uno stato e sparisce con lo stato.
- Press scale 0,98 (Emil 0,97, `better-ui` 0,96): righe dense da 28 px, una scala più forte sembra un salto.
