# Audit: formattazione Markdown in lettura

Data: 2026-11-16 (lavoro del 2026-11-15). Record: `docs/decisions/2026-11-15-ordine-lavori-dopo-v1.md`, gruppo 3.
Richiesta di Matteo (scrawl `1794741347588`, 0:16–0:43): evidenziare percorsi e nomi di file, sistemare le sezioni
tipo "Tasks" non formattate, controllare formattazione ed evidenziazione in generale. I titoli che uscivano dalla
colonna sono già sistemati (f41cdf1).

## Come è stato fatto

Build `cargo build --profile perf` a 7ccaaff, foto headless come `bench/long_title.sh`
(`--size 1240x2200 --scale 1 --at 1.0 --snap … --crop main`, `RUNEMD_THEME=light|dark`), su:

| File | Cosa | Foto (chiaro / scuro) |
|---|---|---|
| `docs/audit/2026-11-16-formattazione-markdown/prova.md` | ogni elemento della lista | `formattazione-light.png` / `-dark.png` |
| `.superpowers/sdd/plan/progress.md`, righe 1–40 | il registro SDD della registrazione | `progress-*.png` |
| `docs/plan.md`, righe 50–80 | un `### Task N:` con `- [ ] **Step**` | `plan-task-*.png` |
| `README.md` | README | `README-*.png` |
| `docs/decisions/2026-11-15-ordine-lavori-dopo-v1.md` | un record di decisione | `decisione-*.png` |

Foto in `docs/audit/2026-11-16-formattazione-markdown/`. Il file di prova copre: liste di compiti, liste annidate,
codice in linea con percorsi (`src/ui/tabs.rs:42`), percorsi nudi nella prosa, tabelle con colonna a destra,
citazioni, callout `> [!NOTE]` e `> [!WARNING]`, titoli 1–6, blocchi di codice `rust` e `sh`, riga orizzontale,
nota a piè di pagina, barrato, corsivo, URL lungo, righe con un solo a capo.

Il Markdown lo disegna `rune_kit::Markdown` (rev 384c299). Il parser del kit accende solo tabelle, barrato e liste
di compiti (`crates/rune-kit/src/markdown/parser.rs:466`) e porta i titoli dal 4 in giù tutti al livello 4
(`parser.rs:503`). Lo stile è in `src/ui/markdown_style.rs`.

## Cosa esce bene

Titoli 1–2, corpo, grassetto, corsivo, barrato, liste annidate (puntati e numeri), tabelle (colonna `---:` a
destra, cifre tabellari), blocchi di codice con i colori della sintassi, riga orizzontale, citazione semplice,
percorsi lunghi che vanno a capo (f41cdf1).

## Cosa esce male (correzioni senza scelta)

Mockup "fix" in `design/mockups/2026-11-16-formattazione-markdown/` (`shots/fix-light.png`, `shots/fix-dark.png`).

| # | Elemento | Oggi | Dopo | Foto | Dove si corregge |
|---|---|---|---|---|---|
| F1 | URL lungo nudo | esce dalla colonna e finisce sotto l'indice "In questo file" | va a capo dentro la colonna, il link resta un link intero | `formattazione-*.png`, "Link lungo" | `doc::breaks::protected` lascia intatti gli URL per non rompere il link automatico (decisione del 2026-11-15); serve un a capo che non cambi il testo del link (spezzare solo il testo disegnato, o `wrap_anywhere` sul solo tratto del link) |
| F2 | Callout `> [!NOTE]`, `[!TIP]`, `[!IMPORTANT]`, `[!WARNING]`, `[!CAUTION]` | citazione con "[!NOTE]" scritto | citazione con etichetta in parole e icona (Nota, Suggerimento, Importante, Attenzione, Cautela), 13/600 in `fg`, icona `fg-2`, nessun colore | `formattazione-*.png` | il kit non li riconosce: in RuneMD prima di disegnare, come `doc::breaks` (testo mostrato), o un `quote_frame` del kit |
| F3 | Note a piè di pagina `[^1]` | "[^1]" e "[^1]: …" scritti come testo | numero in alto nel testo, note in fondo al file sotto una linea, 13,5 in `fg-2` | `formattazione-*.png` | il kit non accende `ENABLE_FOOTNOTES`: in RuneMD sul testo mostrato |
| F4 | Titoli 4, 5, 6 | uguali al 3 (15,5/640); il 3 pesa come una riga in grassetto (`**Files:**` nei piani) | 3 a 17/640 `fg`, 4 a 15,5/600 `fg`, 5 a 14/600 `fg-2`, 6 a 13/500 `fg-3` | `formattazione-*.png`, `plan-task-*.png` | il kit porta 4–6 a un solo livello (`parser.rs:503`): per 5 e 6 serve il livello vero; il 3 è `markdown_style::fonts` |
| F5 | Casella delle liste di compiti | bordo `border_weak` del kit: nello scuro quasi invisibile, sotto 3:1 | bordo 1 in `fg-3`, accesa piena `fg-2` con spunta `panel`, come la casella delle Impostazioni (7,88 / 7,34 e 11,19 / 10,71, tabella "Contrasto" di DESIGN.md) | `plan-task-dark.png`, `formattazione-*.png` | `MarkdownStyle::task_checkbox` in `markdown_style::style_with` |

DESIGN.md va aggiornato con F2–F5 (callout, note, scala dei titoli 3–6, casella) quando si fanno.

## Percorsi e nomi di file (scelta visiva a)

Oggi un percorso è codice solo se l'agente l'ha messo fra backtick; nudo resta testo normale, e dentro la
pastiglia il nome del file pesa come le cartelle (`formattazione-*.png`, prima riga; `progress-*.png`, "Setup").
Varianti nel mockup: `p-oggi`, `p-codice`, `p-nome`, `p-link` (stretch).

Cosa conta come percorso (per tutte le varianti): una parola con almeno un `/` e un'estensione o una `/` finale
(`src/main.rs`, `design/mockups/`, `~/Desktop/x.md`), o un nome di file noto senza cartella con estensione comune
(`Cargo.toml`, `README.md`), con `:riga` facoltativo. Non conta dentro blocchi di codice, link e URL.

## Sezioni "Tasks" (scelta visiva b)

Nei file di Matteo ci sono due forme:

1. **Registro SDD** (`.superpowers/sdd/plan/progress.md`, quello della registrazione, foto `frame-06`): sotto
   `## Tasks` ogni riga è una voce (`Task 1: BASE…`, `Ruling: …`, `Task 1: minor (deferred): …`) separata da
   **un solo a capo**. Per CommonMark un a capo singolo è uno spazio: tutte le righe diventano un paragrafo solo,
   il muro di testo cerchiato da Matteo. Causa: il kit tratta il soft break come spazio (comportamento standard).
2. **Piani superpowers** (`docs/plan.md`): `### Task N: titolo`, poi `**Files:**`, liste, `- [ ] **Step N:** …`.
   Qui la struttura c'è, ma il titolo 3 è 15,5/640 come `**Files:**` (F4) e le caselle quasi non si vedono (F5):
   i task non si separano a colpo d'occhio.

Varianti nel mockup: `t-oggi`, `t-righe`, `t-sezioni`, `t-registro` (stretch). Tutte le varianti `t-` includono
già F4 e F5.

## Mockup

`design/mockups/2026-11-16-formattazione-markdown/index.html`, foto chiaro e scuro in `shots/`.
