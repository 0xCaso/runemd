//! The source split into pieces, one `TextEdit` per piece: a single field of
//! 1 MB costs Rune ~150 ms per key, one field per piece inside `virtual_rows` costs only the
//! mounted rows.
//!
//! The pieces are the file text cut at some `\n`, which are removed: joined with `"\n"`
//! they give back the identical file, byte for byte (extra blank lines, file without a final `\n`, CRLF,
//! blank lines at the top, empty file). It cuts before a full line that follows a blank line,
//! outside fenced code blocks; a piece longer than [`MAX_PIECE`] is also cut at
//! any line (inside a fence only beyond [`MAX_FENCED_PIECE`]).
//!
//! The key at the edges of a piece does what it would do in a single field: Backspace at the start
//! removes the `\n` that separates it from the previous one (the two join), Delete at the end does the same with
//! the next one, the arrows move to the neighbouring piece.

use rune::TextEdit;

/// Beyond this length a piece is cut at the first line (outside fences).
pub const MAX_PIECE: usize = 4 * 1024;
/// Inside a fenced code block it cuts only beyond this length: the colors of the
/// code that follows are the Markdown ones, but a fence opened and never closed does not make the rest
/// of the file a single field.
pub const MAX_FENCED_PIECE: usize = 64 * 1024;

/// The text split into pieces: `split(t).join("\n") == t`.
pub fn split(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let (mut start, mut line_start) = (0, 0);
    let mut fence: Option<(u8, usize)> = None;
    let mut prev_blank = false;
    loop {
        let line_end = text[line_start..].find('\n').map_or(text.len(), |i| line_start + i);
        let line = &text[line_start..line_end];
        let content = line.strip_suffix('\r').unwrap_or(line);
        let blank = content.trim().is_empty();
        if line_start > start {
            let long = line_start - start > if fence.is_some() { MAX_FENCED_PIECE } else { MAX_PIECE };
            if long || (fence.is_none() && prev_blank && !blank) {
                out.push(&text[start..line_start - 1]);
                start = line_start;
            }
        }
        fence = fence_after(fence, content);
        prev_blank = blank;
        if line_end == text.len() {
            break;
        }
        line_start = line_end + 1;
    }
    out.push(&text[start..]);
    out
}

/// The fence open after line `line`: ``` or ~~~ (three or more, at most three spaces before) open
/// it, a line of only the same marks and at least as many closes it.
fn fence_after(open: Option<(u8, usize)>, line: &str) -> Option<(u8, usize)> {
    let indent = line.len() - line.trim_start_matches(' ').len();
    if indent > 3 {
        return open;
    }
    let rest = &line[indent..];
    let ch = match rest.as_bytes().first() {
        Some(&c @ (b'`' | b'~')) => c,
        _ => return open,
    };
    let n = rest.bytes().take_while(|&b| b == ch).count();
    match open {
        None if n >= 3 => Some((ch, n)),
        Some((c, m)) if c == ch && n >= m && rest[n..].trim().is_empty() => None,
        _ => open,
    }
}

/// A piece of the source: its id (row key, stable while typing) and its field.
#[derive(Debug)]
pub struct Piece {
    pub id: u64,
    pub edit: TextEdit,
}

/// The pieces of the source being edited; one has focus (or none, if the field lost it).
#[derive(Debug, Default)]
pub struct Pieces {
    pieces: Vec<Piece>,
    next_id: u64,
    /// Changes when pieces join or split again: the row keys change.
    pub version: u64,
    /// The last piece that had focus or an edit: its cursor holds even when
    /// focus is elsewhere (a click on "Reload").
    last: u64,
}

/// What a key does at the edges of a piece.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edge {
    /// Backspace at the start: joins the piece to the previous one.
    JoinBack,
    /// Delete at the end: joins the next one.
    JoinForward,
    /// Up arrow (first line) or left (start): to the piece before.
    Up,
    Left,
    /// Down arrow (last line) or right (end): to the piece after.
    Down,
    Right,
}

