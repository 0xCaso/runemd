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
  body-light: "oklch(0.28 0.006 270)"
  body-dark: "oklch(0.90 0.004 270)"
  glass-light: "oklch(0.965 0.004 260 / 0.86)"
  glass-dark: "oklch(0.18 0.01 260 / 0.84)"
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
| `fg-2` | 0.36 | 0.82 | Intestazioni di tabella, campo "Apri file" |
| `fg-3` | 0.45 | 0.72 | Tempi, branch, etichette di sezione, meta del file |
| `glass` | 0.965 / 86 % | 0.18 / 84 % | Cornice e sidebar, con blur 40 e saturazione 1,6 |
| `line` | nero 9 % | bianco 14 % | Linee solo decorative: righe di tabella, linee guida dell'albero, bordi dei campi |
| `sel` | nero 6 % | bianco 12 % | Riga selezionata, hover, selezione, riempimento dei risultati di Cerca |
| `mark` | `fg-3` chiaro al 70 % | `fg-3` scuro al 65 % | Segni con un significato: anello dei risultati di Cerca, segno "cambiato" |

**Contrasto.** I rapporti di ogni token sono nella sezione "Contrasto" qui sotto. Abbassare l'opacità del vetro
sotto 86 % / 84 % fa scendere `fg-3` sotto 4,5:1 (misurato sui mockup del 13/11/2026: 2,1:1 con opacità 0,62).

Nell'app il vetro è `El::system_material` (Liquid Glass su macOS 26+): il sistema decide la tinta. Se il
contrasto di `fg-3` scende sotto 4,5:1 sul materiale vero, si alza `fg-3`, non si toglie il vetro.

### Contrasto

Rapporti WCAG calcolati da `src/ui/contrast.rs` (`cargo test --profile perf contrast_table -- --nocapture`), con
il colore composto sullo sfondo, alpha inclusa, in sRGB codificato come lo compone la finestra. "Vetro" è il token
`glass` sul fondo peggiore della scrivania (bianco nel tema scuro, nero nel chiaro): il materiale di sistema vero
sta meglio di così. Minimi: 4,5:1 per il testo, 3:1 per i segni con un significato in tutti e due i temi, nessuno per le
linee solo decorative e i riempimenti. I test falliscono se un minimo non è rispettato o se il tema chiaro peggiora.
"Prima" è il 13/11/2026, "dopo" il fix del 14/11/2026 (`docs/decisions/2026-11-14-cerca-contrasto-scuro.md`).

| Cosa | Scuro prima | Scuro dopo | Chiaro prima | Chiaro dopo | Minimo |
|---|---|---|---|---|---|
| `fg` su panel | 16,89 | 16,89 | 17,07 | 17,07 | 4,5 |
| `body` su panel | 14,50 | 14,50 | 14,39 | 14,39 | 4,5 |
| `fg-2` su panel | 10,46 | 11,19 | 10,71 | 10,71 | 4,5 |
| `fg-3` su panel | 7,04 | 7,88 | 7,34 | 7,34 | 4,5 |
| `fg-3` su vetro | 4,26 | 4,76 | 4,89 | 4,89 | 4,5 |
| `fg-2` su vetro | 6,32 | 6,77 | 7,13 | 7,13 | 4,5 |
| Nome doppio della riga scelta su vetro + `sel` (`fg-3` prima, `fg-2` dopo) | 3,22 | 4,66 | 4,30 | 6,28 | 4,5 |
| Titolo evidenziato dell'indice (`fg`) su panel + `sel` (hover) | — | 12,53 | — | 14,94 | 4,5 |
| Titoli dell'indice (`fg-2`) su panel + `sel` (hover) | — | 8,30 | — | 9,38 | 4,5 |
| `mark` su panel (anello dei risultati di Cerca, segno "cambiato") | 1,19 | 3,89 | 1,22 | 3,52 | 3 |
| `mark` su `sel` (anello di Cerca, lato interno) | 1,27 | 3,34 | 1,22 | 3,27 | 3 |
| Anello del risultato attivo su `sel` (`fg-3` prima, `fg-2` dopo) | 5,73 | 8,30 | 6,42 | 9,38 | 3 |
| Anello "sta scrivendo" sulla barra delle schede (pixel, vedi sotto) | — | 4,62 | — | — | 3 nello scuro |
| Anello "sta scrivendo" sul riempimento `sel` della scheda (pixel) | — | 3,44 | — | — | 3 nello scuro |
| Fuoco della tastiera su vetro | 1,77 | 3,24 | 1,68 | 3,15 | 3 |
| Fuoco della tastiera su panel | 2,13 | 5,36 | 1,97 | 4,72 | 3 |
| Impostazioni: scelta fatta e sua spunta (`fg`) su panel + `sel` | — | 12,53 | — | 14,94 | 4,5 |
| Impostazioni: scelte e bottoni (`fg-2`) su panel + `sel` (hover) | — | 8,30 | — | 9,38 | 4,5 |
| Impostazioni: casella spenta, bordo `fg-3` su panel | — | 7,88 | — | 7,34 | 3 |
| Impostazioni: casella accesa (`fg-2`) e sua spunta (`panel`) | — | 11,19 | — | 10,71 | 3 |
| Etichetta del fuoco spento (`fg-2`) su panel | — | 11,19 | — | 10,71 | 4,5 |
| `line` su panel (linee decorative) | 1,19 | 1,44 | 1,22 | 1,22 | — |
| `line` su vetro (bordo dei campi) | 1,28 | 1,55 | 1,21 | 1,21 | — |
| `sel` su panel | 1,23 | 1,35 | 1,14 | 1,14 | — |
| `sel` accanto al codice in linea | 1,04 | 1,15 | 1,03 | 1,03 | — |

