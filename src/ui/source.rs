//! The source view (Cmd+E): the whole file in the document column, one multiline `text_input`
//! per piece ([`crate::edit::blocks`]) inside `virtual_rows`, so Rune lays out and
//! colors only the mounted rows. Markdown colors come from rune-highlights (via
//! `rune_kit::syntax`) on the theme greys: no accent (DESIGN.md). Each piece is colored on its
//! own, so the whole file has colors (not just the first 128 KiB).
//!
//! The source behaves as a single field: Return breaks the line in the piece (a multiline
//! `text_input` would use it to submit); Backspace at the start and Delete at the end join two pieces;
//! arrows at the edges move to the next piece (up and down from the first and last drawn line,
//! which the view reads from `Cx::caret`: [`caret_lines`]). A selection stays inside one piece.
//! Cmd+S and Cmd+E live in `main::on_key`: the field does not take them.
//!
//! When a save does not write, a row as tall as the tab bar appears above the source,
//! with a line below: "The file changed on disk" with "Reload" and "Keep mine"
//! (pressable rows like the tabs, text `fg2`, hover `sel`), or "Save failed" on
//! its own. UI text in `fg`, no accent.
//!
//! Cmd+W or a middle click on a tab with unsaved changes: in place of that row, with
//! the same look, "Save changes to <name>?" with "Save" (also Return), "Don't save" and
//! "Cancel" (also Esc). The field keeps focus ([`App::answer_close`]).

use std::borrow::Cow;

use rune::{Cx, El, Family, GrowthSpec, Key, KeyEvent, ScrollSpec, Span, TextStyle, Tx, col, el, memo, row, text, text_input};
use rune_kit::syntax::{self, Syntax, SyntaxTheme};

use super::doc_view::{BOTTOM, TOP};
use super::sidebar::{self, Hooks, Motion};
use super::theme::{CODE_TEXT, HAIRLINE, ROW, ROW_PAD, ROW_PAD_X, TAB_BAR_H, TAB_GAP, Theme, UI_STRONG_WEIGHT};
use crate::app::{App, CloseAnswer, Mode, Tab};
use crate::edit::blocks::{Edge, Pieces};
use crate::edit::{Notice, SourceEdit};

/// The row texts (spec, "Editing"; "Save failed": the controller's choice).
pub const CONFLICT_TEXT: &str = "The file changed on disk";
pub const RELOAD_TEXT: &str = "Reload";
pub const KEEP_TEXT: &str = "Keep mine";
pub const FAILED_TEXT: &str = "Save failed";
/// Cmd+E on a file that is not UTF-8.
pub const NOT_UTF8_TEXT: &str = "This file is not UTF-8: editing is off";
/// The keys of the row and its two buttons.
pub const NOTICE_KEY: &str = "source-notice";
/// The row above the reading view when Cmd+E does not enter because the file is not UTF-8.
pub const NOT_UTF8_KEY: &str = "read-not-utf8";
pub const RELOAD_KEY: &str = "source-reload";
pub const KEEP_KEY: &str = "source-keep";
/// The buttons of the row that asks on close (record `2026-11-14-cmdw-unsaved-changes`).
pub const SAVE_TEXT: &str = "Save";
pub const DISCARD_TEXT: &str = "Don't save";
pub const CANCEL_TEXT: &str = "Cancel";
/// The keys of the asking row and its buttons. Only the front tab shows it: like
/// the conflict row's keys they do not carry the tab id, so a script can find them.
pub const ASK_KEY: &str = "source-ask";
pub const SAVE_KEY: &str = "source-ask-save";
pub const DISCARD_KEY: &str = "source-ask-discard";
pub const CANCEL_KEY: &str = "source-ask-cancel";

/// The row's question on close, with the file name.
pub fn ask_text(name: &str) -> String {
    format!("Save changes to {name}?")
}

/// Source text: `typography.doc-code` (mono 13.5).
pub fn font() -> TextStyle {
    TextStyle::new(CODE_TEXT, 400.0).family(Family::Mono)
}

/// Markdown token colors, all from the theme greys: headings in `fg` and heavier, links in
/// `fg`, code in `fg2`, marks (`#`, `-`, `>`, fence, brackets) and link destinations in `fg3`;
/// the rest in the body color.
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

