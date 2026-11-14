# RuneMD

Lettore Markdown per Mac, scritto in Rust con [Rune](https://github.com/anomalyco/rune). Avvio sotto i 350 ms, scroll fluido anche su file da 1 MB.

## Stato: v1

La v1 (piano in `docs/plan.md`, task 1–28) è completa, solo in locale:

- lettura con scroll virtualizzato, schede, sidebar con Recenti e albero del repo del file, Apri veloce (⌘P),
  Cerca nel file (⌘F), indice dei titoli, diagrammi Mermaid;
- segnali live mentre un agente scrive: anello di metallo (sta scrivendo), fuoco (testo nuovo più in basso),
  dither (blocco in arrivo); con "riduci movimento" di macOS restano fermi e visibili;
- sorgente modificabile (⌘E), salvataggio con controllo dei conflitti col disco;
- impostazioni (⌘,): cartelle osservate, font, dimensione, tema, fuoco acceso o spento.

Misure della v1 accanto a quelle dello spike: `docs/perf.md`, sezione "v1". Video del flusso: `docs/proof/`.

## Uso

    make app                        # compila e crea build/RuneMD.app (firmato "RuneMD Local")
    open build/RuneMD.app --args file.md

Prima volta: `scripts/make_cert.sh` crea il certificato di firma locale "RuneMD Local" in un portachiavi dedicato (`~/Library/Keychains/runemd.keychain-db`): nel login keychain codesign aspetta un dialogo grafico e si blocca.

## Aprire i file

- Finder / "Apri con" / `open -a RuneMD file.md`: Launch Services consegna i file con `application:openURLs:` (`src/open/odoc.rs`). Più file insieme arrivano tutti, uno per scheda.
- Terminale: `ln -s "$PWD/scripts/runemd" ~/.local/bin/runemd`, poi `runemd file.md`.
- Per aprire tutti i .md con RuneMD: Finder, "Ottieni informazioni" su un .md, "Apri con" RuneMD, "Cambia tutto...".
- Debug: `RUNEMD_LOG_OPEN=1` stampa su stderr i percorsi ricevuti (`open -a ... --stderr file.log`).
- Clic su un percorso `.md` in T3 Code: da verificare (non provato: non so se passa da Launch Services).

## Sviluppo

    make check    # clippy, test, gitleaks

Misure: `bench/startup.sh`, `bench/scroll.sh`, `bench/memory.sh`, `bench/edit_big.sh` (file di prova in
`fixtures/`). Prove headless: `bench/reduce_motion.sh` (`RUNEMD_REDUCE_MOTION=1|0` vince sulla preferenza di
sistema), `bench/worst_case.sh` (sidebar e albero con 28 repo e nomi lunghi).
Specifica in `docs/spec.md`, piano in `docs/plan.md`, decisioni in `docs/decisions/`.
