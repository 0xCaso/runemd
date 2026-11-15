# SDD ledger — plan: ~/Desktop/GitHub/runemd/docs/plan.md

Spec: docs/spec.md. Repo nuovo, locale, branch main (nessun remote; Matteo ha approvato un commit per task, niente push).
Ruling: lavoro su main nel repo nuovo — non c'è niente da proteggere e non c'è remote — se sbagliato: si crea un branch dopo.

## Setup (controller, prima del Task 1)
- runemd creato (git init). Copiati dallo spike: design/, DESIGN.md, skills-eval.md, docs/app/, docs/decisions/2026-11-13-runemd-*.md (4).
- Spec → docs/spec.md; brief → docs/spec-brief/; piano → docs/plan.md; percorsi aggiornati con sed.
- CLAUDE.md e DECISIONS.md (3 righe RuneMD + riga commit) scritti in runemd.
- Spike: file spostati rimossi; CLAUDE.md/DECISIONS.md ripristinati; CLAUDE.md + riga "RuneMD continues in ~/Desktop/GitHub/runemd" NON committato (non nella lista approvata).

## Pre-flight
| Task | Coppia / interno | Trovato | Ruling |
|---|---|---|---|
| 3↔19 | T3 produce `App::open_paths`, T19 crea `src/app.rs` con `App` | App non esiste al T3 | T3 aggiunge `open_paths` sullo stato esistente in main.rs; T19 lo sposta in app.rs |
| 6↔8 | T6 `view(doc, style: &MarkdownStyle)`, T8 produce lo stile | ok | T6 usa `MarkdownStyle` esistente dello spike fino al T8 |
| 19↔20 | T19 `Mode::Source(SourceEdit)`, SourceEdit nasce al T20 | ordine | T19 ha solo `Mode::Read`; T20 aggiunge `Source` |
| 12 interno | `on_change(.., visible_last: u64 id, ..)` vs `on_scroll(visible_last_index: usize, ..)` | tipi diversi, voluto (id vs indice) | ok, documentare |
| 5↔6↔10↔13 | `Document`, `Change`, `Block.id` | coerenti | — |
| 9↔10/11/14/19/27 | `FsEvent`, `Watcher::drain/add_root` | coerenti | — |
| 2↔11/12/13 | `FxCompositor::set(Mark, Option<FxLayer>)`, `fx::*::layer -> FxLayer` | coerenti | — |
| 4↔9/11/13 | valori DEBOUNCE/WRITING_WINDOW/CHANGED_MARK | coerenti | — |
| 10 interno | sonda `MDSPIKE_LATENCY` (nome dello spike) | nome ereditato | tenere il prefisso usato da probe.rs copiato |
| 2 | gate: soglia fallita → fermarsi e chiedere | stop legittimo del piano | rispettare |
| 4 | sudo fs_usage | request_secret delivery file | rispettare |
| 1,7,8,14-28 | testo interno | coerente | — |