/// The color spans for `text` (one piece). `rune_kit::syntax` caches the last
/// result and colors the first 128 KiB: a piece is much shorter.
pub fn spans(text: &str, theme: &SyntaxTheme) -> Vec<Span> {
    theme.spans(&syntax::highlight(Some("markdown"), text), 0)
}

/// Where the cursor is in piece `piece`, which has focus: on the first and/or the last drawn
/// line. Decides whether arrow up and down move to the next piece.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CaretLines {
    pub piece: u64,
    pub first: bool,
    pub last: bool,
}

/// The field key of piece `piece` of tab `id`.
pub fn input_key(id: u64, piece: u64) -> String {
    format!("src-{id:x}-{piece:x}")
}

/// The row key of the piece: tab id and piece id (stable while typing).
fn row_key(id: u64, piece: u64) -> String {
    format!("{id:x}-p{piece:x}")
}

/// [`CaretLines`] of the focused piece, from the cursor and the field as drawn (window
/// points, after scrolling). Before the first layout: from the text lines.
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

/// The edge that key `ev` touches, if it is one of those that move from one piece to another.
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

/// With the close question row open, Return answers "Save" and Esc "Cancel", before
/// the field (which would break the line). True if it took the key.
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

/// A piece's row: its field, with the colors, in the column.
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

/// What it takes to draw a piece, copyable into the row closures.
#[derive(Clone, Hash)]
struct Style {
    syntax: SyntaxTheme,
    body: rune::Color,
    caret: rune::Color,
    sel: rune::Color,
    label: std::rc::Rc<str>,
}

/// The estimated height of a piece not measured yet: a few `doc-code` lines.
const PIECE_ESTIMATE: f32 = 80.0;

/// The source view of tab `tab`, being edited with `src`, with the row above if
/// the last save did not write. `caret`: [`caret_lines`]. `hover`: the pointer is a fine one
/// and can hover.
#[allow(clippy::too_many_arguments)]
pub fn view(s: &App, hooks: Hooks<App>, tab: &Tab, src: &SourceEdit, caret: Option<CaretLines>, theme: &Theme, reduced: bool, hover: bool) -> El {
    let id = tab.doc.id;
    let style = Style { syntax: syntax_theme(theme), body: theme.body, caret: theme.fg, sel: theme.sel, label: tab.title().into() };
    let rows = super::theme::column(col().key("source-rows").w_full(), s.settings.column)
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

/// The conflict or error row (see [`alert_row`]).
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
    alert_row(NOTICE_KEY, message, buttons, theme, s.settings.column)
}

/// The row above the reading view after a Cmd+E on a file that is not UTF-8 (like the
/// conflict one, without buttons).
pub fn not_utf8_row(theme: &Theme, column: f32) -> El {
    alert_row(NOT_UTF8_KEY, NOT_UTF8_TEXT, Vec::new(), theme, column)
}

/// The row that asks on close with unsaved changes. "Save" is the window's default
/// button: Return presses it even when the field does not have focus.
fn ask_row(s: &App, hooks: Hooks<App>, id: u64, name: &str, theme: &Theme, motion: Motion) -> El {
    let answer = |a: CloseAnswer| move |s: &mut App, tx: &mut Tx| s.answer_close_tx(id, a, tx);
    let buttons = vec![
        button(s, hooks, SAVE_KEY, SAVE_TEXT, theme, motion, answer(CloseAnswer::Save)).default_button(),
        button(s, hooks, DISCARD_KEY, DISCARD_TEXT, theme, motion, answer(CloseAnswer::Discard)),
        button(s, hooks, CANCEL_KEY, CANCEL_TEXT, theme, motion, answer(CloseAnswer::Cancel)),
    ];
    alert_row(ASK_KEY, &ask_text(name), buttons, theme, s.settings.column)
}

