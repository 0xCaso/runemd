//! Il sorgente diviso in pezzi, un `TextEdit` per pezzo: un solo campo da
//! 1 MB costa a Rune ~150 ms per tasto, un campo per pezzo dentro `virtual_rows` costa solo le
//! righe montate.
//!
//! I pezzi sono il testo del file tagliato a certi `\n`, che si tolgono: riuniti con `"\n"`
//! ridanno il file identico, byte per byte (righe vuote in più, file senza `\n` finale, CRLF,
//! righe vuote in testa, file vuoto). Si taglia prima di una riga piena che segue una riga vuota,
//! fuori dai blocchi di codice recintati; un pezzo più lungo di [`MAX_PIECE`] si taglia anche a
//! una riga qualsiasi (dentro un recinto solo oltre [`MAX_FENCED_PIECE`]).
//!
//! Il tasto ai bordi di un pezzo fa quello che farebbe in un campo unico: Backspace all'inizio
//! toglie il `\n` che lo separa dal precedente (i due si uniscono), Canc alla fine lo stesso con
//! il seguente, le frecce passano al pezzo vicino.

use rune::TextEdit;

/// Oltre questa lunghezza un pezzo si taglia alla prima riga (fuori dai recinti).
pub const MAX_PIECE: usize = 4 * 1024;
/// Dentro un blocco di codice recintato si taglia solo oltre questa lunghezza: i colori del
/// codice che segue sono quelli del Markdown, ma un recinto aperto e mai chiuso non fa del resto
/// del file un campo solo.
pub const MAX_FENCED_PIECE: usize = 64 * 1024;

/// Il testo diviso in pezzi: `split(t).join("\n") == t`.
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

/// Il recinto aperto dopo la riga `line`: ``` o ~~~ (tre o più, al massimo tre spazi prima) lo
/// aprono, una riga di soli segni uguali e almeno altrettanti lo chiude.
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

/// Un pezzo del sorgente: il suo id (chiave della riga, stabile mentre si scrive) e il suo campo.
#[derive(Debug)]
pub struct Piece {
    pub id: u64,
    pub edit: TextEdit,
}

/// I pezzi del sorgente in modifica; uno ha il fuoco (o nessuno, se il campo l'ha perso).
#[derive(Debug, Default)]
pub struct Pieces {
    pieces: Vec<Piece>,
    next_id: u64,
    /// Cambia quando i pezzi si uniscono o si ridividono: le chiavi delle righe cambiano.
    pub version: u64,
    /// L'ultimo pezzo che ha avuto il fuoco o una modifica: il suo cursore vale anche quando il
    /// fuoco è altrove (un clic su "Ricarica").
    last: u64,
}

/// Cosa fa un tasto ai bordi di un pezzo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edge {
    /// Backspace all'inizio: unisce il pezzo al precedente.
    JoinBack,
    /// Canc alla fine: unisce il seguente.
    JoinForward,
    /// Freccia su (prima riga) o sinistra (inizio): al pezzo prima.
    Up,
    Left,
    /// Freccia giù (ultima riga) o destra (fine): al pezzo dopo.
    Down,
    Right,
}

impl Pieces {
    /// I pezzi di `text`, con il cursore all'inizio e il fuoco nel primo.
    pub fn new(text: &str) -> Pieces {
        let mut p = Pieces::default();
        p.set_text(text);
        p
    }

    /// Ridivide tutto da `text`: id nuovi, cronologia persa, cursore all'inizio e fuoco nel primo.
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

    /// Il testo intero: i pezzi riuniti con `"\n"`.
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

    /// Il campo del pezzo `id`, se c'è ancora (`on_edit`). Rune passa da qui per ogni tasto, clic
    /// e perdita di fuoco del campo: se il pezzo ha il fuoco diventa l'ultimo usato (Rune lo
    /// chiede anche per pezzi senza fuoco, che non contano).
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

    /// L'indice del pezzo con il fuoco.
    pub fn focused(&self) -> Option<usize> {
        self.pieces.iter().position(|p| p.edit.focused)
    }

    /// Il campo con il fuoco (il primo se nessuno ce l'ha).
    pub fn current(&mut self) -> &mut TextEdit {
        let i = self.focused().unwrap_or(0);
        &mut self.pieces[i].edit
    }

    /// Il cursore nel testo intero (byte): nel pezzo con il fuoco, o nell'ultimo che l'ha avuto.
    pub fn caret(&self) -> usize {
        let Some(i) = self.focused().or_else(|| self.index(self.last)) else { return 0 };
        self.pieces[..i].iter().map(|p| p.edit.text.len() + 1).sum::<usize>() + self.pieces[i].edit.focus
    }

    /// Mette il cursore a `at` del testo intero (un `\n` fra due pezzi è la fine del primo) e il
    /// fuoco nel suo pezzo.
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

