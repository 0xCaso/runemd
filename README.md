# RuneMD

Lettore Markdown per Mac, scritto in Rust con [Rune](https://github.com/anomalyco/rune). Avvio sotto i 350 ms, scroll fluido anche su file da 1 MB.

## Uso

    make app                        # compila e crea build/RuneMD.app (firmato "RuneMD Local")
    open build/RuneMD.app --args file.md

Prima volta: `scripts/make_cert.sh` crea il certificato di firma locale "RuneMD Local" in un portachiavi dedicato (`~/Library/Keychains/runemd.keychain-db`): nel login keychain codesign aspetta un dialogo grafico e si blocca.

## Sviluppo

    make check    # clippy, test, gitleaks

Misure: `bench/startup.sh`, `bench/scroll.sh`, `bench/memory.sh` (file di prova in `fixtures/`).
Specifica in `docs/spec.md`, piano in `docs/plan.md`, decisioni in `docs/decisions/`.
