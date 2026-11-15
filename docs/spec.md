# RuneMD v1: design

13 novembre 2026. Domande, approccio e design decisi con Matteo nel caso Waggle
c_ed9ba7bf-6c2c-489c-ab4f-65258cabb203. Revisione EGM (comprensione, zoom-out, design) applicata; le scelte mie
sono marcate **[mia scelta]**.

Documenti citati (tutti in `~/Desktop/GitHub/rune-md-spike`, da copiare nel repo nuovo in `docs/`):
- `DESIGN.md`: aspetto, token, segnali live;
- `docs/decisions/2026-11-13-runemd-main-screen.md`: record della schermata scelta;
- `design/mockups/2026-11-13-runemd-main/index.html`: mockup, variante `scheda` (giri precedenti in `index-r*.html`);
- `docs/spike/2026-11-13-rune-vs-md-light.md`: misure dello spike; `docs/app/2026-11-13-ricerca-riuso.md`: cosa
  c'è in Rune.

Sorgenti di Rune: `~/Desktop/GitHub/_vendor/rune`, rev `384c2991`.

## Intento

Matteo legge ogni giorno i Markdown che gli agenti scrivono nei repo (spec, piani, report, CLAUDE.md, memorie).
Le sue parole: "leggi soprattutto file scritti da agenti nei repo --> questo. mi capita davvero poco spesso di
scrivere, qualche volta modifico."

RuneMD è un'app Mac in Rune per **leggere** questi file mentre gli agenti li scrivono, e **modificarli** ogni tanto.
Diventa l'app di default per i `.md`. Deve essere la più veloce e la più bella possibile. La velocità è anche il
soggetto di un post (Rune contro Swift, `docs/spike/post/card.html`: primo frame 13× più veloce su 1 MB).

## Obiettivi

1. Doppio clic su un `.md` nel Finder, "Apri con" (anche più file insieme) o `open FILE.md` dal terminale aprono il
   file in RuneMD, sia ad app chiusa sia ad app aperta.
2. Il file aperto si aggiorna da solo quando un agente lo riscrive, e quello che stai leggendo non si sposta.
3. Si vede quale file un agente sta scrivendo e se c'è testo nuovo sotto la parte visibile (segnali live).
4. La sidebar mostra i Markdown recenti di tutti i repo e l'albero del repo del file aperto.
5. Cmd+E passa dalla lettura al sorgente di tutto il file, con i colori; si salva e si torna alla lettura.
6. Funzioni scelte da Matteo per la v1: cerca nel file (Cmd+F), apri veloce (Cmd+P), indice dei titoli, scelta del
   font, schede, cosa è cambiato, diagrammi Mermaid, seleziona e copia tutto, nuovo file (Cmd+N; aggiunto nella
   review della spec).
7. Tema chiaro e scuro, aspetto come `DESIGN.md`.

## Non obiettivi

- Editor ricco, anteprima affiancata, modifica dentro la vista (Matteo ha scelto il tasto unico Cmd+E).
- Sync, cloud, plugin, esportazione PDF, Windows e Linux.
- Distribuzione: notarizzazione, App Store, sito. La v1 è un `.app` locale.

## Anti-obiettivi (fallisce anche se funziona)

- Il testo salta mentre un agente scrive.
- L'aspetto diventa "AI slop": pillole di stato, bagliori fluo, statistiche decorative, accenti a caso (commenti di
  Matteo nel record della schermata).
- È più lenta di md-light sulle misure dello spike.

## Vincoli

- Stack: Rune + rune-kit + rune-fx alla rev `384c2991` del branch `dev`, con le quattro `[patch.crates-io]` dello
  spike (`Cargo.toml`). Ogni aggiornamento di Rune può rompere qualcosa: si aggiorna solo con un motivo scritto.
- Repo nuovo `~/Desktop/GitHub/runemd`, solo locale, senza push (proposto nel giro "approcci", nessuna obiezione).
  Dallo spike si portano `src/blocks.rs`, la base di `src/main.rs`, `fixtures/`, `bench/` e i documenti citati sopra.
- macOS 26+ (Liquid Glass via `El::system_material`, finestra trasparente, tinta `glass` al 60 % sopra).
- Inter va nel bundle (macOS non lo ha); `AppSpec.families = ("Inter", "ui-monospace, Menlo")`.

## Approcci

- **Rune con rune-kit (scelto).** Lo spike ha mostrato il primo frame 13× più veloce di AppKit su 1 MB e lo scroll
  pari. Quasi tutto esiste: `Markdown` (con `rune-highlights` e la feature `mermaid`), `FileTree`, `SplitView`,
  `TabStrip`, `markdown::find`, `combobox` (fuzzy), `rune::prefs`, gli effetti di `rune-fx`.