- Il fuoco della tastiera è il colore del sistema (`Cx::focus_color`, AppKit lo dà al 50 %), 3 pt, pieno in
  tutti e due i temi; nel chiaro è anche scurito del 20 % verso il nero (in luce lineare), perché il blu pieno
  sul vetro chiaro arriva solo a 2,6:1. Misurato con il blu predefinito.
- I bordi dei campi restano in `line`: il campo si riconosce dall'icona e dal segnaposto in `fg-2`, non dal bordo.
- L'anello "sta scrivendo" è metallo liquido di Rune (`LiquidMetalParams::chrome`), non un token: non ha un
  colore nostro da regolare. Misurato sui pixel di `docs/proof/contrast-dark-after.png` (scala 2, un fotogramma):
  per ogni punto dei lati dritti attorno alla scheda si prende il pixel più contrastato sui 3 px di spessore; il
  minimo è 4,62:1 sul pannello e 3,44:1 sul `sel` della scheda (mediane 11,37 e 8,46), cioè ≥ 3:1 su tutto il
  perimetro. Lo studio riflesso gira, quindi un altro fotogramma dà valori un po' diversi.
- Il nome doppio (repo o cartella) è in `fg-2` sulla riga scelta di ⌘P: in `fg-3` sul `sel` scendeva a 3,2:1.

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

**Campo "Apri file".** Alto 30 px, raggio 7, riempimento `field`, bordo interno 0,5 px in `line`. Icona,
testo e "⌘P" in `fg-2` (deve leggersi: commento di Matteo). Il tasto "⌘P" è testo `kbd` (11 px, 400) con
bordo 0,5 px in `line`, raggio 4 e margine interno `kbd-pad`; il campo ha margine `field-margin`.

**Apri veloce (⌘P).** Il campo "Apri file" è un campo di testo: ⌘P gli dà il fuoco. Finché la ricerca non è
vuota, la lista dei risultati prende il posto di Recenti e dell'albero. Le righe sono quelle della sidebar (28 px,
raggio 6, hover `sel`, scala 0,98 premuta); la riga scelta con le frecce ha riempimento `sel` e testo `fg`. Un nome
doppio mostra repo o cartella in `fg-3`, come in Recenti, e in `fg-2` sulla riga scelta. Invio apre, Esc svuota e chiude, nessuna animazione di
apertura (azione da tastiera). Nessun accento: caret `fg`, selezione `sel`. Nessun risultato: "Nessun file" in `fg-3`.

**Scheda.** Una riga come quelle della sidebar, larga quanto il nome del file: alta 28 (`row`), raggio 6, testo UI
13 px con 10 ai lati (`row-pad`), hover `sel`, scala 0,98 premuta. La scheda del file attivo ha riempimento `sel` e
testo `fg`; le altre testo `fg-2`. Barra alta 44 (`tab-bar`), margine 10 ai lati (`tab-bar-pad`), 2 fra le schede
(`tab-gap`), linea di 0,5 in `line` sotto; nessun separatore né icona. Troppe schede: la riga scorre di lato. "⌘E
sorgente" a destra come il tasto "⌘P", in `fg-3`.

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
"Aggiungi cartella…" apre il dialogo delle cartelle), "Font" (Inter, SF Pro, New York se installato col suo nome,
poi le famiglie installate; lista che scorre dopo 8 righe), "Dimensione" ("−", "15,5 pt", "+"; titoli, tabelle e
codice scalano col corpo), "Tema" ("Sistema", "Chiaro", "Scuro"), e la casella "Fuoco per il testo nuovo". Le
righe sono quelle della sidebar (28, raggio 6, hover `sel`, scala 0,98). La scelta fatta ha riempimento `sel`,
testo `fg` e una spunta in `fg`; le altre testo `fg-2`. La casella ha il bordo di 1 in `fg-3`; accesa è piena in
`fg-2` con la spunta in `panel`. Con il fuoco spento l'etichetta "N paragrafi nuovi più in basso" resta, in `fg-2`
su una fascia `panel` alta 44 con la linea `line` sopra. Tutto cambia subito, senza riavvio.

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
