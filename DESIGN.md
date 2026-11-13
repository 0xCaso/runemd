---
name: RuneMD
description: Lettore Markdown per Mac, in Rune, per i file che gli agenti scrivono nei repo.
colors:
  panel-light: "oklch(0.995 0.001 270)"
  panel-dark: "oklch(0.155 0.004 270)"
  fg-light: "oklch(0.22 0.006 270)"
  fg-dark: "oklch(0.95 0.003 270)"
  fg2-light: "oklch(0.36 0.008 270)"
  fg2-dark: "oklch(0.80 0.006 270)"
  fg3-light: "oklch(0.45 0.008 270)"
  fg3-dark: "oklch(0.69 0.008 270)"
  body-light: "oklch(0.28 0.006 270)"
  body-dark: "oklch(0.90 0.004 270)"
  glass-light: "oklch(0.965 0.004 260 / 0.86)"
  glass-dark: "oklch(0.18 0.01 260 / 0.84)"
  line-light: "oklch(0 0 0 / 0.09)"
  line-dark: "oklch(1 0 0 / 0.08)"
  sel-light: "oklch(0 0 0 / 0.06)"
  sel-dark: "oklch(1 0 0 / 0.09)"
  field-light: "oklch(1 0 0 / 0.55)"
  field-dark: "oklch(0 0 0 / 0.22)"
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
  ui-small:
    fontFamily: "Inter"
    fontSize: "12px"
    fontWeight: 500
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
- Tre segnali live, ognuno con un solo significato: metallo, fuoco, dither.
- Densità da app Mac: righe da 28 px, testo UI 13 px; il documento respira (15,5 px, interlinea 1,62).

## Colors

Una scala di grigi OKLCH a bassissimo croma, in due temi. In Rune si scrivono uguali con `oklcha(l, c, h, a)`.

| Ruolo | Chiaro | Scuro | Uso |
|---|---|---|---|
| `panel` | 0.995 | 0.155 | Pannello del documento |
| `fg` | 0.22 | 0.95 | Titoli, nomi file, testo forte |
| `body` | 0.28 | 0.90 | Corpo del documento |
| `fg-2` | 0.36 | 0.80 | Intestazioni di tabella, campo "Apri file" |
| `fg-3` | 0.45 | 0.69 | Tempi, branch, etichette di sezione, meta del file |
| `glass` | 0.965 / 86 % | 0.18 / 84 % | Cornice e sidebar, con blur 40 e saturazione 1,6 |
| `line` | nero 9 % | bianco 8 % | Righe di tabella, linee guida dell'albero |
| `sel` | nero 6 % | bianco 9 % | Riga selezionata, hover |

**Contrasto misurato** (WCAG, sui pixel degli screenshot `shots/scheda-{light,dark}-lettura.png`, 13/11/2026):
`fg-3` sul pannello 5,4:1 in entrambi i temi; `fg-3` sul vetro della sidebar ≥ 4,5:1 nei punti peggiori dello
sfondo (con `--glass` all'86 % / 84 %). Abbassare l'opacità del vetro sotto questi valori fa scendere `fg-3`
sotto 4,5:1 (misurato: 2,1:1 con opacità 0,62).

Nell'app il vetro è `El::system_material` (Liquid Glass su macOS 26+): il sistema decide la tinta. Se il
contrasto di `fg-3` scende sotto 4,5:1 sul materiale vero, si alza `fg-3`, non si toglie il vetro.

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

## Layout

- Finestra: sidebar di vetro larga 248 px a sinistra; il pannello del documento sta a 8 px dai bordi alto,
  destro e basso, e il vetro lo circonda su tutti e quattro i lati.
- Sidebar, dall'alto: semafori; campo "Apri file ⌘P"; sezione "Recenti" (nome file e tempo relativo a
  destra); sezione "Repo del file" (riga intestazione del repo con icona cartella, nome e branch, poi i file
  indentati di 16 px con linea guida verticale).
- Pannello: barra delle schede in alto (nomi dei file aperti, "⌘E sorgente" a destra); colonna del documento
  larga al massimo 680 px e centrata; indice "In questo file" a destra, separato da una linea.
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

**Campo "Apri file".** Alto 30 px, raggio 7, riempimento `field`, bordo interno 0,5 px a `ink` 10 %. Icona,
testo e "⌘P" in `fg-2` (deve leggersi: commento di Matteo).

**Scheda.** Testo UI 13 px. La scheda del file attivo ha riempimento `sel`.

**Segnali live (effetti GPU di Rune).** Ognuno ha un solo significato, e nessun altro elemento lo usa.

| Effetto | Significato | Dove | Inizio | Fine |
|---|---|---|---|---|
| Metallo liquido (anello su 4 lati, 1,5 px, giro di 3 s) | Un agente sta scrivendo questo file | Scheda del file, sua riga in "Recenti", sua riga nell'albero | Appare in 220 ms ease-out alla prima modifica | Si ferma quando il file non cambia più (ipotesi: 3 s senza scritture) |
| Fuoco di Rune (fiamme e braci, alto 110 px) | C'è testo nuovo sotto la parte che stai leggendo | Fondo del pannello, con l'etichetta "N paragrafi nuovi più in basso" | Sale in 420 ms ease-out | Sparisce quando scorri fino in fondo |
| Dither (Bayer 4×4, pulsa a 4 passi in 1,6 s) | Questo blocco sta arrivando | Al posto dei blocchi non ancora scritti, larghi come una riga di testo | Subito | Quando il blocco è completo |

Nel tema chiaro il fuoco usa la luminanza come maschera (sul bianco lo screen sparirebbe) e il dither è
invertito. Con "riduci movimento" l'anello e il dither non si animano (restano fermi e visibili) e il fuoco
compare senza salire.

## Do's and Don'ts

**Do**
- Tenere fermo il pannello del testo: niente effetti sul documento, salvo il dither sui blocchi in arrivo.
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
- Effetti GPU (metallo, fuoco, dither): Impeccable e il "no decorazione" li sospetterebbero; qui ognuno porta
  uno stato e sparisce con lo stato.
- Press scale 0,98 (Emil 0,97, `better-ui` 0,96): righe dense da 28 px, una scala più forte sembra un salto.