- Swift/AppKit: selezione lunga e `odoc` gratis, ma perde la velocità che è il motivo dell'app e del post.
- Web (Tauri o Electron): più lento e più pesante di entrambi.

Per la modifica Matteo ha scelto **"Un tasto: lettura o sorgente"**, non "Sorgente e anteprima affiancate" né
"Modifica dentro la vista".

## Cosa ha deciso Matteo

### Schermata principale ("Scheda viva")

Dettagli in `DESIGN.md` e nel record. Sidebar di vetro con "Apri file ⌘P", "Recenti" e "Repo del file" (riga del
repo con nome e branch, file indentati con linee guida). Pannello pieno a 8 px dai bordi, vetro su tutti e quattro
i lati. Schede in alto, "⌘E sorgente" a destra con alla sua sinistra il pulsante ↔ della larghezza del testo (popover col cursore, ⌘⌥+ e ⌘⌥−), colonna di 680 px, indice "In questo file" a destra.

### Segnali live, uno per significato

| Segnale | Significato |
|---|---|
| Anello di metallo liquido su 4 lati attorno a scheda, riga nei Recenti, riga nell'albero | Un agente sta scrivendo questo file |
| Fuoco dal fondo del pannello, sempre acceso e basso; alto con "N paragrafi nuovi più in basso" | Alto: c'è testo nuovo sotto la parte visibile |
| Dither che pulsa al posto di un blocco | Il blocco sta ancora arrivando |

### Da dove arrivano i file

"Tutti e due": il link apre il file; la sidebar mostra i recenti di tutti i repo e l'albero del repo del file
aperto. Le cartelle osservate sono `~/Desktop/GitHub` (28 repo oggi), più altre aggiunte nelle impostazioni.

### Nome

"RuneMD" ("dice subito cosa è e con cosa è fatta. Utile per il post").

## Comportamento

### Aprire un file

- `Info.plist`: `CFBundleDocumentTypes` per `net.daringfireball.markdown` (`.md`, `.markdown`), ruolo Editor.
  `make_app.sh` registra il bundle con `lsregister`. RuneMD diventa default una volta, a mano, da "Ottieni
  informazioni" nel Finder; il README lo spiega.
- **`odoc` [mia scelta, dopo la revisione].** Il gestore GURL di Rune (`crates/rune/src/open_url/incoming.rs`) è
  `pub(crate)` e si installa prima che parta NSApplication; per `odoc` AppKit installerebbe il suo sopra.
  RuneMD, in `main` prima di `App::run`, osserva `NSApplicationWillFinishLaunchingNotification` e lì aggiunge
  `application:openURLs:` alla classe del delegate, come fa `crates/rune/src/platform/reopen.rs:91-114`. I percorsi
  vanno in una coda statica finché la finestra non c'è, poi arrivano all'app con `AppHandle::update`. Se dopo
  un giorno non funziona: patch piccola a Rune con `App::on_open_file` accanto a `on_open_url` (`app.rs:496`).
- Un file già aperto in una scheda viene portato davanti, non duplicato.
- Da terminale: `open -a RuneMD FILE.md`, oppure `runemd FILE.md` (link nel PATH al binario del bundle) che usa
  `rune::single_instance` (`open_url/instance.rs`) per passare il file all'istanza già aperta.
- Un percorso cliccato in T3 Code: da verificare che passi da Launch Services; se no, si documenta il limite.

### Lettura stabile

- Il file si divide in blocchi top-level (`blocks.rs` dello spike) e si mostra in `virtual_rows`.
- **Id stabili dei blocchi [mia scelta].** Ogni blocco ha un id persistente, non l'indice (lo spike usa
  `format!("b{i}")`, `src/main.rs:101`). A ogni versione nuova un LCS sugli hash dei blocchi (crate `similar` o
  codice proprio; `rune_kit::diff` è una vista di patch, non serve qui) assegna gli id: un blocco uguale tiene il
  suo id; un blocco cambiato sul posto tiene l'id del vecchio; i blocchi uguali ripetuti (`---`) si distinguono con
  un contatore. `virtual_rows_version` segue la revisione del file.
- **Ancora di lettura.** `virtual_rows` tiene ferma la riga in cima per chiave
  (`crates/rune/src/el/virtual_rows.rs:50-53`). Se il blocco in cima cambia altezza, l'offset si corregge dentro il
  blocco; se sparisce, l'ancora passa al blocco seguente che esiste ancora.
