//! La vista del sorgente (Cmd+E): tutto il file in un `text_input` multilinea, nella colonna del
//! documento, con i colori Markdown di rune-highlights (via `rune_kit::syntax`) sui grigi del
//! tema: nessun accento (DESIGN.md).
//!
//! Invio va a capo (un `text_input` multilinea lo userebbe per inviare). Cmd+S e Cmd+E stanno in
//! `main::on_key`: il campo non li prende.
//!
//! Quando un salvataggio non scrive, sopra il sorgente compare una riga alta come la barra delle
//! schede, con una linea sotto: "Il file è cambiato su disco" con "Ricarica" e "Tieni il mio"
//! (righe premibili come le schede, testo `fg2`, hover `sel`), o "Salvataggio non riuscito" da
//! sola. Testo UI in `fg`, nessun accento.

use rune::{El, Family, ScrollSpec, Span, TextStyle, Tx, col, el, row, text, text_input};
use rune_kit::syntax::{self, Syntax, SyntaxTheme};

use super::doc_view::{BOTTOM, TOP};
use super::sidebar::{self, Hooks, Motion};
use super::theme::{CODE_TEXT, COLUMN, HAIRLINE, ROW, ROW_PAD, ROW_PAD_X, TAB_BAR_H, TAB_GAP, Theme, UI_STRONG_WEIGHT};
use crate::app::{App, Tab};
use crate::edit::{Notice, SourceEdit};

/// I testi della riga (spec, "Modifica"; "Salvataggio non riuscito": scelta del controller).
pub const CONFLICT_TEXT: &str = "Il file è cambiato su disco";
pub const RELOAD_TEXT: &str = "Ricarica";
pub const KEEP_TEXT: &str = "Tieni il mio";
pub const FAILED_TEXT: &str = "Salvataggio non riuscito";
/// Le chiavi della riga e dei suoi due bottoni.
pub const NOTICE_KEY: &str = "source-notice";
pub const RELOAD_KEY: &str = "source-reload";
pub const KEEP_KEY: &str = "source-keep";

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

/// La vista del sorgente della scheda `tab`, in modifica con `src`, con la riga sopra se
/// l'ultimo salvataggio non ha scritto. `hover`: il puntatore è fine e può passare sopra.
pub fn view(s: &App, hooks: Hooks<App>, tab: &Tab, src: &SourceEdit, theme: &Theme, reduced: bool, hover: bool) -> El {
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
    let scroll = col().key("source").w_full().grow(1.0).min_h(0.0).scroll(ScrollSpec { stick_bottom: false }).child(page);
    let mut view = col().key(format!("source-{id:x}")).w_full().grow(1.0).min_h(0.0);
    if let Some(notice) = src.notice {
        view = view.child(notice_row(s, hooks, id, notice, theme, Motion { reduced, hover }));
    }
    view.child(scroll)
}

/// La riga sopra il sorgente: il testo allineato a quello del sorgente, i bottoni a destra con il
/// loro testo allineato al bordo destro della colonna; una linea di 0,5 in `line` sotto.
fn notice_row(s: &App, hooks: Hooks<App>, id: u64, notice: Notice, theme: &Theme, motion: Motion) -> El {
    let message = match notice {
        Notice::Conflict => CONFLICT_TEXT,
        Notice::SaveFailed => FAILED_TEXT,
    };
    let mut line = row()
        .w_full()
        .max_w(COLUMN)
        .self_center()
        .h(TAB_BAR_H)
        .pl(ROW_PAD_X)
        .pr(ROW_PAD_X - ROW_PAD)
        .gap(TAB_GAP)
        .center_y()
        .child(text(message).font(sidebar::ui_font()).color(theme.fg).truncate().grow(1.0).shrink().min_w(0.0));
    if notice == Notice::Conflict {
        let reload = button(s, hooks, RELOAD_KEY, RELOAD_TEXT, theme, motion, move |s: &mut App| s.reload_source(id));
        let keep = button(s, hooks, KEEP_KEY, KEEP_TEXT, theme, motion, move |s: &mut App| s.keep_mine(id));
        line = line.child(reload).child(keep);
    }
    col().key(NOTICE_KEY).role("alert").label(message).w_full().child(line).child(el().w_full().h(HAIRLINE).fill(theme.line))
}

