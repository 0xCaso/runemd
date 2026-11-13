# RuneMD

Lettore Markdown per Mac, scritto in Rust con [Rune](https://github.com/anomalyco/rune). Avvio sotto i 350 ms, scroll fluido anche su file da 1 MB.

## Uso

    make app                        # compila e crea build/RuneMD.app (firmato "RuneMD Local")
    open build/RuneMD.app --args file.md

Prima volta: `scripts/make_cert.sh` crea il certificato di firma locale "RuneMD Local" in un portachiavi dedicato (`~/Library/Keychains/runemd.keychain-db`): nel login keychain codesign aspetta un dialogo grafico e si blocca.

## Aprire i file

- Finder / "Apri con" / `open -a RuneMD file.md`: Launch Services consegna i file con `application:openURLs:` (`src/open/odoc.rs`). Più file insieme arrivano tutti; per ora si vede l'ultimo (le schede arrivano col task 19).
- Terminale: `ln -s "$PWD/scripts/runemd" ~/.local/bin/runemd`, poi `runemd file.md`.
- Per aprire tutti i .md con RuneMD: Finder, "Ottieni informazioni" su un .md, "Apri con" RuneMD, "Cambia tutto...".
- Debug: `RUNEMD_LOG_OPEN=1` stampa su stderr i percorsi ricevuti (`open -a ... --stderr file.log`).
- Clic su un percorso `.md` in T3 Code: da verificare (non provato: non so se passa da Launch Services).

## Sviluppo

    make check    # clippy, test, gitleaks

Misure: `bench/startup.sh`, `bench/scroll.sh`, `bench/memory.sh` (file di prova in `fixtures/`).
Specifica in `docs/spec.md`, piano in `docs/plan.md`, decisioni in `docs/decisions/`.