- **Parole lunghe.** Percorsi e nomi lunghi in titoli, paragrafi, liste e celle vanno a capo dopo `/ - _ .`
  (fra due lettere o cifre); ovunque solo se il tratto senza questi segni è più largo di una riga. Codice a
  blocchi, HTML, URL nudi e destinazioni dei link restano come sono. Copia, indice e Cerca leggono il sorgente
  vero (`doc::breaks`, fixture `fixtures/long-title.md`).
- Test di accettazione: con uno script che inserisce testo sopra la vista, che allunga il blocco in cima e che
  allunga l'ultimo blocco, il pixel in cima alla vista non si sposta.

### Osservare i file

- **Un solo servizio di eventi [mia scelta].** Un unico stream FSEvents ricorsivo (crate `notify` 8.2, come
  `crates/rune-luau/src/watch.rs`, con `Waker` per svegliare la UI) sulle cartelle osservate. Da quello vengono
  la ricarica del file aperto, lo stato "sta scrivendo" per scheda, Recenti e albero, e l'indice di Recenti e Cmd+P.
  Uno stream in più solo per le radici fuori dalle cartelle osservate (file aperti altrove e il loro repo).
- Si osserva la cartella, non il file: un agente che scrive su un file temporaneo e poi lo rinomina non fa
  perdere il file.
- Filtro: solo `.md` e `.markdown`, saltando i percorsi con `.git`, `node_modules`, `target`. **Niente filtro
  `.gitignore`** [mia scelta]: spesso i file degli agenti (`.claude/`, piani locali) sono ignorati da git, e sono
  proprio quelli da vedere.
- Rilettura con debounce (ipotesi: 50 ms). Una lettura vuota o molto più corta della precedente mentre il file
  cambia si scarta e si rilegge dopo il debounce.
- **Scritture che non sono di un agente.** RuneMD ricorda l'hash di quello che salva e ignora l'evento uguale. Un
  cambio di `.git/HEAD`, `.git/ORIG_HEAD` o `.git/FETCH_HEAD` nello stesso repo negli ultimi 2 s non accende
  l'anello [mia scelta]: checkout scrive `HEAD`, pull e `reset --hard` spostano solo i ref ma scrivono `ORIG_HEAD`
  e `FETCH_HEAD`; in un worktree si guarda l'`HEAD` in `.git/worktrees/<nome>/` (`watch::git_head_root`).
- Primo task del piano: registrare con `fs_usage` come scrivono davvero Claude Code e Codex una spec lunga
  (riscrittura intera, rename, append) e tarare debounce e soglie su quei dati.

### Segnali live: quando si accendono

- **Anello ("sta scrivendo")**: il file è cambiato da una scrittura esterna negli ultimi N secondi (ipotesi: 3 s).
- **Fuoco** (v1.1: sempre acceso in lettura, basso, a riposo): sale quando ci sono blocchi nuovi o cambiati
  sotto la parte visibile, e torna a riposo quando l'ultimo di questi entra nella parte visibile. L'impostazione
  "Fuoco in fondo al pannello" lo spegne del tutto (resta solo l'etichetta col testo nuovo). Sotto, intensità, altezza, velocità e colore
  (Fuoco, Blu, Viola, Verde, Accento di sistema) si salvano in prefs.json; un valore non valido torna al default, e i
  default sono il fuoco di prima. La velocità non cambia i risvegli a riposo. A riposo si
  ridisegna 12 volte al secondo, senza display link, per non tenere la CPU alta. L'etichetta conta questi blocchi e resta "N paragrafi nuovi più in basso" (testo approvato).
  Con la finestra non attiva l'animazione si ferma e il fuoco resta fermo e visibile [mia scelta].
- **Dither ("sta arrivando")**: solo per l'ultimo blocco del file, quando è aperto (fence ``` non chiusa, oppure
  il file non finisce con una riga vuota dopo il blocco) e il file è cambiato negli ultimi N secondi. Altrimenti si
  mostra il testo. La barra ha l'altezza di una riga e sta in fondo al documento, quindi non sposta niente sopra.
  Conseguenza: se gli agenti riscrivono il file intero in un colpo, il dither si vede di rado (vedi "Domande per
  Matteo").
- **Cosa è cambiato**: i blocchi nuovi o cambiati hanno un segno grigio sottile a sinistra per qualche secondo
  (ipotesi: 8 s).
- Con "riduci movimento" l'anello e il dither restano fermi e visibili, il fuoco è fermo e cambia altezza senza salire.

### Effetti GPU (rune-fx) [mia scelta, dopo la revisione]

rune-fx non è un metodo di `El`: serve un `App::compositor` unico (`crates/rune/src/compose.rs`, `app.rs:554`) che
disegna ogni frame con `FxRenderer`, più `El::effect(mark)` sugli elementi.

- Un mark per ogni posto che può avere un effetto (scheda, riga Recenti, riga albero, fondo del pannello, blocco in
  arrivo). `animating()` è falso senza agenti attivi; il fuoco a riposo (v1.1) si ridisegna coi risvegli a 12 fps.
- L'anello è `LiquidMetal` come bordo: `.border(w)` dipinge dentro la forma (`rune-fx/README.md:108`), quindi il
  mark è più grande della riga di 1,5 px per lato, o si usa `effect_bleed`.
- Fuoco nel tema chiaro: stessa copertura del tema scuro, il testo resta sotto le fiamme (decisione `docs/decisions/2026-11-14-light-theme-fire.md`).
- Pipeline compilate in anticipo all'avvio fuori dal primo frame (20–150 ms l'una a cache fredda, README:170).
- **Task 1 del piano**: lo spike con il compositor attaccato e nessun effetto attivo deve restare nelle soglie di
  avvio e scroll. Se no, si ferma e si decide prima di costruire il resto.