## Tasks
Task 1: BASE root (nessun commit), HEAD 0e0e3b6. Implementer a382e1858b685d390.
Ruling: keychain dedicato runemd.keychain-db invece del login — il login blocca codesign su un prompt GUI; resta un certificato autofirmato locale "RuneMD Local" come deciso da Matteo — se sbagliato: Matteo sposta il cert nel login a mano.
Ruling: cert non trusted (find-identity -v = 0) accettato — codesign firma; la stabilità TCC si verifica al task 17 — se sbagliato: add-trusted-cert a mano da Matteo.
Task 1: minor (deferred): Makefile senza .PHONY; Info.plist senza CFBundleVersion; OFL.txt dentro Resources/Fonts; bench/startup.sh WORKDIR non documentato; env MDSPIKE_*; brief usa codesign -dv (serve -dvv).
Task 1: fix round 1/5 (5 addressed, 0 open; commits 0e0e3b6..0095282)
Task 1: minor (deferred): re-import su keychain esistente non verificato; parsing hash di find-identity fragile (guardato); password keychain "runemd" in chiaro nella riga di comando (cert locale senza valore).
Task 1: complete (commits root..0095282, review clean)
Task 2: BASE 0095282. Implementer abd1daee0b2e450e5. Report BLOCKED: scroll non misurabile (schermo bloccato, frames=0, anche senza compositor). Startup 236/245 ms ok.
Ruling: commit del task 2 con startup misurato e scroll "da misurare" in docs/perf.md; misura scroll rimandata a schermo sbloccato, obbligatoria prima del task 8 — a riposo nessuna pipeline è costruita, quindi il rischio è basso — se sbagliato: si torna sul compositor prima della fase 2.
Ruling: Mark tab parte da 100 (evita collisione con Fire=1/Dither=2) — interfaccia invariata.
Ruling: niente warm-up delle pipeline al task 2 (rune-fx non espone warm per tipo); i task 11-13 decidono — costo: primo frame di un effetto può scattare.
Task 2: minor (deferred): TreeRing senza saturating_add; add_format ogni frame; nessun test su FxCompositor.animating (set Some/None); commento "renderer al primo frame" impreciso; re-export FxLayer con allow.
Task 2: complete (commits 0095282..f04c761, review clean) — PENDING: misura scroll a schermo sbloccato prima del task 8.
Task 3: BASE f04c761. Implementer a0e25936fc3fb11b8. PENDING proof: docs/proof/odoc.png a schermo sbloccato; click in T3 Code da verificare.
Task 3: review (session wer-team) — Important 1: open_paths riusa le chiavi "b{i}" delle righe → altezze del documento vecchio. ⚠️ proof odoc.png, click T3 Code, verifica visiva: in sospeso (schermo bloccato).
Task 3: minor (deferred): poll perde il re-render se c'è un ScrollRun; lock().unwrap() dentro callback extern "C" (abort se poisoned); conversione URL→path a mano invece di NSURL path; shim prefissa $PWD anche ai flag; errori di lettura solo su stderr; nessun test su set_waker / push prima del waker.
Task 3: fix round 1/5 (1 addressed, 0 open; commits d88aa63..a4126b9). Implementer af7b30dbebfc9c253.
Task 3: minor (deferred): test row_keys lascia una cartella temp; altezze di Rune per chiavi vecchie forse mai rimosse (memoria dopo molte aperture: verificare al task 19).
Task 3: complete (commits f04c761..a4126b9, review clean) — PENDING: docs/proof/odoc.png, click in T3 Code, verifica visiva; schermo bloccato.
Task 4: BASE a4126b9.
Ruling: registrazione senza sudo (registratore a polling di stat+contenuto, 2 ms) come via principale; fs_usage solo se `sudo -n` passa — sudo chiede la password e caso non è al Mac; il polling vede inode (rename atomico), dimensione e prefisso (append vs riscrittura) e gli intervalli, cioè tutto quello che serve per i tre valori — se sbagliato: si ripete con fs_usage quando caso dà la password.
Task 4: implementer a3f2e03f30e423993, commit a8a28ce. Valori: DEBOUNCE 50, WRITING_WINDOW 10000 (era 3000), CHANGED_MARK 8000. Codex: modifica in place non misurata (limite d'uso ChatGPT).
Ruling: il ramo fs_usage rotto (kill del PID di grep lascia fs_usage come root) entra nel loop come Important — un processo root che resta acceso non è polish — costo se sbagliato: un giro di fix in più.
Task 4: minor (deferred): prima scansione loggata come create/temp; stat e read separati; nome log -<n> va passato in AGENT; truncate-then-write di Codex non osservato.
Task 4: fix round 1/5 (3 addressed, 0 open; commits a8a28ce..14ef2cd)
Task 4: minor (deferred): agent-writes.md:25 dice "1 create" in codex-2.log, sono 2 righe di snapshot; "circa 700 s" sono 776; snapshot claude-2 non spiegato; intervals.py --only conta i temp di tutti i file.
Task 4: complete (commits a4126b9..14ef2cd, review clean). Waggle c_4e50b426 aperto: WRITING_WINDOW 10 s (in uso) vs 15 s vs 3 s.
Task 5: BASE 14ef2cd. Implementer a1f78545db063f5f6, commit b3fcc05 (TDD senza RED dichiarato).
Task 5: minor (deferred): blocco spostato o edit che cambia il numero di blocchi → id nuovo (ancora del task 6 può perderlo: il task 6 deve tenere un fallback per indice/offset); DefaultHasher non stabile tra versioni (solo in memoria, ok); manca test per Replace di lunghezza diversa e rimozione in mezzo; doppia riga vuota ids.rs:171.
Task 5: complete (commits 14ef2cd..b3fcc05, review clean)
Task 6: BASE b3fcc05.
Ruling: chiave riga = `{generation:x}-{id:x}` invece di `{id:x}` — gli id ripartono da 1 a ogni Document nuovo e ricadrebbero sulle altezze del file precedente (finding del Task 3); generation cresce solo all'apertura, non al reload — se sbagliato: nessuno, è un prefisso.
