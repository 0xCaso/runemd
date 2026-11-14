//! La vista del sorgente (Cmd+E): tutto il file nella colonna del documento, un `text_input`
//! multilinea per pezzo ([`crate::edit::blocks`]) dentro `virtual_rows`, così Rune impagina e
//! colora solo le righe montate. I colori Markdown sono di rune-highlights (via
//! `rune_kit::syntax`) sui grigi del tema: nessun accento (DESIGN.md). Ogni pezzo si colora da
//! sé, quindi tutto il file ha colori (non solo i primi 128 KiB).
//!
//! Il sorgente si comporta come un campo solo: Invio va a capo nel pezzo (un `text_input`
//! multilinea lo userebbe per inviare); Backspace all'inizio e Canc alla fine uniscono due pezzi;
//! le frecce ai bordi passano al pezzo vicino (su e giù dalla prima e dall'ultima riga disegnata,
//! che la vista legge da `Cx::caret`: [`caret_lines`]). Una selezione resta dentro un pezzo.
//! Cmd+S e Cmd+E stanno in `main::on_key`: il campo non li prende.
//!
//! Quando un salvataggio non scrive, sopra il sorgente compare una riga alta come la barra delle
//! schede, con una linea sotto: "Il file è cambiato su disco" con "Ricarica" e "Tieni il mio"
//! (righe premibili come le schede, testo `fg2`, hover `sel`), o "Salvataggio non riuscito" da
//! sola. Testo UI in `fg`, nessun accento.
//!
//! Cmd+W o il clic centrale su una scheda con modifiche non salvate: al posto di quella riga, con
//! lo stesso aspetto, "Salvare le modifiche a <nome>?" con "Salva" (anche Invio), "Non salvare" e
//! "Annulla" (anche Esc). Il campo tiene il fuoco ([`App::answer_close`]).

use std::borrow::Cow;

use rune::{Cx, El, Family, GrowthSpec, Key, KeyEvent, ScrollSpec, Span, TextStyle, Tx, col, el, memo, row, text, text_input};
use rune_kit::syntax::{self, Syntax, SyntaxTheme};

use super::doc_view::{BOTTOM, TOP};
use super::sidebar::{self, Hooks, Motion};
use super::theme::{CODE_TEXT, COLUMN, HAIRLINE, ROW, ROW_PAD, ROW_PAD_X, TAB_BAR_H, TAB_GAP, Theme, UI_STRONG_WEIGHT};
use crate::app::{App, CloseAnswer, Mode, Tab};
use crate::edit::blocks::{Edge, Pieces};
use crate::edit::{Notice, SourceEdit};

/// I testi della riga (spec, "Modifica"; "Salvataggio non riuscito": scelta del controller).
pub const CONFLICT_TEXT: &str = "Il file è cambiato su disco";
pub const RELOAD_TEXT: &str = "Ricarica";
pub const KEEP_TEXT: &str = "Tieni il mio";
pub const FAILED_TEXT: &str = "Salvataggio non riuscito";
/// Cmd+E su un file che non è UTF-8.
pub const NOT_UTF8_TEXT: &str = "Il file non è UTF-8: modifica disattivata";
/// Le chiavi della riga e dei suoi due bottoni.
pub const NOTICE_KEY: &str = "source-notice";
/// La riga sopra la lettura quando Cmd+E non entra perché il file non è UTF-8.
pub const NOT_UTF8_KEY: &str = "read-not-utf8";
pub const RELOAD_KEY: &str = "source-reload";
pub const KEEP_KEY: &str = "source-keep";
/// I bottoni della riga che chiede alla chiusura (record `2026-11-14-cmdw-modifiche-non-salvate`).
pub const SAVE_TEXT: &str = "Salva";
pub const DISCARD_TEXT: &str = "Non salvare";
pub const CANCEL_TEXT: &str = "Annulla";
/// Le chiavi della riga che chiede e dei suoi bottoni. Solo la scheda davanti la mostra: come
/// quelle della riga del conflitto non portano l'id della scheda, così uno script le trova.
pub const ASK_KEY: &str = "source-ask";
pub const SAVE_KEY: &str = "source-ask-save";
pub const DISCARD_KEY: &str = "source-ask-discard";
pub const CANCEL_KEY: &str = "source-ask-cancel";

/// La domanda della riga alla chiusura, con il nome del file.
pub fn ask_text(name: &str) -> String {
    format!("Salvare le modifiche a {name}?")
}

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

/// Gli span dei colori per `text` (un pezzo). `rune_kit::syntax` tiene in cache l'ultimo
/// risultato e colora i primi 128 KiB: un pezzo è molto più corto.
pub fn spans(text: &str, theme: &SyntaxTheme) -> Vec<Span> {
    theme.spans(&syntax::highlight(Some("markdown"), text), 0)
}

/// Dove sta il cursore nel pezzo `piece`, che ha il fuoco: sulla prima e/o sull'ultima riga
/// disegnata. Decide se freccia su e giù passano al pezzo vicino.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CaretLines {
    pub piece: u64,
    pub first: bool,
    pub last: bool,
}

/// La chiave del campo del pezzo `piece` della scheda `id`.
pub fn input_key(id: u64, piece: u64) -> String {
    format!("src-{id:x}-{piece:x}")
}

/// La chiave della riga del pezzo: id della scheda e del pezzo (stabile mentre si scrive).
fn row_key(id: u64, piece: u64) -> String {
    format!("{id:x}-p{piece:x}")
}

