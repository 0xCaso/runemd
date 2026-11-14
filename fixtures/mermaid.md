# Diagrammi Mermaid

Un diagramma Mermaid si legge come un'immagine, ma resta testo nel file: il lettore lo disegna con i colori del tema.

## Flusso

```mermaid
flowchart LR
    A[Apri il file] --> B{Il file è cambiato?}
    B -- sì --> C[Aggiorna la vista]
    B -- no --> D[Resta com'è]
    C --> E[Mostra cosa è cambiato]
    D --> E
```

## Sequenza

```mermaid
sequenceDiagram
    participant U as Utente
    participant A as App
    participant D as Disco
    U->>A: Apre un file
    A->>D: Legge il Markdown
    D-->>A: Testo
    A-->>U: Documento in vista
    Note over A,D: Il disco resta la fonte
```

## Non valido

Questo blocco non è un diagramma che il lettore conosce: resta un blocco di codice.

```mermaid
questo non è un diagramma
```

Fine.