### Sidebar

- **Avvio**: il primo frame mostra solo il documento. Una scansione in background (mtime dei `.md` nelle cartelle
  osservate) riempie Recenti e l'indice di Cmd+P; l'indice si salva in Application Support e si aggiorna dagli
  eventi.
- **Recenti**: i `.md` cambiati di recente, più nuovi in alto, tempo relativo, al massimo 20 righe (ipotesi).
- **Repo del file**: radice = la cartella con `.git` più vicina sopra il file (fuori da un repo: la cartella del
  file); nome e branch da `.git/HEAD`. `FileTree` con `compact_folders(false)` e cartelle aperte fino al file
  aperto; i percorsi vengono dall'indice (solo cartelle con `.md`).
- **Permessi**: `~/Desktop` è protetta (TCC). Il `.app` si firma sempre con lo stesso certificato locale, così il
  permesso resta dopo ogni build. Se l'accesso è negato, la sidebar lo dice ("Nessun accesso a Desktop") con il
  link "Apri Impostazioni di Sistema" alle impostazioni di sistema.

### Schede

- `TabStrip` di rune-kit, sempre `status(None)` (i suoi pallini e spinner sono vietati da `DESIGN.md`); l'anello va
  con `TabItem::decorate`.
- Cmd+W chiude, e così la × sulla scheda davanti o in hover (v1.1) e il clic centrale; Cmd+1…9 e Ctrl+Tab cambiano scheda [mia scelta, convenzione Mac].
- Scheda di anteprima (dopo la v1, come VS Code): un clic singolo su un file nella sidebar (Recenti, albero,
  risultati di ⌘P, anche Invio in ⌘P) lo apre in una sola scheda di anteprima, nome in corsivo, che il clic singolo
  dopo sostituisce nello stesso posto. Diventa fissa con un doppio clic sulla riga o sulla scheda, una modifica nel
  sorgente o ⌘S. Da Finder, terminale, ⌘N e copie di recupero il file si apre fisso. Un file già aperto: si porta
  davanti la sua scheda.

### Modifica (Cmd+E)

- Cmd+E mostra il sorgente di tutto il file in un `text_input` multilinea con i colori Markdown (rune-highlights).
  Cmd+S salva; Cmd+E di nuovo salva (solo se ci sono modifiche) e torna alla lettura.
- All'ingresso RuneMD ricorda l'hash del file. Prima di ogni salvataggio lo confronta col disco: se è cambiato,
  non salva e mostra una riga "Il file è cambiato su disco" con "Ricarica" e "Tieni il mio". Mai sovrascrivere in
  silenzio. Durante la modifica la vista non si ricarica da sola.
- **Soglia**: digitando su `fixtures/big.md`, CPU per frame p95 ≤ 8 ms (ipotesi). `text_input` fa un solo layout
  per tutto il testo e lo clona a ogni vista (`crates/rune/src/el/input.rs:57-63`); lo spike ha misurato solo un
  paragrafo da 6 KB.
- **Piano B se la soglia non passa** [mia scelta]: sorgente virtualizzato, un `text_input` per blocco in
  `virtual_rows`, colori solo sulle righe montate, blocchi riuniti al salvataggio. Resta un'unica vista sorgente.

### Altre funzioni v1