/// [`CaretLines`] del pezzo con il fuoco, dal cursore e dal campo come sono disegnati (punti
/// della finestra, dopo lo scroll). Prima del primo layout: dalle righe del testo.
pub fn caret_lines(cx: &Cx<App>, tab: &Tab, src: &SourceEdit) -> Option<CaretLines> {
    let i = src.pieces.focused()?;
    let p = src.pieces.get(i)?;
    let key = input_key(tab.doc.id, p.id);
    let (first, last) = match (cx.caret(&key), cx.window_layout(&key)) {
        (Some([_, y, _, h]), Some([_, top, _, height])) => (y < top + h * 0.5, y + h > top + height - h * 0.5),
        _ => {
            let at = p.edit.focus.min(p.edit.text.len());
            (!p.edit.text[..at].contains('\n'), !p.edit.text[at..].contains('\n'))
        }
    };
    Some(CaretLines { piece: p.id, first, last })
}

fn pieces_of(s: &App, id: u64) -> Option<&Pieces> {
    match &s.tabs.iter().find(|t| t.doc.id == id)?.mode {
        Mode::Source(src) => Some(&src.pieces),
        Mode::Read => None,
    }
}

/// Il bordo che il tasto `ev` tocca, se è uno di quelli che passano da un pezzo all'altro.
fn edge_of(ev: &KeyEvent) -> Option<Edge> {
    let m = ev.mods;
    let plain = !m.shift && !m.cmd && !m.alt && !m.ctrl;
    match ev.key {
        Key::Backspace if !m.cmd && !m.ctrl => Some(Edge::JoinBack),
        Key::Delete if !m.cmd && !m.ctrl => Some(Edge::JoinForward),
        Key::Up if plain => Some(Edge::Up),
        Key::Down if plain => Some(Edge::Down),
        Key::Left if plain => Some(Edge::Left),
        Key::Right if plain => Some(Edge::Right),
        _ => None,
    }
}

/// Con la riga che chiede alla chiusura aperta, Invio risponde "Salva" ed Esc "Annulla", prima
/// del campo (che andrebbe a capo). Vero se ha preso il tasto.
pub fn ask_key(s: &mut App, id: u64, ev: &KeyEvent, tx: &mut Tx) -> bool {
    if !s.asking(id) || ev.mods != rune::Mods::default() {
        return false;
    }
    let answer = match ev.key {
        Key::Enter => CloseAnswer::Save,
        Key::Escape => CloseAnswer::Cancel,
        _ => return false,
    };
    if !tx.key_repeat() {
        s.answer_close_tx(id, answer, tx);
    }
    true
}

/// La riga di un pezzo: il suo campo, con i colori, nella colonna.
fn piece_row(id: u64, piece: u64, edit: &rune::TextEdit, style: &Style, lines: (bool, bool)) -> El {
    let (first, last) = lines;
    let input = text_input(edit, font())
        .key(input_key(id, piece))
        .w_full()
        .color(style.body)
        .caret(style.caret, style.sel)
        .label(&*style.label)
        .spans(spans(&edit.text, &style.syntax))
        .on_edit(move |s: &mut App| s.source_edit(id, piece))
        .on_input_change(move |s: &mut App, _: &mut Tx| s.source_changed(id))
        .on_submit(move |s: &mut App, _: &mut Tx| {
            s.source_edit(id, piece).insert("\n");
            s.source_changed(id);
        })
        .on_input_key(move |s: &mut App, ev: &KeyEvent, tx: &mut Tx| ask_key(s, id, ev, tx) || edge_of(ev).is_some_and(|e| s.source_edge(id, piece, e, first, last, tx)));
    col().w_full().px(ROW_PAD_X).child(input)
}

/// Quello che serve per disegnare un pezzo, copiabile nelle closure delle righe.
#[derive(Clone, Hash)]
struct Style {
    syntax: SyntaxTheme,
    body: rune::Color,
    caret: rune::Color,
    sel: rune::Color,
    label: std::rc::Rc<str>,
}

/// L'altezza stimata di un pezzo non ancora misurato: qualche riga di `doc-code`.
const PIECE_ESTIMATE: f32 = 80.0;

/// La vista del sorgente della scheda `tab`, in modifica con `src`, con la riga sopra se
/// l'ultimo salvataggio non ha scritto. `caret`: [`caret_lines`]. `hover`: il puntatore è fine
/// e può passare sopra.
#[allow(clippy::too_many_arguments)]
pub fn view(s: &App, hooks: Hooks<App>, tab: &Tab, src: &SourceEdit, caret: Option<CaretLines>, theme: &Theme, reduced: bool, hover: bool) -> El {
    let id = tab.doc.id;
    let style = Style { syntax: syntax_theme(theme), body: theme.body, caret: theme.fg, sel: theme.sel, label: tab.title().into() };
    let rows = col()
        .key("source-rows")
        .w_full()
        .max_w(COLUMN)
        .self_center()
        .growth(GrowthSpec::default())
        .virtual_rows(
            src.pieces.len(),
            PIECE_ESTIMATE,
            move |s: &App, i| Cow::Owned(pieces_of(s, id).and_then(|p| p.get(i)).map_or_else(|| format!("{id:x}-i{i:x}"), |p| row_key(id, p.id))),
            move |s: &App, i| {
                let Some(p) = pieces_of(s, id).and_then(|p| p.get(i)) else { return el() };
                let lines = caret.filter(|c| c.piece == p.id).map_or((false, false), |c| (c.first, c.last));
                memo(format!("piece-{id:x}-{:x}", p.id), (&p.edit, lines, &style), || piece_row(id, p.id, &p.edit, &style, lines))
            },
        )
        .virtual_rows_version((id, src.pieces.version));
    let page = col().w_full().pt(TOP).pb(BOTTOM).child(rows);
    let scroll = col().key("source").w_full().grow(1.0).min_h(0.0).scroll(ScrollSpec { stick_bottom: false }).child(page);
    let mut view = col().key(format!("source-{id:x}")).transition(super::theme::mode_transition(reduced)).w_full().grow(1.0).min_h(0.0);
    let motion = Motion { reduced, hover };
    if src.asking {
        view = view.child(ask_row(s, hooks, id, &tab.title(), theme, motion));
    } else if let Some(notice) = src.notice {
        view = view.child(notice_row(s, hooks, id, notice, theme, motion));
    }
    view.child(scroll)
}

