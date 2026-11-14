//! La vista del sorgente (Cmd+E): tutto il file in un `text_input` multilinea, nella colonna del
//! documento, con i colori Markdown di rune-highlights (via `rune_kit::syntax`) sui grigi del
//! tema: nessun accento (DESIGN.md).
//!
//! Invio va a capo (un `text_input` multilinea lo userebbe per inviare). Cmd+S e Cmd+E stanno in
//! `main::on_key`: il campo non li prende.

use rune::{El, Family, ScrollSpec, Span, TextStyle, Tx, col, el, text_input};
use rune_kit::syntax::{self, Syntax, SyntaxTheme};

use super::doc_view::{BOTTOM, TOP};
use super::theme::{CODE_TEXT, COLUMN, ROW_PAD_X, Theme, UI_STRONG_WEIGHT};
use crate::app::{App, Tab};
use crate::edit::SourceEdit;

/// Il testo del sorgente: `typography.doc-code` (mono 13,5).
pub fn font() -> TextStyle {
    TextStyle::new(CODE_TEXT, 400.0).family(Family::Mono)
}

/// I colori dei token Markdown, tutti dai grigi del tema: titoli in `fg` e più pesanti, link in
/// `fg`, codice in `fg2`, segni (`#`, `-`, `>`, fence, parentesi) e destinazioni dei link in `fg3`;
/// il resto nel colore del corpo.
pub fn syntax_theme(theme: &Theme) -> SyntaxTheme {
    use Syntax::*;
    let mut t = SyntaxTheme::neutral(theme.dark);
    for kind in [
        Keyword,
        String,
        Number,
        Comment,
        Type,
        Function,
        Property,
        Punctuation,
        Inserted,
        Deleted,
        Parameter,
        Heading,
        Link,
        Operator,
        Regex,
        Attribute,
        Variable,
    ] {
        let c = match kind {
            Heading | Link => theme.fg,
            String | Number | Type | Function | Property | Regex | Attribute | Variable | Parameter => theme.fg2,
            Punctuation | Keyword | Comment | Operator => theme.fg3,
            _ => theme.body,
        };
        t = t.with(kind, c);
    }
    t.with_weight(Heading, UI_STRONG_WEIGHT)
}

/// Gli span dei colori per `text`. `rune_kit::syntax` tiene in cache l'ultimo risultato per testo
/// e colora i primi 128 KiB (oltre il testo resta nel colore del corpo).
pub fn spans(text: &str, theme: &SyntaxTheme) -> Vec<Span> {
    theme.spans(&syntax::highlight(Some("markdown"), text), 0)
}