- **Cerca (Cmd+F)**: `markdown::find`, Invio e Maiusc+Invio tra i risultati.
- **Apri veloce (Cmd+P)**: fuzzy di `rune-kit/src/combobox.rs` sui nomi dell'indice.
- **Frontmatter YAML** (v1.1): al byte 0, chiuso da `---` o `...`, è un blocco di metadati quieto in lettura,
  fuori dall'indice; il sorgente non cambia (DESIGN.md, "Frontmatter").
- **Indice dei titoli**: H1–H3 a destra; evidenziato il titolo della sezione visibile; un clic scorre.
- **Font**: famiglia e dimensione del corpo. Prima Inter, New York, SF Pro; poi l'elenco dei font installati via
  CoreText (Rune non lo espone).
- **Mermaid**: feature `mermaid` di rune-kit.
- **Seleziona e copia tutto**: alla rev fissata Rune ha già la selezione sulle righe virtuali non costruite e
  Cmd+A/Cmd+C con ricostruzione delle righe (`crates/rune/src/runtime/virtual/selection_collect.rs`, tetto 10.000
  righe e 4 MiB). Primo passo: uno script `--script` con Cmd+A e Cmd+C su `big.md` che confronta gli appunti con
  il file. Se il testo copiato non è il Markdown sorgente, `selection_copy` sul gruppo mappa i blocchi selezionati
  agli id stabili e copia il loro sorgente. Sopra i 4 MiB Cmd+A copia tutto il file dal disco [mia scelta].

- **Nuovo file (Cmd+N)** [mia scelta sul come]: un campo come quello di Cmd+P chiede il nome; il file nasce
  vuoto nella cartella del file aperto (senza file aperto: la prima cartella osservata), si apre in una scheda nuova
  già in sorgente (Cmd+E). `.md` si aggiunge se manca; un nome già esistente apre quel file, non lo sovrascrive.

### Impostazioni

Cartelle osservate (aggiungi, togli), font, dimensione, tema (sistema, chiaro, scuro), salvate con `rune::prefs`.
Si aprono con Cmd+, in un pannello nella finestra principale [mia scelta: evita una seconda finestra].

## Prestazioni (soglie della v1)

Stessa macchina e stessi file dello spike (`fixtures/normal.md`, `fixtures/big.md`), con compositor, sidebar e
watcher attivi:

- avvio warm al primo frame con contenuto ≤ 350 ms su normal e su big (spike: 302 e 295 ms);
- sidebar riempita ≤ 1 s dopo il primo frame con l'indice salvato (ipotesi);
- scroll: refresh persi ≤ 3 % (spike: 2,4 e 2,6 %);
- memoria a riposo su big con l'indice caricato ≤ 100 MB (spike: 78 MB);
- dalla scrittura dell'agente al frame nuovo ≤ 150 ms, latenza FSEvents e debounce compresi (ipotesi);
- modifica: p95 ≤ 8 ms per frame su big (ipotesi).

Si rimisurano alla fine di ogni fase.

## Ordine di lavoro [mia scelta]

1. **Fondamenta e rischi**: repo, bundle firmato, compositor rune-fx misurato, `odoc`, id stabili e ancora,
   registrazione di come scrivono gli agenti, prova di Cmd+A/Cmd+C.
2. **Lettura live**: watcher unico, ricarica, i tre segnali, cosa è cambiato.
3. **Sidebar**: indice, Recenti, albero, Cmd+P, permessi.
4. **Schede e modifica**: `TabStrip`, Cmd+E, conflitti, misura e piano B, nuovo file.
5. **Il resto**: cerca, indice dei titoli, font, Mermaid, impostazioni, tema chiaro, controllo finale delle soglie.

## Risposte di Matteo (review della spec, 13/11)

1. **Dither raro: tenerlo così.** Solo sull'ultimo blocco aperto. Matteo: "non so quanto spesso succederà, magari
   se succede ci sta che venga mostrato come l'abbiamo messo".
2. **Firma: sì, certificato autofirmato locale** nel Portachiavi, solo per RuneMD.
3. **"Nuovo file" entra nella v1** (Matteo: "della parte che hai messo fuori metterei "nuovo file" in "dentro"").

## Cosa resta al builder

- Moduli, tipi, divisione dello stato.
- I valori di debounce, "sta scrivendo", "cosa è cambiato" e dei parametri rune-fx, partendo dalle ipotesi sopra e
  dalla registrazione del task 1.
- Bundle e icona (modello: `bench/make_app.sh` dello spike e l'app Rune `anomalyco/clip` su GitHub).
- Le skill di design da usare: `skills-eval.md`.