    /// Il fuoco nel pezzo `i`, con il cursore a `at` (portato a un confine di carattere).
    fn focus(&mut self, i: usize, at: usize) {
        for (j, p) in self.pieces.iter_mut().enumerate() {
            p.edit.focused = j == i;
        }
        self.last = self.pieces[i].id;
        let e = &mut self.pieces[i].edit;
        let at = (0..=at.min(e.text.len())).rev().find(|&k| e.text.is_char_boundary(k)).unwrap_or(0);
        e.set_caret(at);
    }

    /// Il tasto `edge` sul pezzo `id`, se il cursore è al bordo giusto (nessuna selezione né
    /// composizione). Ritorna se l'ha preso; [`Edge::JoinBack`] e [`Edge::JoinForward`] cambiano
    /// il testo. `first` e `last`: il cursore è sulla prima o sull'ultima riga disegnata del pezzo
    /// (per le frecce su e giù, che senza layout non si sanno).
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

    /// Unisce il pezzo `i + 1` al pezzo `i` (toglie il `\n` fra i due); il cursore va dove si
    /// incontravano. La cronologia del campo riparte: un Cmd+Z non può togliere il pezzo unito.
    fn join(&mut self, i: usize) {
        let next = self.pieces.remove(i + 1);
        let at = self.pieces[i].edit.text.len();
        let text = format!("{}{}", self.pieces[i].edit.text, next.edit.text);
        self.pieces[i].edit.replace_text(text);
        self.version += 1;
        self.focus(i, at);
    }
}

/// La colonna (in caratteri) del byte `at` nella sua riga.
fn column(text: &str, at: usize) -> usize {
    let start = text[..at].rfind('\n').map_or(0, |k| k + 1);
    text[start..at].chars().count()
}

/// Il byte della colonna `col` in `line` (al massimo la sua fine).
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
        assert_eq!(split("# a\n\nuno\ndue\n\n\n\ntre\n"), ["# a\n", "uno\ndue\n\n\n", "tre\n"]);
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
        assert_eq!((p.focused(), p.current().focus), (Some(0), 3), "il \\n fra due pezzi è la fine del primo");
        p.set_caret(99);
        assert_eq!((p.focused(), p.caret()), (Some(1), 6));
        // Senza fuoco vale l'ultimo pezzo che l'ha avuto.
        p.current().focused = false;
        assert_eq!((p.focused(), p.caret()), (None, 6));
    }

    #[test]
    fn backspace_at_the_start_joins_the_previous_piece() {
        let mut p = Pieces::new("ab\n\ncd\n\nef");
        let ids: Vec<u64> = p.iter().map(|x| x.id).collect();
        p.set_caret(4);
        assert!(p.edge(ids[1], Edge::JoinBack, true, true));
        assert_eq!(texts(&p), ["ab\ncd\n", "ef"], "il \\n fra i due è tolto");
        assert_eq!(p.text(), "ab\ncd\n\nef");
        assert_eq!((p.focused(), p.current().focus), (Some(0), 3));
        // All'inizio del primo pezzo non c'è niente da unire.
        p.set_caret(0);
        assert!(!p.edge(ids[0], Edge::JoinBack, true, true));
        // Con una selezione, Backspace è del campo.
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
        assert!(!p.edge(first, Edge::JoinForward, true, true), "a metà non unisce");
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
        let mut p = Pieces::new("uno\ndue\n\ntre\nquattro");
        let ids: Vec<u64> = p.iter().map(|x| x.id).collect();
        // "tre" a colonna 2 → su: ultima riga del primo (vuota) → colonna 0.
        p.set_caret(11);
        assert!(!p.edge(ids[1], Edge::Up, false, false), "non sulla prima riga: è del campo");
        assert!(p.edge(ids[1], Edge::Up, true, false));
        assert_eq!((p.focused(), p.caret()), (Some(0), 8));
        // giù dall'ultima riga del primo: prima riga del secondo, stessa colonna (0).
        assert!(p.edge(ids[0], Edge::Down, false, true));
        assert_eq!((p.focused(), p.caret()), (Some(1), 9));
        // colonna conservata: "due" colonna 2 non è l'ultima riga del primo pezzo, ma con `last`.
        p.set_caret(6);
        assert!(p.edge(ids[0], Edge::Down, false, true));
        assert_eq!((p.focused(), p.caret()), (Some(1), 11));
        // sinistra all'inizio, destra alla fine.
        p.set_caret(9);
        assert!(p.edge(ids[1], Edge::Left, false, false));
        assert_eq!((p.focused(), p.caret()), (Some(0), 8));
        assert!(p.edge(ids[0], Edge::Right, false, false));
        assert_eq!((p.focused(), p.caret()), (Some(1), 9));
        // Ai capi del file le frecce restano al campo.
        assert!(!p.edge(ids[1], Edge::Down, false, true));
        p.set_caret(0);
        assert!(!p.edge(ids[0], Edge::Up, true, false));
    }
}