/// La vista del sorgente della scheda `tab`, in modifica con `src`.
pub fn view(tab: &Tab, src: &SourceEdit, theme: &Theme) -> El {
    let id = tab.doc.id;
    let input = text_input(&src.edit, font())
        .key("source-input")
        .w_full()
        .color(theme.body)
        .caret(theme.fg, theme.sel)
        .label(tab.title())
        .spans(spans(&src.edit.text, &syntax_theme(theme)))
        .on_edit(move |s: &mut App| s.source_edit(id))
        .on_input_change(move |s: &mut App, _: &mut Tx| s.source_changed(id))
        .on_submit(move |s: &mut App, _: &mut Tx| {
            s.source_edit(id).insert("\n");
            s.source_changed(id);
        });
    let column = col().w_full().max_w(COLUMN).self_center().px(ROW_PAD_X).child(input);
    let page = col().w_full().pt(TOP).pb(BOTTOM).child(column);
    let scroll = col().key("source").w_full().h_full().scroll(ScrollSpec { stick_bottom: false }).child(page);
    el().key(format!("source-{id:x}")).w_full().grow(1.0).min_h(0.0).child(scroll)
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::time::Instant;

    use rune::{Env, Key, KeyEvent, Mods, Runtime};

    use super::*;
    use crate::app::Mode;

    fn cmd(c: char) -> KeyEvent {
        KeyEvent { key: Key::Command(c), mods: Mods { cmd: true, ..Mods::default() } }
    }

    fn typed(t: &str) -> KeyEvent {
        KeyEvent { key: Key::Text(t.into()), mods: Mods::default() }
    }

    fn settle(rt: &mut Runtime<App>) {
        for _ in 0..4000 {
            if !rt.frame(1.0 / 120.0) {
                return;
            }
        }
        panic!("la vista non si ferma");
    }

    fn open(path: &Path) -> Runtime<App> {
        let mut state = App::new(crate::index::Library::load(Vec::new(), None), true);
        state.open_paths(vec![path.to_owned()]);
        let mut rt = Runtime::new(state, crate::view, (900.0, 1000.0), Env::new(true, 2.0));
        rt.on_key = Some(crate::on_key);
        rt.frame(0.0);
        settle(&mut rt);
        rt
    }

    fn source_text(rt: &Runtime<App>) -> Option<&str> {
        match &rt.state.tab()?.mode {
            Mode::Source(s) => Some(&s.edit.text),
            Mode::Read => None,
        }
    }

    /// Cmd+E apre il sorgente con il fuoco nel campo; si scrive, Invio va a capo, Cmd+E salva e
    /// torna alla lettura con il testo salvato.
    #[test]
    fn cmd_e_edits_and_saves() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.md");
        std::fs::write(&path, "# a\n\nuno\n").unwrap();
        let mut rt = open(&path);
        rt.key(cmd('e'));
        settle(&mut rt);
        assert_eq!(source_text(&rt), Some("# a\n\nuno\n"));
        assert!(rt.elements().iter().any(|e| e.key.as_deref() == Some("source-input")), "il campo è nella vista");
        rt.key(typed("X"));
        rt.key(KeyEvent { key: Key::Enter, mods: Mods::default() });
        settle(&mut rt);
        assert_eq!(source_text(&rt), Some("X\n# a\n\nuno\n"), "scrive al cursore, Invio va a capo");
        rt.key(cmd('e'));
        settle(&mut rt);
        assert!(source_text(&rt).is_none(), "di nuovo in lettura");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "X\n# a\n\nuno\n");
        assert_eq!(rt.state.active_document().blocks.len(), 3, "la lettura mostra il testo salvato (X, titolo, uno)");
    }

    /// Costo per frame digitando nel sorgente: un tasto a metà del file e il frame che lo mostra,
    /// headless (layout e scena, niente GPU). Soglia della spec su `fixtures/big.md` (1 MB): p95
    /// ≤ 8 ms; `normal.md` (21 KB) per confronto.
    /// `cargo test --profile perf source_typing_cost -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn source_typing_cost() {
        for name in ["normal.md", "big.md"] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join(name);
            std::fs::copy(Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures").join(name), &path).unwrap();
            let mut rt = open(&path);
            rt.key(cmd('e'));
            settle(&mut rt);
            rt.update(|s, _| {
                let id = s.tab().unwrap().doc.id;
                let e = s.source_edit(id);
                let mid = (e.text.len() / 2..e.text.len()).find(|&i| e.text.is_char_boundary(i)).unwrap();
                e.set_caret(mid);
            });
            settle(&mut rt);
            let mut ms = Vec::new();
            for c in "Una frase scritta nel mezzo del file, per misurare il costo di ogni tasto. ".chars().cycle().take(200) {
                let t = Instant::now();
                rt.key(typed(&c.to_string()));
                rt.frame(1.0 / 60.0);
                ms.push(t.elapsed().as_secs_f64() * 1000.0);
            }
            ms.sort_by(f64::total_cmp);
            let pct = |p: f64| ms[((ms.len() - 1) as f64 * p).round() as usize];
            eprintln!("source_typing_cost {name}: n={} p50={:.2} ms p95={:.2} ms max={:.2} ms", ms.len(), pct(0.5), pct(0.95), pct(1.0));
        }
    }

    /// Nessun accento: ogni colore degli span è un grigio del tema.
    #[test]
    fn markdown_colors_are_theme_greys() {
        for dark in [true, false] {
            let theme = Theme::for_mode(dark);
            let src = "# Titolo\n\nTesto con `codice` e [link](https://x.y).\n\n- uno\n- due\n\n```rs\nfn a() {}\n```\n";
            let spans = spans(src, &syntax_theme(&theme));
            assert!(!spans.is_empty(), "il Markdown si colora");
            let greys = [theme.fg, theme.fg2, theme.fg3, theme.body];
            for s in &spans {
                assert!(s.color.is_none_or(|c| greys.contains(&c)), "{:?} in {:?}", s.color, &src[s.start as usize..s.end as usize]);
            }
            let title = spans.iter().find(|s| s.start == 0).expect("il titolo");
            assert_eq!((title.color, title.weight), (Some(theme.fg), Some(UI_STRONG_WEIGHT)));
        }
    }
}