/// La riga del conflitto o dell'errore (vedi [`alert_row`]).
fn notice_row(s: &App, hooks: Hooks<App>, id: u64, notice: Notice, theme: &Theme, motion: Motion) -> El {
    let message = match notice {
        Notice::Conflict => CONFLICT_TEXT,
        Notice::SaveFailed => FAILED_TEXT,
        Notice::NotUtf8 => NOT_UTF8_TEXT,
    };
    let mut buttons = Vec::new();
    if notice == Notice::Conflict {
        buttons.push(button(s, hooks, RELOAD_KEY, RELOAD_TEXT, theme, motion, move |s: &mut App, _: &mut Tx| s.reload_source(id)));
        buttons.push(button(s, hooks, KEEP_KEY, KEEP_TEXT, theme, motion, move |s: &mut App, _: &mut Tx| s.keep_mine(id)));
    }
    alert_row(NOTICE_KEY, message, buttons, theme)
}

/// La riga sopra la lettura dopo un Cmd+E su un file che non è UTF-8 (come quella del
/// conflitto, senza bottoni).
pub fn not_utf8_row(theme: &Theme) -> El {
    alert_row(NOT_UTF8_KEY, NOT_UTF8_TEXT, Vec::new(), theme)
}

/// La riga che chiede alla chiusura con modifiche non salvate. "Salva" è il bottone di default
/// della finestra: Invio lo preme anche quando il campo non ha il fuoco.
fn ask_row(s: &App, hooks: Hooks<App>, id: u64, name: &str, theme: &Theme, motion: Motion) -> El {
    let answer = |a: CloseAnswer| move |s: &mut App, tx: &mut Tx| s.answer_close_tx(id, a, tx);
    let buttons = vec![
        button(s, hooks, SAVE_KEY, SAVE_TEXT, theme, motion, answer(CloseAnswer::Save)).default_button(),
        button(s, hooks, DISCARD_KEY, DISCARD_TEXT, theme, motion, answer(CloseAnswer::Discard)),
        button(s, hooks, CANCEL_KEY, CANCEL_TEXT, theme, motion, answer(CloseAnswer::Cancel)),
    ];
    alert_row(ASK_KEY, &ask_text(name), buttons, theme)
}

/// Una riga sopra il sorgente, alta come la barra delle schede e annunciata come `alert`: il
/// testo allineato a quello del sorgente, i bottoni a destra con il loro testo allineato al bordo
/// destro della colonna; una linea di 0,5 in `line` sotto.
fn alert_row(key: &str, message: &str, buttons: Vec<El>, theme: &Theme) -> El {
    let line = row()
        .w_full()
        .max_w(COLUMN)
        .self_center()
        .h(TAB_BAR_H)
        .pl(ROW_PAD_X)
        .pr(ROW_PAD_X - ROW_PAD)
        .gap(TAB_GAP)
        .center_y()
        .child(text(message).font(sidebar::ui_font()).color(theme.fg).truncate().grow(1.0).shrink().min_w(0.0).a11y_hidden())
        .children(buttons);
    col().key(key).role("alert").label(message).w_full().child(line).child(el().w_full().h(HAIRLINE).fill(theme.line))
}