/// Un bottone della riga: come una scheda in secondo piano (alta `ROW`, `ROW_PAD` ai lati, testo
/// `fg2`, hover `sel`, scala 0,98 premuta).
fn button(s: &App, hooks: Hooks<App>, key: &str, label: &str, theme: &Theme, motion: Motion, on: impl Fn(&mut App) + 'static) -> El {
    let e = row()
        .h(ROW)
        .px(ROW_PAD)
        .center_y()
        .role("button")
        .label(label)
        .child(text(label).font(sidebar::ui_font()).color(theme.fg2).a11y_hidden())
        .on_press(on);
    sidebar::pressable(e, key.to_owned(), &s.sidebar, hooks, theme, motion, true).key(key)
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

    fn texts(rt: &Runtime<App>) -> Vec<String> {
        rt.elements().into_iter().filter_map(|e| e.text).collect()
    }

    fn has(rt: &Runtime<App>, key: &str) -> bool {
        rt.elements().iter().any(|e| e.key.as_deref() == Some(key))
    }

    fn tap(rt: &mut Runtime<App>, key: &str) {
        let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} non c'è"));
        rt.pointer_move(x, y);
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        settle(rt);
    }

    /// Cmd+E, si scrive, il file cambia su disco, Cmd+S: compare la riga e niente si scrive.
    /// "Tieni il mio" scrive il testo locale e la riga sparisce.
    #[test]
    fn conflict_row_and_keep_mine() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.md");
        std::fs::write(&path, "# a\n\nuno\n").unwrap();
        let mut rt = open(&path);
        rt.key(cmd('e'));
        rt.key(typed("X"));
        settle(&mut rt);
        assert!(!has(&rt, NOTICE_KEY), "nessuna riga prima del conflitto");
        std::fs::write(&path, "# a\n\ndell'agente\n").unwrap();
        rt.key(cmd('s'));
        settle(&mut rt);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "# a\n\ndell'agente\n", "non scrive");
        assert!(has(&rt, NOTICE_KEY), "compare la riga");
        let t = texts(&rt);
        for want in [CONFLICT_TEXT, RELOAD_TEXT, KEEP_TEXT] {
            assert!(t.iter().any(|x| x == want), "{want} in {t:?}");
        }
        // Con la riga sopra il campo tiene il fuoco: si continua a scrivere.
        rt.key(typed("Y"));
        settle(&mut rt);
        assert_eq!(source_text(&rt), Some("XY# a\n\nuno\n"));
        tap(&mut rt, KEEP_KEY);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "XY# a\n\nuno\n", "il disco ha il testo locale");
        assert!(!has(&rt, NOTICE_KEY), "la riga sparisce");
        assert_eq!(source_text(&rt), Some("XY# a\n\nuno\n"), "resta nel sorgente");
    }

    /// "Ricarica" scarta le modifiche: il sorgente mostra il testo dell'agente, il disco non cambia.
    #[test]
    fn conflict_reload_discards() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.md");
        std::fs::write(&path, "# a\n\nuno\n").unwrap();
        let mut rt = open(&path);
        rt.key(cmd('e'));
        rt.key(typed("X"));
        settle(&mut rt);
        let agent = "# a\n\ndell'agente\n";
        std::fs::write(&path, agent).unwrap();
        rt.key(cmd('e'));
        settle(&mut rt);
        assert!(has(&rt, NOTICE_KEY), "Cmd+E non esce: riga");
        tap(&mut rt, RELOAD_KEY);
        assert_eq!(source_text(&rt), Some(agent));
        assert!(!has(&rt, NOTICE_KEY));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), agent);
        rt.key(cmd('e'));
        settle(&mut rt);
        assert!(source_text(&rt).is_none(), "senza modifiche Cmd+E torna alla lettura");
    }

    /// Cmd+W rifiutato su una scheda in secondo piano (clic centrale): la riga si vede quando la
    /// scheda torna davanti.
    #[test]
    fn a_refused_close_shows_the_row_when_the_tab_comes_to_the_front() {
        let dir = tempfile::tempdir().unwrap();
        let (a, b) = (dir.path().join("a.md"), dir.path().join("b.md"));
        std::fs::write(&a, "# a\n").unwrap();
        std::fs::write(&b, "# b\n").unwrap();
        let mut rt = open(&a);
        rt.key(cmd('e'));
        rt.key(typed("X"));
        settle(&mut rt);
        rt.update(|s, _| s.open_paths(vec![b.clone()]));
        settle(&mut rt);
        std::fs::write(&a, "# a\n\ndell'agente\n").unwrap();
        rt.update(|s, _| assert!(!s.close(0), "non si chiude"));
        settle(&mut rt);
        assert!(!has(&rt, NOTICE_KEY), "la scheda davanti è b");
        rt.key(cmd('1'));
        settle(&mut rt);
        assert!(has(&rt, NOTICE_KEY), "a davanti: la riga");
        assert!(texts(&rt).iter().any(|x| x == CONFLICT_TEXT));
    }

    /// Un errore di scrittura: la riga dice "Salvataggio non riuscito", senza i due bottoni.
    #[test]
    fn a_failed_save_says_so() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.md");
        std::fs::write(&path, "# a\n").unwrap();
        let mut rt = open(&path);
        rt.key(cmd('e'));
        rt.key(typed("X"));
        settle(&mut rt);
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o555)).unwrap();
        rt.key(cmd('s'));
        settle(&mut rt);
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(has(&rt, NOTICE_KEY));
        let t = texts(&rt);
        assert!(t.iter().any(|x| x == FAILED_TEXT), "{t:?}");
        assert!(!t.iter().any(|x| x == CONFLICT_TEXT || x == KEEP_TEXT));
        assert_eq!(source_text(&rt), Some("X# a\n"), "le modifiche restano");
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
