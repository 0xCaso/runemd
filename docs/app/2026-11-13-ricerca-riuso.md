# Cosa possiamo riusare per l'app Markdown

13 novembre 2026. Ho letto i repo pubblici di github.com/anomalyco e i sorgenti di Rune in
`~/Desktop/GitHub/_vendor/rune` (rev `384c2991`).

## Cosa ho capito che vuoi

- Un'app Mac in Rune per **leggere e modificare** file Markdown. Deve essere la più bella e la più veloce
  possibile.
- Vuoi farla diventare **l'app di default per i file `.md`**.
- Lo stile è quello di "Current": cornice e sidebar traslucide, contenuto nero in tema scuro e bianco
  in tema chiaro.
- Funzioni ancora aperte: file recenti, albero delle cartelle o dei repo, scelta del font, altro.

## OpenCode

OpenCode è scritto in TypeScript (la TUI usa opentui), quindi il suo codice Markdown non si può usare
in un'app Rust. **Però il pezzo migliore è già in Rune:** `rune-highlights` è il colorista del codice
dell'app desktop di OpenCode (`@pierre/highlights`, 80 linguaggi), riscritto in Rust nativo. Avremo
gli stessi colori di OpenCode nei blocchi di codice.

## Cosa c'è già in Rune (non va scritto da zero)

| Bisogno | Pezzo di Rune | Note |
|---|---|---|
| Rendering Markdown | `rune_kit::Markdown` | Tabelle GFM, task list cliccabili (`on_task`), link, immagini, diff. Cache del parsing. Già usato nello spike. |
| Codice colorato | `rune-highlights` | Lo stesso di OpenCode desktop. |
| Diagrammi | `rune-mermaid` | Mermaid nativo (flowchart, sequence, gantt…), senza web view. |
| Cerca nel documento | `rune_kit::search` + `markdown::find` | Trova come Safari: maiuscole, accenti, Unicode. |
| Albero di file | `rune_kit::Tree` + `FileTree` | Virtualizzato (50.000 nodi al costo di 40), tastiera completa. |
| Struttura finestra | `rune_kit::shell` | `SplitView` con sidebar ridimensionabile, `Toolbar`, `TabStrip` (schede), `WindowTitlebar`. |
| Vetro / traslucenza | `El::system_material` | Liquid Glass (`NSGlassEffectView`) su macOS 26+, vibrancy prima. Esempi: `frosted-bar`, `glass-panel`. |
| Diff tra versioni | `rune_kit::diff` | Utile per "cosa è cambiato da quando l'ho aperto". |
| Helper Mac | `rune-mac` | Ricorda dimensione e posizione della finestra, schermo intero. |
| Esempio di app vera | `anomalyco/clip` | App Rune per macOS pubblicata: è il modello per bundle e icona. |

## Cosa manca e va scritto

1. **Aprire i `.md` dal Finder (app di default).** Rune riceve solo link con uno schema proprio
   (`myapp://`, Apple Event `GURL`). Non riceve "apri questo documento" (Apple Event `odoc`), che
   macOS usa per doppio clic e "Apri con". Va aggiunto un gestore `odoc` nella nostra app (stesso
   metodo di `crates/rune/src/open_url/incoming.rs`), più `CFBundleDocumentTypes` per `.md` in
   Info.plist. Stima: mezza giornata di agente (stima, nessuna classe misurata).
2. **Selezione e copia oltre lo schermo** (limite trovato nello spike): Cmd+A e copia di tratti
   lunghi.
3. **Editor.** Oggi `text_input` mostra il sorgente grezzo di un blocco. Un vero editor (sorgente con
   colori a sinistra e anteprima a destra, oppure modifica dentro la vista) è la parte più grande e
   più da decidere.
4. Stile tipografico proprio al posto di `MarkdownStyle::neutral`.

## Rischio

Rune va fissato a una rev del branch `dev` con quattro patch `[patch.crates-io]` (vedi lo spike). Ogni
aggiornamento di Rune può rompere qualcosa.