/// Un bottone della riga: come una scheda in secondo piano (alta `ROW`, `ROW_PAD` ai lati, testo
/// `fg2`, hover `sel`, scala 0,98 premuta).
fn button(s: &App, hooks: Hooks<App>, key: &str, label: &str, theme: &Theme, motion: Motion, on: impl Fn(&mut App, &mut Tx) + 'static) -> El {
    let e = row()
        .h(ROW)
        .px(ROW_PAD)
        .center_y()
        .role("button")
        .label(label)
        .child(text(label).font(sidebar::ui_font()).color(theme.fg2).a11y_hidden())
        .on_press_tx(on);
    sidebar::pressable(e, key.to_owned(), &s.sidebar, hooks, theme, motion, true).key(key)
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::time::Instant;

    use rune::{Env, Key, KeyEvent, Mods, Runtime};

    use super::*;

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
        let mut state = App::bare(crate::index::Library::load(Vec::new(), None));
        state.open_paths(vec![path.to_owned()]);
        let mut rt = Runtime::new(state, crate::view, (900.0, 1000.0), Env::new(true, 2.0));
        rt.on_key = Some(crate::on_key);
        rt.frame(0.0);
        settle(&mut rt);
        rt
    }

    fn source_text(rt: &Runtime<App>) -> Option<String> {
        match &rt.state.tab()?.mode {
            Mode::Source(s) => Some(s.text()),
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
        assert_eq!(source_text(&rt).as_deref(), Some("# a\n\nuno\n"));
        assert!(rt.elements().iter().any(|e| e.key.as_deref().is_some_and(|k| k.starts_with("src-"))), "il campo è nella vista");
        rt.key(typed("X"));
        rt.key(KeyEvent { key: Key::Enter, mods: Mods::default() });
        settle(&mut rt);
        assert_eq!(source_text(&rt).as_deref(), Some("X\n# a\n\nuno\n"), "scrive al cursore, Invio va a capo");
        rt.key(cmd('e'));
        settle(&mut rt);
        assert!(source_text(&rt).as_deref().is_none(), "di nuovo in lettura");
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
        assert_eq!(source_text(&rt).as_deref(), Some("XY# a\n\nuno\n"));
        tap(&mut rt, KEEP_KEY);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "XY# a\n\nuno\n", "il disco ha il testo locale");
        assert!(!has(&rt, NOTICE_KEY), "la riga sparisce");
        assert_eq!(source_text(&rt).as_deref(), Some("XY# a\n\nuno\n"), "resta nel sorgente");
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
        assert_eq!(source_text(&rt).as_deref(), Some(agent));
        assert!(!has(&rt, NOTICE_KEY));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), agent);
        rt.key(cmd('e'));
        settle(&mut rt);
        assert!(source_text(&rt).as_deref().is_none(), "senza modifiche Cmd+E torna alla lettura");
    }

    fn esc() -> KeyEvent {
        KeyEvent { key: Key::Escape, mods: Mods::default() }
    }

    fn element(rt: &Runtime<App>, key: &str) -> rune::ElementInfo {
        rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key)).unwrap_or_else(|| panic!("{key} non c'è"))
    }

    /// Nel sorgente con modifiche: Cmd+E, si scrive "X" all'inizio di a.md.
    fn edited(path: &Path) -> Runtime<App> {
        std::fs::write(path, "# a\n\nuno\n").unwrap();
        let mut rt = open(path);
        rt.key(cmd('e'));
        rt.key(typed("X"));
        settle(&mut rt);
        rt
    }

    /// Senza modifiche Cmd+W chiude subito, anche nel sorgente.
    #[test]
    fn cmd_w_without_edits_closes_at_once() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.md");
        std::fs::write(&path, "# a\n").unwrap();
        let mut rt = open(&path);
        rt.key(cmd('e'));
        settle(&mut rt);
        rt.key(cmd('w'));
        settle(&mut rt);
        assert!(rt.state.tabs.is_empty());
        assert!(!has(&rt, ASK_KEY));
    }

    /// Cmd+W con modifiche: la riga chiede, annunciata come quella del conflitto, con tre bottoni
    /// dal nome accessibile. Un secondo Cmd+W non chiude. Esc la toglie: modifiche e cursore restano
    /// e il campo ha ancora il fuoco.
    #[test]
    fn cmd_w_with_edits_asks_and_esc_cancels() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.md");
        let mut rt = edited(&path);
        rt.key(cmd('w'));
        settle(&mut rt);
        assert!(has(&rt, ASK_KEY), "compare la riga");
        assert!(!has(&rt, NOTICE_KEY));
        let question = ask_text("a.md");
        assert_eq!(question, "Salvare le modifiche a a.md?");
        let t = texts(&rt);
        for want in [question.as_str(), SAVE_TEXT, DISCARD_TEXT, CANCEL_TEXT] {
            assert!(t.iter().any(|x| x == want), "{want} in {t:?}");
        }
        let row = element(&rt, ASK_KEY);
        assert_eq!((row.role, row.label.as_deref()), ("alert", Some(question.as_str())), "annunciata come il conflitto");
        for (key, label) in [(SAVE_KEY, SAVE_TEXT), (DISCARD_KEY, DISCARD_TEXT), (CANCEL_KEY, CANCEL_TEXT)] {
            let b = element(&rt, key);
            assert_eq!((b.role, b.label.as_deref()), ("button", Some(label)));
        }
        rt.key(cmd('w'));
        settle(&mut rt);
        assert_eq!(rt.state.tabs.len(), 1, "il secondo Cmd+W non chiude");
        assert!(has(&rt, ASK_KEY), "resta la riga");
        rt.key(esc());
        settle(&mut rt);
        assert!(!has(&rt, ASK_KEY), "Esc toglie la riga");
        assert_eq!(source_text(&rt).as_deref(), Some("X# a\n\nuno\n"), "modifiche intatte");
        assert_eq!(caret(&rt), 1, "cursore intatto");
        rt.key(typed("Y"));
        settle(&mut rt);
        assert_eq!(source_text(&rt).as_deref(), Some("XY# a\n\nuno\n"), "il campo ha ancora il fuoco");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "# a\n\nuno\n", "niente scritto");
    }

    /// Invio risponde "Salva": scrive e chiude.
    #[test]
    fn enter_saves_and_closes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.md");
        let mut rt = edited(&path);
        rt.key(cmd('w'));
        settle(&mut rt);
        rt.key(key(Key::Enter));
        settle(&mut rt);
        assert!(rt.state.tabs.is_empty(), "chiusa");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "X# a\n\nuno\n");
    }

    /// "Salva" con il file cambiato su disco: la scheda resta, la riga diventa quella del conflitto.
    #[test]
    fn save_with_a_conflict_shows_the_conflict_row() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.md");
        let mut rt = edited(&path);
        rt.key(cmd('w'));
        settle(&mut rt);
        std::fs::write(&path, "# a\n\ndell'agente\n").unwrap();
        tap(&mut rt, SAVE_KEY);
        assert_eq!(rt.state.tabs.len(), 1, "non chiusa");
        assert!(!has(&rt, ASK_KEY));
        assert!(texts(&rt).iter().any(|x| x == CONFLICT_TEXT), "la riga del conflitto");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "# a\n\ndell'agente\n");
    }

    /// "Non salvare": chiude, il file su disco non cambia.
    #[test]
    fn discard_closes_without_writing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.md");
        let mut rt = edited(&path);
        rt.key(cmd('w'));
        settle(&mut rt);
        tap(&mut rt, DISCARD_KEY);
        assert!(rt.state.tabs.is_empty(), "chiusa");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "# a\n\nuno\n", "il file non cambia");
    }

    /// "Annulla": la riga sparisce, modifiche e cursore restano.
    #[test]
    fn cancel_keeps_the_edits() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.md");
        let mut rt = edited(&path);
        rt.key(cmd('w'));
        settle(&mut rt);
        tap(&mut rt, CANCEL_KEY);
        assert_eq!(rt.state.tabs.len(), 1);
        assert!(!has(&rt, ASK_KEY));
        assert_eq!(source_text(&rt).as_deref(), Some("X# a\n\nuno\n"));
        assert_eq!(caret(&rt), 1);
    }

    /// L'opacità di ciò che entra (`enter`) e di ciò che esce (`exit`, se c'è ancora) dopo Cmd+E,
    /// frame per frame a 120 Hz: (ms, entra, esce).
    fn fade(rt: &mut Runtime<App>, enter: &str, exit: &str) -> Vec<(f32, f32, Option<f32>)> {
        let op = |rt: &Runtime<App>, k: &str| rt.elements().into_iter().find(|e| e.key.as_deref() == Some(k)).map(|e| e.opacity);
        rt.key(cmd('e'));
        (1..=60).map(|f| {
            rt.frame(1.0 / 120.0);
            (f as f32 * 1000.0 / 120.0, op(rt, enter).unwrap_or(0.0), op(rt, exit))
        }).collect()
    }

    /// Cmd+E più veloce (v1.1): il sorgente entra al 95 % entro 175 ms e la lettura esce entro
    /// 150; al ritorno la lettura entra al 95 % entro 175 ms. Si vede la dissolvenza (non è un
    /// taglio). Con "riduci movimento" è immediato.
    #[test]
    fn cmd_e_cross_fades_fast() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.md");
        std::fs::write(&path, "# a\n\nuno\n").unwrap();
        let mut rt = open(&path);
        let id = rt.state.tab().unwrap().doc.id;
        let src = format!("source-{id:x}");
        let check = |what: &str, frames: &[(f32, f32, Option<f32>)]| {
            eprintln!("{what}: {:?}", frames.iter().step_by(3).collect::<Vec<_>>());
            let at = |ms: f32| frames.iter().find(|f| f.0 >= ms).unwrap();
            assert!(at(25.0).1 < 0.9, "{what}: si vede la dissolvenza");
            assert!(at(175.0).1 >= 0.95, "{what}: al 95 % entro 175 ms: {:?}", at(175.0));
            assert!(at(150.0).2.is_none_or(|o| o <= 0.05), "{what}: l'altra fuori entro 150 ms: {:?}", at(150.0));
        };
        let enter = fade(&mut rt, &src, "doc-pane");
        check("sorgente", &enter);
        settle(&mut rt);
        let back = fade(&mut rt, "doc-pane", &src);
        check("lettura", &back);
        settle(&mut rt);
        rt.update(|s, _| s.motion_env = Some(true));
        settle(&mut rt);
        let reduced = fade(&mut rt, &src, "doc-pane");
        assert!(reduced[0].1 >= 0.99 && reduced[0].2.is_none_or(|o| o <= 0.01), "riduci movimento: subito {:?}", reduced[0]);
    }

    /// La × su una scheda in secondo piano con modifiche chiede come Cmd+W: viene davanti con la
    /// riga e niente si scrive.
    #[test]
    fn the_close_button_on_a_tab_with_edits_asks() {
        let dir = tempfile::tempdir().unwrap();
        let (a, b) = (dir.path().join("a.md"), dir.path().join("b.md"));
        std::fs::write(&b, "# b\n").unwrap();
        let mut rt = edited(&a);
        rt.update(|s, _| s.open_paths(vec![b.clone()]));
        settle(&mut rt);
        let tab = format!("t-{:x}", rt.state.tabs[0].doc.id);
        let (x, y) = rt.find(&tab).unwrap();
        rt.pointer_move(x, y);
        settle(&mut rt);
        tap(&mut rt, &format!("{tab}-close"));
        assert_eq!((rt.state.tabs.len(), rt.state.active), (2, 0), "a davanti, aperta");
        assert!(has(&rt, ASK_KEY), "la riga");
        assert_eq!(std::fs::read_to_string(&a).unwrap(), "# a\n\nuno\n");
    }

    /// Il clic centrale su una scheda in secondo piano con modifiche: viene davanti con la riga.
    #[test]
    fn a_middle_click_on_a_background_tab_with_edits_asks() {
        let dir = tempfile::tempdir().unwrap();
        let (a, b) = (dir.path().join("a.md"), dir.path().join("b.md"));
        std::fs::write(&b, "# b\n").unwrap();
        let mut rt = edited(&a);
        rt.update(|s, _| s.open_paths(vec![b.clone()]));
        settle(&mut rt);
        assert!(!has(&rt, ASK_KEY));
        let tab = format!("t-{:x}", rt.state.tabs[0].doc.id);
        let (x, y) = rt.find(&tab).unwrap();
        rt.pointer_move(x, y);
        rt.button_down(x, y, rune::PointerButton::Middle);
        rt.button_up(x, y, rune::PointerButton::Middle);
        settle(&mut rt);
        assert_eq!((rt.state.tabs.len(), rt.state.active), (2, 0), "a davanti, aperta");
        assert!(has(&rt, ASK_KEY), "la riga");
        assert_eq!(std::fs::read_to_string(&a).unwrap(), "# a\n\nuno\n");
    }

    /// Un file che non è UTF-8 non entra nel sorgente (lo riscriverebbe con U+FFFD): Cmd+E
    /// lascia la lettura e una riga lo dice. Il file non cambia.
    #[test]
    fn a_file_that_is_not_utf8_stays_in_read_mode() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.md");
        let bytes = b"# a\n\ncaff\xe8 \xff\n".to_vec();
        std::fs::write(&path, &bytes).unwrap();
        let mut rt = open(&path);
        assert!(!has(&rt, NOT_UTF8_KEY));
        rt.key(cmd('e'));
        settle(&mut rt);
        assert!(source_text(&rt).is_none(), "resta in lettura");
        assert!(has(&rt, NOT_UTF8_KEY), "la riga");
        assert!(texts(&rt).iter().any(|t| t == NOT_UTF8_TEXT));
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
    }

    /// Un errore di scrittura: la riga dice "Salvataggio non riuscito", senza i due bottoni.
    #[test]
    fn a_failed_save_says_so() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        if crate::edit::tests::as_root(dir.path()) {
            return;
        }
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
        assert_eq!(source_text(&rt).as_deref(), Some("X# a\n"), "le modifiche restano");
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
            rt.update(|s, tx| {
                let id = s.tab().unwrap().doc.id;
                let Some(Mode::Source(src)) = s.tabs.iter_mut().find(|t| t.doc.id == id).map(|t| &mut t.mode) else { unreachable!() };
                let mid = src.text().len() / 2;
                src.pieces.set_caret(mid);
                let piece = src.pieces.get(src.pieces.focused().unwrap()).unwrap().id;
                tx.scroll_into_view(row_key(id, piece));
            });
            settle(&mut rt);
            let mut ms = Vec::new();
            for c in "Una frase scritta nel mezzo del file, per misurare il costo di ogni tasto. ".chars().cycle().take(200) {
                let t = Instant::now();
                rt.key(typed(&c.to_string()));
                rt.frame(1.0 / 60.0);
                ms.push(t.elapsed().as_secs_f64() * 1000.0);
            }
            let text = source_text(&rt).unwrap();
            let at = text.find("per misurare il costo di ogni tasto. Una frase").expect("il testo scritto è nel sorgente");
            assert!(at > text.len() / 3, "scritto a metà del file");
            ms.sort_by(f64::total_cmp);
            let pct = |p: f64| ms[((ms.len() - 1) as f64 * p).round() as usize];
            eprintln!("source_typing_cost {name}: n={} p50={:.2} ms p95={:.2} ms max={:.2} ms", ms.len(), pct(0.5), pct(0.95), pct(1.0));
        }
    }

    fn key(k: Key) -> KeyEvent {
        KeyEvent { key: k, mods: Mods::default() }
    }

    fn caret(rt: &Runtime<App>) -> usize {
        match &rt.state.tab().unwrap().mode {
            Mode::Source(s) => s.pieces.caret(),
            Mode::Read => panic!("non nel sorgente"),
        }
    }

    /// Il sorgente in pezzi si usa come un campo solo: le frecce passano da un pezzo all'altro,
    /// Invio va a capo nel pezzo, Backspace all'inizio e Canc alla fine tolgono il `\n` fra due
    /// pezzi; quello che si scrive va nel pezzo dove sta il cursore.
    #[test]
    fn pieces_edit_like_one_field() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.md");
        std::fs::write(&path, "# a\n\nuno\n\ndue\n").unwrap();
        let mut rt = open(&path);
        rt.key(cmd('e'));
        settle(&mut rt);
        // Giù dalla prima riga del primo pezzo ("# a\n"): la sua ultima riga è quella vuota.
        rt.key(key(Key::Down));
        settle(&mut rt);
        assert_eq!(caret(&rt), 4, "riga vuota del primo pezzo");
        rt.key(key(Key::Down));
        settle(&mut rt);
        assert_eq!(caret(&rt), 5, "nel secondo pezzo");
        rt.key(typed("X"));
        settle(&mut rt);
        assert_eq!(source_text(&rt).as_deref(), Some("# a\n\nXuno\n\ndue\n"), "scrive nel secondo pezzo");
        rt.key(key(Key::Up));
        settle(&mut rt);
        assert_eq!(caret(&rt), 4, "su: di nuovo nel primo pezzo");
        rt.key(key(Key::Right));
        rt.key(key(Key::Right));
        settle(&mut rt);
        assert_eq!(caret(&rt), 6, "destra: dopo la X");
        rt.key(key(Key::Left));
        settle(&mut rt);
        rt.key(key(Key::Backspace));
        settle(&mut rt);
        assert_eq!(source_text(&rt).as_deref(), Some("# a\nXuno\n\ndue\n"), "Backspace all'inizio unisce al pezzo prima");
        assert_eq!(caret(&rt), 4);
        rt.key(key(Key::Enter));
        settle(&mut rt);
        assert_eq!(source_text(&rt).as_deref(), Some("# a\n\nXuno\n\ndue\n"), "Invio va a capo");
        // Canc alla fine del primo pezzo ("# a\n\nXuno\n"): toglie il \n prima di "due".
        if let Mode::Source(s) = &mut rt.state.tabs[0].mode {
            s.pieces.set_caret(10);
        }
        rt.invalidate();
        settle(&mut rt);
        rt.key(key(Key::Delete));
        settle(&mut rt);
        assert_eq!(source_text(&rt).as_deref(), Some("# a\n\nXuno\ndue\n"), "Canc alla fine unisce il pezzo dopo");
        rt.key(cmd('e'));
        settle(&mut rt);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "# a\n\nXuno\ndue\n");
    }

    /// Le frecce attraversano molti pezzi di `big.md`: il cursore resta nella vista (lo scroll
    /// lo segue) e il fuoco è nel pezzo del cursore.
    #[test]
    fn the_caret_stays_in_view_across_pieces() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("big.md");
        std::fs::copy(Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/big.md"), &path).unwrap();
        let mut rt = open(&path);
        rt.key(cmd('e'));
        settle(&mut rt);
        let view = |rt: &Runtime<App>| rt.elements().into_iter().find(|e| e.key.as_deref() == Some("source")).unwrap().rect;
        let focused_caret = |rt: &Runtime<App>| rt.elements().into_iter().find_map(|e| e.caret.filter(|_| e.key.as_deref().is_some_and(|k| k.starts_with("src-"))));
        let piece = |rt: &Runtime<App>| match &rt.state.tab().unwrap().mode {
            Mode::Source(s) => s.pieces.focused().unwrap(),
            Mode::Read => panic!(),
        };
        // Ogni volta che il cursore passa a un altro pezzo (dopo i primi, sul bordo della vista).
        for k in [Key::Down, Key::Up] {
            for crossing in 0..12 {
                let was = piece(&rt);
                for _ in 0..400 {
                    rt.key(key(k.clone()));
                    rt.frame(1.0 / 60.0);
                    if piece(&rt) != was {
                        break;
                    }
                }
                assert_ne!(piece(&rt), was, "{k:?} passa al pezzo vicino");
                settle(&mut rt);
                let [_, top, _, h] = view(&rt);
                let c = focused_caret(&rt).expect("un campo ha il cursore");
                assert!(c[1] >= top - 0.5 && c[1] + c[3] <= top + h + 0.5, "{k:?} {crossing}: cursore {c:?} fuori da {top}..{}", top + h);
            }
        }
        let Mode::Source(s) = &rt.state.tab().unwrap().mode else { panic!() };
        assert!(s.pieces.focused().is_some());
        rt.key(typed("Z"));
        settle(&mut rt);
        let Mode::Source(s) = &rt.state.tab().unwrap().mode else { panic!() };
        assert_eq!(s.text().as_bytes()[s.pieces.caret() - 1], b'Z', "si scrive al cursore");
    }

    /// Due schede sullo stesso nome e testo (un README in un repo e nel suo worktree): scrivere
    /// nella seconda cambia solo la seconda.
    #[test]
    fn two_tabs_with_the_same_file_keep_their_own_pieces() {
        let dir = tempfile::tempdir().unwrap();
        let (a, b) = (dir.path().join("a/README.md"), dir.path().join("b/README.md"));
        for p in [&a, &b] {
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, "# uguale\n\ntesto\n").unwrap();
        }
        let mut rt = open(&a);
        rt.key(cmd('e'));
        settle(&mut rt);
        rt.update(|s, _| s.open_paths(vec![b.clone()]));
        settle(&mut rt);
        rt.key(cmd('e'));
        settle(&mut rt);
        rt.key(cmd('1'));
        settle(&mut rt);
        // Nessun campo a fuoco (un clic fuori): i due pezzi hanno lo stesso stato.
        rt.update(|s, _| {
            for t in &mut s.tabs {
                if let Mode::Source(src) = &mut t.mode {
                    src.pieces.current().focused = false;
                }
            }
        });
        settle(&mut rt);
        rt.key(cmd('2'));
        settle(&mut rt);
        let piece = rt.elements().into_iter().filter_map(|e| e.key).find(|k| k.starts_with("src-")).unwrap();
        tap(&mut rt, &piece);
        rt.key(KeyEvent { key: Key::Home, mods: Mods::default() });
        rt.key(typed("B"));
        settle(&mut rt);
        let text = |rt: &Runtime<App>, i: usize| match &rt.state.tabs[i].mode {
            Mode::Source(s) => s.text(),
            Mode::Read => panic!("non nel sorgente"),
        };
        assert_eq!(text(&rt, 0), "# uguale\n\ntesto\n", "a non cambia");
        assert_eq!(text(&rt, 1), "B# uguale\n\ntesto\n", "b ha il testo");
        rt.update(|s, _| s.save_all());
        assert_eq!(std::fs::read_to_string(&a).unwrap(), "# uguale\n\ntesto\n");
        assert_eq!(std::fs::read_to_string(&b).unwrap(), "B# uguale\n\ntesto\n");
    }

    /// "Ricarica" toglie il fuoco al campo (il clic è sul bottone) ma il cursore resta dov'era.
    #[test]
    fn reload_keeps_the_caret() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.md");
        std::fs::write(&path, "# a\n\nuno\n\ndue\n").unwrap();
        let mut rt = open(&path);
        rt.key(cmd('e'));
        settle(&mut rt);
        for k in [Key::Down, Key::Down, Key::Right, Key::Right] {
            rt.key(key(k));
            settle(&mut rt);
        }
        rt.key(typed("X"));
        settle(&mut rt);
        let before = caret(&rt);
        assert_eq!(before, 8);
        std::fs::write(&path, "# a\n\nuno\n\ndue, dell'agente\n").unwrap();
        rt.key(cmd('s'));
        settle(&mut rt);
        tap(&mut rt, RELOAD_KEY);
        assert_eq!(source_text(&rt).as_deref(), Some("# a\n\nuno\n\ndue, dell'agente\n"));
        assert_eq!(caret(&rt), before, "il cursore resta dov'era");
        // Anche con il cursore messo da un clic su un altro pezzo.
        let last = rt.elements().into_iter().filter_map(|e| e.key).rfind(|k| k.starts_with("src-")).unwrap();
        tap(&mut rt, &last);
        let clicked = caret(&rt);
        assert!(clicked > 5, "il clic è nell'ultimo pezzo: {clicked}");
        rt.key(typed("Y"));
        settle(&mut rt);
        std::fs::write(&path, "# a\n\nuno\n\ndue, dell'agente, ancora\n").unwrap();
        rt.key(cmd('s'));
        settle(&mut rt);
        tap(&mut rt, RELOAD_KEY);
        assert_eq!(caret(&rt), clicked + 1);
    }

    /// Backspace all'inizio di un pezzo e un carattere nello stesso giro di tasti (nessun frame
    /// in mezzo): il carattere va nel pezzo unito, non si perde.
    #[test]
    fn a_key_right_after_a_join_lands_in_the_joined_piece() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.md");
        std::fs::write(&path, "# a\n\nuno\n").unwrap();
        let mut rt = open(&path);
        rt.key(cmd('e'));
        settle(&mut rt);
        if let Mode::Source(s) = &mut rt.state.tabs[0].mode {
            s.pieces.set_caret(5);
        }
        rt.invalidate();
        settle(&mut rt);
        rt.key(key(Key::Backspace));
        rt.key(typed("Q"));
        settle(&mut rt);
        assert_eq!(source_text(&rt).as_deref(), Some("# a\nQuno\n"));
        // E dopo una freccia che passa al pezzo vicino, o Canc alla fine.
        let path = dir.path().join("b.md");
        std::fs::write(&path, "a\n\nb\n\nc\n").unwrap();
        let mut rt = open(&path);
        rt.key(cmd('e'));
        settle(&mut rt);
        assert_eq!(source_text(&rt).as_deref(), Some("a\n\nb\n\nc\n"));
        if let Mode::Source(s) = &mut rt.state.tabs[0].mode {
            s.pieces.set_caret(6);
        }
        rt.invalidate();
        settle(&mut rt);
        rt.key(key(Key::Up));
        rt.key(typed("W"));
        settle(&mut rt);
        assert_eq!(source_text(&rt).as_deref(), Some("a\n\nb\nW\nc\n"), "su: nel pezzo prima");
        rt.key(key(Key::Delete));
        rt.key(typed("V"));
        settle(&mut rt);
        assert_eq!(source_text(&rt).as_deref(), Some("a\n\nb\nWVc\n"), "Canc: nel pezzo unito");
    }

    /// Il sorgente virtualizzato (un campo per pezzo): riunire i pezzi ridà il file identico,
    /// byte per byte, e un salvataggio senza modifiche lo riscrive uguale.
    #[test]
    fn virtualized_roundtrip() {
        let big = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/big.md")).unwrap();
        let cases = [
            big.as_str(),
            "# a\n\nuno",
            "# a\n\n\n\nuno\n\n\n\n\ndue\n",
            "\n\n\n# a\n\nuno\n",
            "",
            "\n",
            "# a\r\n\r\nuno\r\n\r\n\r\ndue\r\n",
            "```\n\n\n```\n\n~~~\nx\n\n",
        ];
        for text in cases {
            let pieces = crate::edit::blocks::Pieces::new(text);
            assert_eq!(pieces.text(), text, "{:?}", &text[..text.len().min(40)]);
        }
        let pieces = crate::edit::blocks::Pieces::new(&big);
        assert!(pieces.len() > 100, "big.md in pezzi: {}", pieces.len());
        assert!(pieces.iter().all(|p| p.edit.text.len() <= 2 * crate::edit::blocks::MAX_PIECE));
        // Dal file al disco: entrare, "Tieni il mio" (scrive senza confrontare), stessi byte.
        let dir = tempfile::tempdir().unwrap();
        for (i, text) in cases.iter().enumerate() {
            let path = dir.path().join(format!("{i}.md"));
            std::fs::write(&path, text).unwrap();
            let mut live = crate::live::Live::default();
            let mut tab = Tab::new(crate::doc::document::Document::load(&path).unwrap(), live.follow(&path));
            tab.mode = Mode::Source(Box::new(crate::edit::enter(&tab.doc)));
            if let Mode::Source(s) = &tab.mode {
                assert_eq!(s.text(), *text);
            }
            crate::edit::keep_mine(&mut tab, &mut crate::live::reload::OwnWrites::default()).unwrap();
            assert_eq!(std::fs::read(&path).unwrap(), text.as_bytes(), "{i}.md");
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
