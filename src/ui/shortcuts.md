# Scorciatoie di RuneMD (dal codice, 2026-11-15)

| Tasti | Azione | Dove nel codice |
|---|---|---|
| ⌘P | Apri file (campo della sidebar) | `src/main.rs` `on_key` |
| ⌘N | Nuovo file | `src/main.rs` `on_key` |
| ⌘S | Salva (nel sorgente) | `src/main.rs` `on_key` |
| ⌘W | Chiudi la scheda | `src/main.rs` `tab_key` |
| ⌘1 … ⌘8 | Vai alla scheda n | `src/main.rs` `tab_key` |
| ⌘9 | Ultima scheda | `src/main.rs` `tab_key` |
| ⌃⇥ / ⌃⇧⇥ | Scheda dopo / prima | `src/main.rs` `tab_key` |
| ⌘E | Sorgente e lettura | `src/main.rs` `on_key` |
| ⌘F | Cerca nel file (solo lettura) | `src/main.rs` `on_key` |
| ↵ / ⇧↵ | Risultato dopo / prima (in Cerca) | `src/ui/find.rs` |
| ⌘⌥+ (anche ⌘⌥=) / ⌘⌥− | Larghezza del testo, un passo | `src/ui/tabs.rs` `width_key` |
| ⌘A | Seleziona tutto il documento | `src/ui/doc_view.rs` |
| ⌘, | Impostazioni | `src/main.rs` `on_key` |
| Esc | Chiude popover della larghezza, impostazioni, Cerca, ⌘P | `src/main.rs`, `src/ui/quick_open.rs` |
| ↑ ↓ ↵ (in ⌘P) | Scegli e apri un risultato | `src/ui/quick_open.rs` |