/// A row above the source, as tall as the tab bar and announced as `alert`: the
/// text aligned with the source text, the buttons on the right with their text aligned to the
/// column's right edge; a 0.5 line in `line` below.
fn alert_row(key: &str, message: &str, buttons: Vec<El>, theme: &Theme, column: f32) -> El {
    let line = super::theme::column(row().w_full(), column)
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

/// A row button: like a background tab (`ROW` tall, `ROW_PAD` on the sides, text
/// `fg2`, hover `sel`, scale 0.98 when pressed).
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
        panic!("the view does not settle");
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

    /// Cmd+E opens the source with focus in the field; you type, Return breaks the line, Cmd+E saves and
    /// goes back to reading with the saved text.
    #[test]
    fn cmd_e_edits_and_saves() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.md");
        std::fs::write(&path, "# a\n\none\n").unwrap();
        let mut rt = open(&path);
        rt.key(cmd('e'));
        settle(&mut rt);
        assert_eq!(source_text(&rt).as_deref(), Some("# a\n\none\n"));
        assert!(rt.elements().iter().any(|e| e.key.as_deref().is_some_and(|k| k.starts_with("src-"))), "the field is in the view");
        rt.key(typed("X"));
        rt.key(KeyEvent { key: Key::Enter, mods: Mods::default() });
        settle(&mut rt);
        assert_eq!(source_text(&rt).as_deref(), Some("X\n# a\n\none\n"), "types at the cursor, Return breaks the line");
        rt.key(cmd('e'));
        settle(&mut rt);
        assert!(source_text(&rt).as_deref().is_none(), "back in reading");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "X\n# a\n\none\n");
        assert_eq!(rt.state.active_document().blocks.len(), 3, "reading shows the saved text (X, heading, one)");
    }

    fn texts(rt: &Runtime<App>) -> Vec<String> {
        rt.elements().into_iter().filter_map(|e| e.text).collect()
    }

    fn has(rt: &Runtime<App>, key: &str) -> bool {
        rt.elements().iter().any(|e| e.key.as_deref() == Some(key))
    }

    fn tap(rt: &mut Runtime<App>, key: &str) {
        let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is missing"));
        rt.pointer_move(x, y);
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        settle(rt);
    }

    /// Cmd+E, you type, the file changes on disk, Cmd+S: the row appears and nothing is written.
    /// "Keep mine" writes the local text and the row disappears.
    #[test]
    fn conflict_row_and_keep_mine() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.md");
        std::fs::write(&path, "# a\n\none\n").unwrap();
        let mut rt = open(&path);
        rt.key(cmd('e'));
        rt.key(typed("X"));
        settle(&mut rt);
        assert!(!has(&rt, NOTICE_KEY), "no row before the conflict");
        std::fs::write(&path, "# a\n\nfrom the agent\n").unwrap();
        rt.key(cmd('s'));
        settle(&mut rt);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "# a\n\nfrom the agent\n", "does not write");
        assert!(has(&rt, NOTICE_KEY), "the row appears");
        let t = texts(&rt);
        for want in [CONFLICT_TEXT, RELOAD_TEXT, KEEP_TEXT] {
            assert!(t.iter().any(|x| x == want), "{want} in {t:?}");
        }
        // With the row above, the field keeps focus: you keep typing.
        rt.key(typed("Y"));
        settle(&mut rt);
        assert_eq!(source_text(&rt).as_deref(), Some("XY# a\n\none\n"));
        tap(&mut rt, KEEP_KEY);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "XY# a\n\none\n", "the disk has the local text");
        assert!(!has(&rt, NOTICE_KEY), "the row disappears");
        assert_eq!(source_text(&rt).as_deref(), Some("XY# a\n\none\n"), "stays in the source");
    }

    /// "Reload" discards the changes: the source shows the agent's text, the disk does not change.
    #[test]
    fn conflict_reload_discards() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.md");
        std::fs::write(&path, "# a\n\none\n").unwrap();
        let mut rt = open(&path);
        rt.key(cmd('e'));
        rt.key(typed("X"));
        settle(&mut rt);
        let agent = "# a\n\nfrom the agent\n";
        std::fs::write(&path, agent).unwrap();
        rt.key(cmd('e'));
        settle(&mut rt);
        assert!(has(&rt, NOTICE_KEY), "Cmd+E does not leave: row");
        tap(&mut rt, RELOAD_KEY);
        assert_eq!(source_text(&rt).as_deref(), Some(agent));
        assert!(!has(&rt, NOTICE_KEY));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), agent);
        rt.key(cmd('e'));
        settle(&mut rt);
        assert!(source_text(&rt).as_deref().is_none(), "without changes Cmd+E goes back to reading");
    }

    fn esc() -> KeyEvent {
        KeyEvent { key: Key::Escape, mods: Mods::default() }
    }

    fn element(rt: &Runtime<App>, key: &str) -> rune::ElementInfo {
        rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key)).unwrap_or_else(|| panic!("{key} is missing"))
    }

    /// In the source with changes: Cmd+E, type "X" at the start of a.md.
    fn edited(path: &Path) -> Runtime<App> {
        std::fs::write(path, "# a\n\none\n").unwrap();
        let mut rt = open(path);
        rt.key(cmd('e'));
        rt.key(typed("X"));
        settle(&mut rt);
        rt
    }

    /// Without changes Cmd+W closes at once, in the source too.
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

    /// Cmd+W with changes: the row asks, announced like the conflict one, with three buttons
    /// with an accessible name. A second Cmd+W does not close. Esc removes it: changes and cursor stay
    /// and the field still has focus.
    #[test]
    fn cmd_w_with_edits_asks_and_esc_cancels() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.md");
        let mut rt = edited(&path);
        rt.key(cmd('w'));
        settle(&mut rt);
        assert!(has(&rt, ASK_KEY), "the row appears");
        assert!(!has(&rt, NOTICE_KEY));
        let question = ask_text("a.md");
        assert_eq!(question, "Save changes to a.md?");
        let t = texts(&rt);
        for want in [question.as_str(), SAVE_TEXT, DISCARD_TEXT, CANCEL_TEXT] {
            assert!(t.iter().any(|x| x == want), "{want} in {t:?}");
        }
        let row = element(&rt, ASK_KEY);
        assert_eq!((row.role, row.label.as_deref()), ("alert", Some(question.as_str())), "announced like the conflict");
        for (key, label) in [(SAVE_KEY, SAVE_TEXT), (DISCARD_KEY, DISCARD_TEXT), (CANCEL_KEY, CANCEL_TEXT)] {
            let b = element(&rt, key);
            assert_eq!((b.role, b.label.as_deref()), ("button", Some(label)));
        }
        rt.key(cmd('w'));
        settle(&mut rt);
        assert_eq!(rt.state.tabs.len(), 1, "the second Cmd+W does not close");
        assert!(has(&rt, ASK_KEY), "the row stays");
        rt.key(esc());
        settle(&mut rt);
        assert!(!has(&rt, ASK_KEY), "Esc removes the row");
        assert_eq!(source_text(&rt).as_deref(), Some("X# a\n\none\n"), "changes intact");
        assert_eq!(caret(&rt), 1, "cursor intact");
        rt.key(typed("Y"));
        settle(&mut rt);
        assert_eq!(source_text(&rt).as_deref(), Some("XY# a\n\none\n"), "the field still has focus");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "# a\n\none\n", "nothing written");
    }

    /// Return answers "Save": it writes and closes.
    #[test]
    fn enter_saves_and_closes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.md");
        let mut rt = edited(&path);
        rt.key(cmd('w'));
        settle(&mut rt);
        rt.key(key(Key::Enter));
        settle(&mut rt);
        assert!(rt.state.tabs.is_empty(), "closed");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "X# a\n\none\n");
    }

    /// "Save" with the file changed on disk: the tab stays, the row becomes the conflict one.
    #[test]
    fn save_with_a_conflict_shows_the_conflict_row() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.md");
        let mut rt = edited(&path);
        rt.key(cmd('w'));
        settle(&mut rt);
        std::fs::write(&path, "# a\n\nfrom the agent\n").unwrap();
        tap(&mut rt, SAVE_KEY);
        assert_eq!(rt.state.tabs.len(), 1, "not closed");
        assert!(!has(&rt, ASK_KEY));
        assert!(texts(&rt).iter().any(|x| x == CONFLICT_TEXT), "the conflict row");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "# a\n\nfrom the agent\n");
    }

    /// "Don't save": closes, the file on disk does not change.
    #[test]
    fn discard_closes_without_writing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.md");
        let mut rt = edited(&path);
        rt.key(cmd('w'));
        settle(&mut rt);
        tap(&mut rt, DISCARD_KEY);
        assert!(rt.state.tabs.is_empty(), "closed");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "# a\n\none\n", "the file does not change");
    }

    /// "Cancel": the row disappears, changes and cursor stay.
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
        assert_eq!(source_text(&rt).as_deref(), Some("X# a\n\none\n"));
        assert_eq!(caret(&rt), 1);
    }

    /// The opacity of what enters (`enter`) and of what exits (`exit`, if still there) after Cmd+E,
    /// frame by frame at 120 Hz: (ms, enter, exit).
    fn fade(rt: &mut Runtime<App>, enter: &str, exit: &str) -> Vec<(f32, f32, Option<f32>)> {
        let op = |rt: &Runtime<App>, k: &str| rt.elements().into_iter().find(|e| e.key.as_deref() == Some(k)).map(|e| e.opacity);
        rt.key(cmd('e'));
        (1..=60).map(|f| {
            rt.frame(1.0 / 120.0);
            (f as f32 * 1000.0 / 120.0, op(rt, enter).unwrap_or(0.0), op(rt, exit))
        }).collect()
    }

    /// Faster Cmd+E (v1.1): the source enters to 95 % within 175 ms and reading exits within
    /// 150; on the way back reading enters to 95 % within 175 ms. The fade shows (it is not a
    /// cut). With "reduce motion" it is immediate.
    #[test]
    fn cmd_e_cross_fades_fast() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.md");
        std::fs::write(&path, "# a\n\none\n").unwrap();
        let mut rt = open(&path);
        let id = rt.state.tab().unwrap().doc.id;
        let src = format!("source-{id:x}");
        let check = |what: &str, frames: &[(f32, f32, Option<f32>)]| {
            eprintln!("{what}: {:?}", frames.iter().step_by(3).collect::<Vec<_>>());
            let at = |ms: f32| frames.iter().find(|f| f.0 >= ms).unwrap();
            assert!(at(25.0).1 < 0.9, "{what}: the fade shows");
            assert!(at(175.0).1 >= 0.95, "{what}: at 95 % within 175 ms: {:?}", at(175.0));
            assert!(at(150.0).2.is_none_or(|o| o <= 0.05), "{what}: the other one out within 150 ms: {:?}", at(150.0));
        };
        let enter = fade(&mut rt, &src, "doc-pane");
        check("source", &enter);
        settle(&mut rt);
        let back = fade(&mut rt, "doc-pane", &src);
        check("reading", &back);
        settle(&mut rt);
        rt.update(|s, _| s.motion_env = Some(true));
        settle(&mut rt);
        let reduced = fade(&mut rt, &src, "doc-pane");
        assert!(reduced[0].1 >= 0.99 && reduced[0].2.is_none_or(|o| o <= 0.01), "reduce motion: at once {:?}", reduced[0]);
    }

    /// The × on a background tab with changes asks like Cmd+W: it comes to the front with the
    /// row and nothing is written.
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
        assert_eq!((rt.state.tabs.len(), rt.state.active), (2, 0), "a in front, open");
        assert!(has(&rt, ASK_KEY), "the row");
        assert_eq!(std::fs::read_to_string(&a).unwrap(), "# a\n\none\n");
    }

    /// A middle click on a background tab with changes: it comes to the front with the row.
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
        assert_eq!((rt.state.tabs.len(), rt.state.active), (2, 0), "a in front, open");
        assert!(has(&rt, ASK_KEY), "the row");
        assert_eq!(std::fs::read_to_string(&a).unwrap(), "# a\n\none\n");
    }

    /// A file that is not UTF-8 does not enter the source (it would rewrite it with U+FFFD): Cmd+E
    /// stays in reading and a row says so. The file does not change.
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
        assert!(source_text(&rt).is_none(), "stays in reading");
        assert!(has(&rt, NOT_UTF8_KEY), "the row");
        assert!(texts(&rt).iter().any(|t| t == NOT_UTF8_TEXT));
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
    }

    /// A write error: the row says "Save failed", without the two buttons.
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
        assert_eq!(source_text(&rt).as_deref(), Some("X# a\n"), "the changes stay");
    }

    /// Cost per frame typing in the source: a key in the middle of the file and the frame that shows it,
    /// headless (layout and scene, no GPU). Spec threshold on `fixtures/big.md` (1 MB): p95
    /// ≤ 8 ms; `normal.md` (21 KB) for comparison.
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
            for c in "A sentence typed in the middle of the file, to measure the cost of each key. ".chars().cycle().take(200) {
                let t = Instant::now();
                rt.key(typed(&c.to_string()));
                rt.frame(1.0 / 60.0);
                ms.push(t.elapsed().as_secs_f64() * 1000.0);
            }
            let text = source_text(&rt).unwrap();
            let at = text.find("to measure the cost of each key. A sentence").expect("the typed text is in the source");
            assert!(at > text.len() / 3, "typed in the middle of the file");
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
            Mode::Read => panic!("not in the source"),
        }
    }

    /// The source in pieces is used as a single field: arrows move from one piece to another,
    /// Return breaks the line in the piece, Backspace at the start and Delete at the end remove the `\n` between two
    /// pieces; what you type goes into the piece where the cursor is.
    #[test]
    fn pieces_edit_like_one_field() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.md");
        std::fs::write(&path, "# a\n\none\n\ntwo\n").unwrap();
        let mut rt = open(&path);
        rt.key(cmd('e'));
        settle(&mut rt);
        // Down from the first line of the first piece ("# a\n"): its last line is the empty one.
        rt.key(key(Key::Down));
        settle(&mut rt);
        assert_eq!(caret(&rt), 4, "empty line of the first piece");
        rt.key(key(Key::Down));
        settle(&mut rt);
        assert_eq!(caret(&rt), 5, "in the second piece");
        rt.key(typed("X"));
        settle(&mut rt);
        assert_eq!(source_text(&rt).as_deref(), Some("# a\n\nXone\n\ntwo\n"), "types in the second piece");
        rt.key(key(Key::Up));
        settle(&mut rt);
        assert_eq!(caret(&rt), 4, "up: back in the first piece");
        rt.key(key(Key::Right));
        rt.key(key(Key::Right));
        settle(&mut rt);
        assert_eq!(caret(&rt), 6, "right: after the X");
        rt.key(key(Key::Left));
        settle(&mut rt);
        rt.key(key(Key::Backspace));
        settle(&mut rt);
        assert_eq!(source_text(&rt).as_deref(), Some("# a\nXone\n\ntwo\n"), "Backspace at the start joins with the piece before");
        assert_eq!(caret(&rt), 4);
        rt.key(key(Key::Enter));
        settle(&mut rt);
        assert_eq!(source_text(&rt).as_deref(), Some("# a\n\nXone\n\ntwo\n"), "Return breaks the line");
        // Delete at the end of the first piece ("# a\n\nXone\n"): removes the \n before "two".
        if let Mode::Source(s) = &mut rt.state.tabs[0].mode {
            s.pieces.set_caret(10);
        }
        rt.invalidate();
        settle(&mut rt);
        rt.key(key(Key::Delete));
        settle(&mut rt);
        assert_eq!(source_text(&rt).as_deref(), Some("# a\n\nXone\ntwo\n"), "Delete at the end joins the piece after");
        rt.key(cmd('e'));
        settle(&mut rt);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "# a\n\nXone\ntwo\n");
    }

    /// Arrows cross many pieces of `big.md`: the cursor stays in the view (the scroll
    /// follows it) and focus is in the cursor's piece.
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
        // Each time the cursor moves to another piece (after the first ones, at the edge of the view).
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
                assert_ne!(piece(&rt), was, "{k:?} moves to the next piece");
                settle(&mut rt);
                let [_, top, _, h] = view(&rt);
                let c = focused_caret(&rt).expect("a field has the cursor");
                assert!(c[1] >= top - 0.5 && c[1] + c[3] <= top + h + 0.5, "{k:?} {crossing}: cursor {c:?} outside {top}..{}", top + h);
            }
        }
        let Mode::Source(s) = &rt.state.tab().unwrap().mode else { panic!() };
        assert!(s.pieces.focused().is_some());
        rt.key(typed("Z"));
        settle(&mut rt);
        let Mode::Source(s) = &rt.state.tab().unwrap().mode else { panic!() };
        assert_eq!(s.text().as_bytes()[s.pieces.caret() - 1], b'Z', "types at the cursor");
    }

    /// Two tabs with the same name and text (a README in a repo and in its worktree): typing
    /// in the second changes only the second.
    #[test]
    fn two_tabs_with_the_same_file_keep_their_own_pieces() {
        let dir = tempfile::tempdir().unwrap();
        let (a, b) = (dir.path().join("a/README.md"), dir.path().join("b/README.md"));
        for p in [&a, &b] {
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, "# same\n\ntext\n").unwrap();
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
        // No focused field (a click outside): the two pieces have the same state.
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
            Mode::Read => panic!("not in the source"),
        };
        assert_eq!(text(&rt, 0), "# same\n\ntext\n", "a does not change");
        assert_eq!(text(&rt, 1), "B# same\n\ntext\n", "b has the text");
        rt.update(|s, _| s.save_all());
        assert_eq!(std::fs::read_to_string(&a).unwrap(), "# same\n\ntext\n");
        assert_eq!(std::fs::read_to_string(&b).unwrap(), "B# same\n\ntext\n");
    }

    /// "Reload" takes focus away from the field (the click is on the button) but the cursor stays where it was.
    #[test]
    fn reload_keeps_the_caret() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.md");
        std::fs::write(&path, "# a\n\none\n\ntwo\n").unwrap();
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
        std::fs::write(&path, "# a\n\none\n\ntwo, from the agent\n").unwrap();
        rt.key(cmd('s'));
        settle(&mut rt);
        tap(&mut rt, RELOAD_KEY);
        assert_eq!(source_text(&rt).as_deref(), Some("# a\n\none\n\ntwo, from the agent\n"));
        assert_eq!(caret(&rt), before, "the cursor stays where it was");
        // Also with the cursor placed by a click on another piece.
        let last = rt.elements().into_iter().filter_map(|e| e.key).rfind(|k| k.starts_with("src-")).unwrap();
        tap(&mut rt, &last);
        let clicked = caret(&rt);
        assert!(clicked > 5, "the click is in the last piece: {clicked}");
        rt.key(typed("Y"));
        settle(&mut rt);
        std::fs::write(&path, "# a\n\none\n\ntwo, from the agent, still\n").unwrap();
        rt.key(cmd('s'));
        settle(&mut rt);
        tap(&mut rt, RELOAD_KEY);
        assert_eq!(caret(&rt), clicked + 1);
    }

    /// Backspace at the start of a piece and a character in the same round of keys (no frame
    /// in between): the character goes into the joined piece, it is not lost.
    #[test]
    fn a_key_right_after_a_join_lands_in_the_joined_piece() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.md");
        std::fs::write(&path, "# a\n\none\n").unwrap();
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
        assert_eq!(source_text(&rt).as_deref(), Some("# a\nQone\n"));
        // And after an arrow that moves to the next piece, or Delete at the end.
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
        assert_eq!(source_text(&rt).as_deref(), Some("a\n\nb\nW\nc\n"), "up: in the piece before");
        rt.key(key(Key::Delete));
        rt.key(typed("V"));
        settle(&mut rt);
        assert_eq!(source_text(&rt).as_deref(), Some("a\n\nb\nWVc\n"), "Delete: in the joined piece");
    }

    /// The virtualized source (one field per piece): joining the pieces gives back the identical file,
    /// byte for byte, and a save without changes rewrites it the same.
    #[test]
    fn virtualized_roundtrip() {
        let big = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/big.md")).unwrap();
        let cases = [
            big.as_str(),
            "# a\n\none",
            "# a\n\n\n\none\n\n\n\n\ntwo\n",
            "\n\n\n# a\n\none\n",
            "",
            "\n",
            "# a\r\n\r\none\r\n\r\n\r\ntwo\r\n",
            "```\n\n\n```\n\n~~~\nx\n\n",
        ];
        for text in cases {
            let pieces = crate::edit::blocks::Pieces::new(text);
            assert_eq!(pieces.text(), text, "{:?}", &text[..text.len().min(40)]);
        }
        let pieces = crate::edit::blocks::Pieces::new(&big);
        assert!(pieces.len() > 100, "big.md in pieces: {}", pieces.len());
        assert!(pieces.iter().all(|p| p.edit.text.len() <= 2 * crate::edit::blocks::MAX_PIECE));
        // From the file to the disk: enter, "Keep mine" (writes without comparing), same bytes.
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

    /// No accent: every span color is a theme grey.
    #[test]
    fn markdown_colors_are_theme_greys() {
        for dark in [true, false] {
            let theme = Theme::for_mode(dark);
            let src = "# Title\n\nText with `code` and [link](https://x.y).\n\n- one\n- two\n\n```rs\nfn a() {}\n```\n";
            let spans = spans(src, &syntax_theme(&theme));
            assert!(!spans.is_empty(), "the Markdown is colored");
            let greys = [theme.fg, theme.fg2, theme.fg3, theme.body];
            for s in &spans {
                assert!(s.color.is_none_or(|c| greys.contains(&c)), "{:?} in {:?}", s.color, &src[s.start as usize..s.end as usize]);
            }
            let title = spans.iter().find(|s| s.start == 0).expect("the heading");
            assert_eq!((title.color, title.weight), (Some(theme.fg), Some(UI_STRONG_WEIGHT)));
        }
    }
}