impl Pieces {
    /// The pieces of `text`, with the cursor at the start and focus in the first.
    pub fn new(text: &str) -> Pieces {
        let mut p = Pieces::default();
        p.set_text(text);
        p
    }

    /// Splits everything again from `text`: new ids, history lost, cursor at the start and focus in the first.
    pub fn set_text(&mut self, text: &str) {
        self.pieces = split(text)
            .into_iter()
            .map(|t| {
                self.next_id += 1;
                Piece { id: self.next_id, edit: TextEdit::new(t) }
            })
            .collect();
        self.version += 1;
        self.set_caret(0);
    }

    /// The whole text: the pieces joined with `"\n"`.
    pub fn text(&self) -> String {
        let len = self.pieces.iter().map(|p| p.edit.text.len() + 1).sum::<usize>();
        let mut out = String::with_capacity(len);
        for (i, p) in self.pieces.iter().enumerate() {
            if i > 0 {
                out.push('\n');
            }
            out.push_str(&p.edit.text);
        }
        out
    }

    pub fn len(&self) -> usize {
        self.pieces.len()
    }

    pub fn get(&self, i: usize) -> Option<&Piece> {
        self.pieces.get(i)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Piece> {
        self.pieces.iter()
    }

    /// The field of piece `id`, if it still exists (`on_edit`). Rune goes through here for each key, click
    /// and focus loss of the field: if the piece has focus it becomes the last used (Rune
    /// also asks for pieces without focus, which do not count).
    pub fn edit_mut(&mut self, id: u64) -> Option<&mut TextEdit> {
        let p = self.pieces.iter_mut().find(|p| p.id == id)?;
        if p.edit.focused {
            self.last = id;
        }
        Some(&mut p.edit)
    }

    fn index(&self, id: u64) -> Option<usize> {
        self.pieces.iter().position(|p| p.id == id)
    }

    /// The index of the piece with focus.
    pub fn focused(&self) -> Option<usize> {
        self.pieces.iter().position(|p| p.edit.focused)
    }

    /// The field with focus (the first if none has it).
    pub fn current(&mut self) -> &mut TextEdit {
        let i = self.focused().unwrap_or(0);
        &mut self.pieces[i].edit
    }

    /// The cursor in the whole text (bytes): in the piece with focus, or in the last one that had it.
    pub fn caret(&self) -> usize {
        let Some(i) = self.focused().or_else(|| self.index(self.last)) else { return 0 };
        self.pieces[..i].iter().map(|p| p.edit.text.len() + 1).sum::<usize>() + self.pieces[i].edit.focus
    }

    /// Puts the cursor at `at` of the whole text (a `\n` between two pieces is the end of the first) and
    /// focus in its piece.
    pub fn set_caret(&mut self, at: usize) {
        let mut acc = 0;
        let last = self.pieces.len() - 1;
        let mut target = (last, self.pieces[last].edit.text.len());
        for (i, p) in self.pieces.iter().enumerate() {
            if at <= acc + p.edit.text.len() {
                target = (i, at - acc);
                break;
            }
            acc += p.edit.text.len() + 1;
        }
        self.focus(target.0, target.1);
    }

    /// Focus in piece `i`, with the cursor at `at` (moved to a character boundary).
    fn focus(&mut self, i: usize, at: usize) {
        for (j, p) in self.pieces.iter_mut().enumerate() {
            p.edit.focused = j == i;
        }
        self.last = self.pieces[i].id;
        let e = &mut self.pieces[i].edit;
        let at = (0..=at.min(e.text.len())).rev().find(|&k| e.text.is_char_boundary(k)).unwrap_or(0);
        e.set_caret(at);
    }

    /// Key `edge` on piece `id`, if the cursor is at the right edge (no selection or
    /// composition). Returns whether it took it; [`Edge::JoinBack`] and [`Edge::JoinForward`] change
    /// the text. `first` and `last`: the cursor is on the first or last drawn line of the piece
    /// (for the up and down arrows, which are unknown without layout).
    pub fn edge(&mut self, id: u64, edge: Edge, first: bool, last: bool) -> bool {
        let Some(i) = self.index(id) else { return false };
        let e = &self.pieces[i].edit;
        if e.anchor != e.focus || e.preedit().is_some() {
            return false;
        }
        let (at, len) = (e.focus, e.text.len());
        let has_prev = i > 0;
        let has_next = i + 1 < self.pieces.len();
        match edge {
            Edge::JoinBack if at == 0 && has_prev => self.join(i - 1),
            Edge::JoinForward if at == len && has_next => self.join(i),
            Edge::Left if at == 0 && has_prev => {
                let end = self.pieces[i - 1].edit.text.len();
                self.focus(i - 1, end);
            }
            Edge::Right if at == len && has_next => self.focus(i + 1, 0),
            Edge::Up if first && has_prev => {
                let col = column(&e.text, at);
                let prev = &self.pieces[i - 1].edit.text;
                let start = prev.rfind('\n').map_or(0, |k| k + 1);
                let at = start + byte_at_column(&prev[start..], col);
                self.focus(i - 1, at);
            }
            Edge::Down if last && has_next => {
                let col = column(&e.text, at);
                let next = &self.pieces[i + 1].edit.text;
                let line = &next[..next.find('\n').unwrap_or(next.len())];
                let at = byte_at_column(line, col);
                self.focus(i + 1, at);
            }
            _ => return false,
        }
        true
    }

    /// Joins piece `i + 1` to piece `i` (removes the `\n` between the two); the cursor goes where they
    /// met. The field history restarts: a Cmd+Z cannot remove the joined piece.
    fn join(&mut self, i: usize) {
        let next = self.pieces.remove(i + 1);
        let at = self.pieces[i].edit.text.len();
        let text = format!("{}{}", self.pieces[i].edit.text, next.edit.text);
        self.pieces[i].edit.replace_text(text);
        self.version += 1;
        self.focus(i, at);
    }
}

/// The column (in characters) of byte `at` in its line.
fn column(text: &str, at: usize) -> usize {
    let start = text[..at].rfind('\n').map_or(0, |k| k + 1);
    text[start..at].chars().count()
}

/// The byte of column `col` in `line` (at most its end).
fn byte_at_column(line: &str, col: usize) -> usize {
    let line = line.strip_suffix('\r').unwrap_or(line);
    line.char_indices().nth(col).map_or(line.len(), |(k, _)| k)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(p: &Pieces) -> Vec<&str> {
        p.iter().map(|x| x.edit.text.as_str()).collect()
    }

    #[test]
    fn split_cuts_before_a_line_after_a_blank_one() {
        assert_eq!(split("# a\n\none\ntwo\n\n\n\nthree\n"), ["# a\n", "one\ntwo\n\n\n", "three\n"]);
        assert_eq!(split(""), [""]);
        assert_eq!(split("\n\n\na"), ["\n\n", "a"]);
        assert_eq!(split("a\r\n\r\nb"), ["a\r\n\r", "b"]);
    }

    #[test]
    fn split_keeps_fenced_code_whole() {
        let t = "a\n\n```rs\nfn a() {}\n\nfn b() {}\n```\n\nb\n";
        assert_eq!(split(t), ["a\n", "```rs\nfn a() {}\n\nfn b() {}\n```\n", "b\n"]);
        let tilde = "~~~~\n\n```\n\n~~~~\n\nc";
        assert_eq!(split(tilde), ["~~~~\n\n```\n\n~~~~\n", "c"]);
    }

    #[test]
    fn split_cuts_long_pieces_at_a_line() {
        let line = "x".repeat(100);
        let t = vec![line.as_str(); 200].join("\n");
        let pieces = split(&t);
        assert!(pieces.len() > 1);
        assert!(pieces.iter().all(|p| p.len() <= MAX_PIECE + 101), "{:?}", pieces.iter().map(|p| p.len()).collect::<Vec<_>>());
        assert_eq!(pieces.join("\n"), t);
    }

    #[test]
    fn caret_crosses_pieces() {
        let mut p = Pieces::new("ab\n\ncd");
        assert_eq!(texts(&p), ["ab\n", "cd"]);
        p.set_caret(4);
        assert_eq!((p.focused(), p.current().focus), (Some(1), 0));
        assert_eq!(p.caret(), 4);
        p.set_caret(3);
        assert_eq!((p.focused(), p.current().focus), (Some(0), 3), "the \\n between two pieces is the end of the first");
        p.set_caret(99);
        assert_eq!((p.focused(), p.caret()), (Some(1), 6));
        // Without focus the last piece that had it holds.
        p.current().focused = false;
        assert_eq!((p.focused(), p.caret()), (None, 6));
    }

    #[test]
    fn backspace_at_the_start_joins_the_previous_piece() {
        let mut p = Pieces::new("ab\n\ncd\n\nef");
        let ids: Vec<u64> = p.iter().map(|x| x.id).collect();
        p.set_caret(4);
        assert!(p.edge(ids[1], Edge::JoinBack, true, true));
        assert_eq!(texts(&p), ["ab\ncd\n", "ef"], "the \\n between the two is removed");
        assert_eq!(p.text(), "ab\ncd\n\nef");
        assert_eq!((p.focused(), p.current().focus), (Some(0), 3));
        // At the start of the first piece there is nothing to join.
        p.set_caret(0);
        assert!(!p.edge(ids[0], Edge::JoinBack, true, true));
        // With a selection, Backspace belongs to the field.
        p.set_caret(7);
        p.current().set_selection(0, 1);
        assert!(!p.edge(ids[2], Edge::JoinBack, true, true));
    }

    #[test]
    fn delete_at_the_end_joins_the_next_piece() {
        let mut p = Pieces::new("ab\n\ncd");
        let first = p.get(0).unwrap().id;
        p.set_caret(3);
        assert!(p.edge(first, Edge::JoinForward, true, true));
        assert_eq!(texts(&p), ["ab\ncd"]);
        assert_eq!((p.focused(), p.current().focus), (Some(0), 3));
        assert!(!p.edge(first, Edge::JoinForward, true, true), "in the middle it does not join");
    }

    #[test]
    fn a_join_cannot_be_undone_into_losing_text() {
        let mut p = Pieces::new("ab\n\ncd");
        let second = p.get(1).unwrap().id;
        p.set_caret(4);
        assert!(p.edge(second, Edge::JoinBack, true, true));
        p.current().undo();
        assert_eq!(p.text(), "ab\ncd");
    }

    #[test]
    fn arrows_move_to_the_neighbour_piece() {
        let mut p = Pieces::new("one\ntwo\n\nsix\nseventy");
        let ids: Vec<u64> = p.iter().map(|x| x.id).collect();
        // "six" at column 2 → up: last line of the first (empty) → column 0.
        p.set_caret(11);
        assert!(!p.edge(ids[1], Edge::Up, false, false), "not on the first line: it belongs to the field");
        assert!(p.edge(ids[1], Edge::Up, true, false));
        assert_eq!((p.focused(), p.caret()), (Some(0), 8));
        // down from the last line of the first: first line of the second, same column (0).
        assert!(p.edge(ids[0], Edge::Down, false, true));
        assert_eq!((p.focused(), p.caret()), (Some(1), 9));
        // column kept: "two" column 2 is not the last line of the first piece, but with `last`.
        p.set_caret(6);
        assert!(p.edge(ids[0], Edge::Down, false, true));
        assert_eq!((p.focused(), p.caret()), (Some(1), 11));
        // left at the start, right at the end.
        p.set_caret(9);
        assert!(p.edge(ids[1], Edge::Left, false, false));
        assert_eq!((p.focused(), p.caret()), (Some(0), 8));
        assert!(p.edge(ids[0], Edge::Right, false, false));
        assert_eq!((p.focused(), p.caret()), (Some(1), 9));
        // At the ends of the file the arrows stay with the field.
        assert!(!p.edge(ids[1], Edge::Down, false, true));
        p.set_caret(0);
        assert!(!p.edge(ids[0], Edge::Up, true, false));
    }
}
