//! Apri veloce (Cmd+P): il campo "Apri file" della sidebar cerca fra i nomi dell'indice e la
//! lista dei risultati prende il posto di Recenti e dell'albero finché la ricerca non è vuota.

use std::cmp::Reverse;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use rune::{Key, KeyEvent, TextEdit, Tx, TextStyle, col, row, text, text_input, icon, El, ScrollSpec, el};
use rune_kit::glyphs;

use super::sidebar::{Hooks, Motion, pressable, small_font, ui_font, name_cell};
use super::theme::{FIELD_H, FIELD_MARGIN_BOTTOM, FIELD_MARGIN_X, HAIRLINE, ICON, KBD_PAD_X, KBD_PAD_Y, KBD_TEXT, KBD_WEIGHT, R_FIELD, R_KBD, ROW, ROW_GAP, ROW_PAD, SECTION_PAD_BOTTOM, SECTION_PAD_TOP, Theme};

/// Risultati al massimo (ipotesi: oltre non si scorre, si scrive di più).
pub const MAX_RESULTS: usize = 50;


/// Un nome che combacia vale sempre più di una cartella che combacia.
const BASENAME_BONUS: i32 = 10_000;
/// Una cartella dal nome uguale alla ricerca, e una che la contiene come sottosequenza.
const FOLDER_EXACT: i32 = 5_000;
const FOLDER_FUZZY: i32 = 1_000;
/// Ogni cartella fra quella che combacia e il file toglie questo: a parità vince il file più vicino.
const DEPTH_COST: i32 = 100;
/// Delle cartelle sopra il file se ne guardano al massimo queste: il resto (`/Users/…`) non dice cosa cercare.
const FOLDER_PARTS: usize = 4;
/// Un percorso con "/" nella ricerca si confronta con le ultime parti, come testo unico.
const TAIL_PARTS: usize = 3;

/// Sottosequenza senza maiuscole, come `score` di `rune-kit/src/combobox.rs` (privata, quindi
/// copiata): le corrispondenze contigue e a inizio parola valgono di più, ogni byte saltato toglie
/// uno. `None` se il testo non contiene tutta la ricerca. Lavora sui byte (maiuscole ASCII): per
/// 7000 percorsi a ogni tasto la versione a caratteri costava 4 ms (p95 5,1 ms).
fn score(needle: &[u8], hay: &[u8]) -> Option<i32> {
    if hay.len() < needle.len() {
        return None;
    }
    // Salta da una lettera cercata alla successiva: un percorso senza la ricerca finisce presto.
    let mut at = 0;
    let mut total = -((hay.len() - needle.len()) as i32);
    let mut last: Option<usize> = None;
    for &want in needle {
        let i = at + hay[at..].iter().position(|b| b.to_ascii_lowercase() == want)?;
        let boundary = i == 0 || !(hay[i - 1].is_ascii_alphanumeric() || hay[i - 1] >= 0x80);
        total += 10 + if last.is_some_and(|l| l + 1 == i) { 12 } else { 0 } + if boundary { 8 } else { 0 };
        last = Some(i);
        at = i + 1;
    }
    Some(total)
}

/// Il punteggio di una parola della ricerca: il nome del file prima; poi la cartella che combacia
/// (il nome intero meglio di una sottosequenza, la più vicina al file meglio della più lontana).
/// Niente penalità per la lunghezza delle parti sopra la cartella che combacia.
fn score_token(needle: &[u8], full: &[u8]) -> Option<i32> {
    let name_at = full.iter().rposition(|&b| b == b'/').map_or(0, |i| i + 1);
    if let Some(s) = score(needle, &full[name_at..]) {
        return Some(BASENAME_BONUS + s);
    }
    let mut best: Option<i32> = None;
    let mut end = name_at.saturating_sub(1);
    for depth in 0..FOLDER_PARTS {
        if end == 0 {
            break;
        }
        let start = full[..end].iter().rposition(|&b| b == b'/').map_or(0, |i| i + 1);
        let seg = &full[start..end];
        let found = if seg.eq_ignore_ascii_case(needle) { Some(FOLDER_EXACT) } else { score(needle, seg).map(|s| FOLDER_FUZZY + s) };
        if let Some(f) = found.map(|f| f - depth as i32 * DEPTH_COST) {
            best = Some(best.map_or(f, |b| b.max(f)));
        }
        end = start.saturating_sub(1);
    }
    if best.is_none() && needle.contains(&b'/') {
        let mut cut = name_at;
        for _ in 1..TAIL_PARTS {
            cut = full[..cut.saturating_sub(1)].iter().rposition(|&b| b == b'/').map_or(0, |i| i + 1);
            if cut == 0 {
                break;
            }
        }
        return score(needle, &full[cut..]);
    }
    best
}

/// Il punteggio di un percorso: ogni parola della ricerca (separate da spazi) deve combaciare,
/// in qualsiasi ordine; il punteggio è la somma.
fn score_path(tokens: &[Vec<u8>], path: &Path) -> Option<i32> {
    let full = path.as_os_str().as_bytes();
    let mut total = 0;
    for t in tokens {
        total += score_token(t, full)?;
    }
    Some(total)
}

/// I `limit` percorsi che meglio combaciano con `query`, dal migliore; a parità il più corto e poi
/// l'ordine alfabetico. Una ricerca vuota non dà niente. Gira sul thread della UI a ogni tasto:
/// niente copie per i percorsi che non combaciano.
pub fn rank<'a>(query: &str, paths: impl Iterator<Item = &'a Path>, limit: usize) -> Vec<PathBuf> {
    let lower = query.to_lowercase();
    let tokens: Vec<Vec<u8>> = lower.split_whitespace().map(|t| t.as_bytes().to_vec()).collect();
    if tokens.is_empty() || limit == 0 {
        return Vec::new();
    }
    let mut hits: Vec<(i32, &Path)> = paths.filter_map(|p| score_path(&tokens, p).map(|s| (s, p))).collect();
    let order = |a: &(i32, &Path), b: &(i32, &Path)| (Reverse(a.0), a.1.as_os_str().len(), a.1).cmp(&(Reverse(b.0), b.1.as_os_str().len(), b.1));
    if hits.len() > limit {
        hits.select_nth_unstable_by(limit, order);
        hits.truncate(limit);
    }
    hits.sort_by(order);
    hits.into_iter().map(|(_, p)| p.to_path_buf()).collect()
}

/// Lo stato di Cmd+P: il campo, i risultati e la riga scelta con le frecce.
#[derive(Default)]
pub struct QuickOpen {
    pub edit: TextEdit,
    pub hits: Vec<PathBuf>,
    pub selected: usize,
}

impl QuickOpen {
    /// La lista dei risultati si vede finché la ricerca non è vuota.
    pub fn active(&self) -> bool {
        !self.edit.text.trim().is_empty()
    }

    /// Cmd+P: il fuoco va nel campo.
    pub fn focus(&mut self) {
        self.edit.focused = true;
    }

    /// Esc, o un file scelto: ricerca vuota e fuoco fuori dal campo.
    pub fn close(&mut self) {
        self.edit = TextEdit::default();
        self.hits.clear();
        self.selected = 0;
    }

    /// La ricerca è cambiata: nuovi risultati dai nomi dell'indice, scelta alla prima riga.
    pub fn requery<'a>(&mut self, names: impl Iterator<Item = &'a Path>) {
        self.hits = rank(&self.edit.text, names, MAX_RESULTS);
        self.selected = 0;
    }

    /// È cambiato l'indice, non la ricerca: la riga scelta resta quella, per percorso (una
    /// scrittura altrove non deve spostare ciò che Invio apre). Se non c'è più, torna alla prima.
    pub fn refresh<'a>(&mut self, names: impl Iterator<Item = &'a Path>) {
        let before = self.chosen().map(Path::to_path_buf);
        self.hits = rank(&self.edit.text, names, MAX_RESULTS);
        self.selected = before.and_then(|b| self.hits.iter().position(|h| *h == b)).unwrap_or(0);
    }

    /// Sposta la scelta di `delta` righe, da un capo all'altro.
    pub fn step(&mut self, delta: isize) {
        if !self.hits.is_empty() {
            self.selected = (self.selected as isize + delta).rem_euclid(self.hits.len() as isize) as usize;
        }
    }

    pub fn chosen(&self) -> Option<&Path> {
        self.hits.get(self.selected).map(PathBuf::as_path)
    }
}

fn row_key(path: &Path) -> String {
    format!("quick-{}", path.display())
}

/// Il campo "Apri file ⌘P": un campo di testo vero. ⌘P gli dà il fuoco; frecce, Invio ed Esc
/// lavorano sulla lista sotto.
pub(super) fn field<S: 'static>(sb: &super::sidebar::Sidebar, hooks: Hooks<S>, theme: &Theme) -> El {
    let kbd = kbd("⌘P", theme);
    let input = text_input(&sb.quick.edit, ui_font())
        .single_line()
        .key("quick-input")
        .grow(1.0)
        .shrink()
        .min_w(0.0)
        .color(theme.fg)
        .placeholder("Apri file", theme.fg2)
        .caret(theme.fg, theme.sel)
        .label("Apri file")
        .on_edit(move |s: &mut S| &mut (hooks.get_mut)(s).quick.edit)
        .on_input_change(move |s: &mut S, _: &mut Tx| (hooks.query)(s))
        .on_input_key(move |s: &mut S, ev: &KeyEvent, tx: &mut Tx| on_key(s, hooks, ev, tx));
    field_row(icon(&glyphs::SEARCH, ICON).color(theme.fg2), input, kbd, theme)
}

/// La riga del campo della sidebar (Cmd+P e Cmd+N): icona, campo di testo e il tasto a destra.
pub(super) fn field_row(lead: El, input: El, kbd: El, theme: &Theme) -> El {
    row()
        .key("open-file")
        .h(FIELD_H)
        .mx(FIELD_MARGIN_X)
        .mb(FIELD_MARGIN_BOTTOM)
        .px(ROW_PAD)
        .gap(ROW_GAP)
        .center_y()
        .radius(R_FIELD)
        .fill(theme.field)
        .border(HAIRLINE, theme.line)
        .child(lead)
        .child(input)
        .child(kbd)
}

/// Il tasto scorciatoia a destra del campo ("⌘P").
pub(super) fn kbd(label: &'static str, theme: &Theme) -> El {
    el().key("kbd").border(HAIRLINE, theme.line).radius(R_KBD).px(KBD_PAD_X).py(KBD_PAD_Y).child(text(label).font(TextStyle::new(KBD_TEXT, KBD_WEIGHT)).color(theme.fg2))
}

/// I tasti del campo prima dell'editor. Vero se li prende.
fn on_key<S: 'static>(s: &mut S, hooks: Hooks<S>, ev: &KeyEvent, tx: &mut Tx) -> bool {
    if (hooks.get)(s).quick.edit.preedit().is_some() {
        return false;
    }
    let repeat = tx.key_repeat();
    match &ev.key {
        Key::Escape => {
            if !repeat {
                (hooks.get_mut)(s).quick.close();
            }
            true
        }
        Key::Enter if !ev.mods.alt && !ev.mods.shift => {
            if !repeat {
                let chosen = (hooks.get)(s).quick.chosen().map(Path::to_path_buf);
                if let Some(path) = chosen {
                    (hooks.get_mut)(s).quick.close();
                    (hooks.open)(s, path, tx);
                }
            }
            true
        }
        Key::Down | Key::Up if !ev.mods.cmd && !ev.mods.ctrl && !ev.mods.alt => {
            let quick = &mut (hooks.get_mut)(s).quick;
            quick.step(if matches!(ev.key, Key::Down) { 1 } else { -1 });
            if let Some(p) = quick.chosen() {
                tx.scroll_into_view(row_key(p));
            }
            true
        }
        _ => false,
    }
}

/// La lista dei risultati, al posto di Recenti e dell'albero.
pub(super) fn view<S: 'static>(sb: &super::sidebar::Sidebar, hooks: Hooks<S>, theme: &Theme, motion: Motion) -> El {
    let mut list = col().key("quick-rows").w_full();
    if sb.quick_rows.is_empty() {
        list = list.child(text("Nessun file").font(small_font()).color(theme.fg3).px(ROW_PAD).pt(SECTION_PAD_TOP).pb(SECTION_PAD_BOTTOM));
    }
    for (i, r) in sb.quick_rows.iter().enumerate() {
        let selected = i == sb.quick.selected;
        let path = r.path.clone();
        let mut e = row()
            .w_full()
            .h(ROW)
            .px(ROW_PAD)
            .gap(ROW_GAP)
            .center_y()
            .child(name_cell(&r.name, r.qualifier.as_deref(), theme))
            .on_press_tx(move |s: &mut S, tx: &mut Tx| {
                (hooks.get_mut)(s).quick.close();
                (hooks.open)(s, path.clone(), tx);
            });
        if selected {
            e = e.fill(theme.sel);
        }
        let key = row_key(&r.path);
        list = list.child(pressable(e, key.clone(), sb, hooks, theme, motion, !selected).key(key));
    }
    col().key("quick").w_full().grow(1.0).min_h(0.0).scroll(ScrollSpec { stick_bottom: false }).child(list)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranks_basename_first() {
        let paths = [Path::new("a/spec.md"), Path::new("spec/x.md")];
        let hits = rank("spec", paths.iter().copied(), 10);
        assert_eq!(hits.first().map(PathBuf::as_path), Some(Path::new("a/spec.md")));
        let swapped = [Path::new("spec/x.md"), Path::new("a/spec.md")];
        assert_eq!(rank("spec", swapped.iter().copied(), 10)[0], Path::new("a/spec.md"));
    }

    #[test]
    fn needs_every_letter_in_order_and_ignores_case() {
        let paths = [Path::new("r/Notes.md"), Path::new("r/nos.md"), Path::new("r/readme.md")];
        let hits = rank("NTS", paths.iter().copied(), 10);
        assert_eq!(hits, [Path::new("r/Notes.md")]);
        assert!(rank("  ", paths.iter().copied(), 10).is_empty());
    }

    #[test]
    fn caps_the_results_and_keeps_the_best() {
        let owned: Vec<PathBuf> = (0..200).map(|i| PathBuf::from(format!("r/x{i}/file{i}.md"))).chain([PathBuf::from("r/zzz/f.md")]).collect();
        let hits = rank("f", owned.iter().map(PathBuf::as_path), 5);
        assert_eq!(hits.len(), 5);
        assert_eq!(hits[0], Path::new("r/zzz/f.md"), "il nome più corto e più vicino vince");
    }

    #[test]
    fn folder_match_still_finds_files_without_a_name_match() {
        let paths = [Path::new("/Users/me/runemd/README.md"), Path::new("/Users/me/other/notes.md")];
        assert_eq!(rank("runemd", paths.iter().copied(), 10), [Path::new("/Users/me/runemd/README.md")]);
    }

    #[test]
    fn arrows_wrap_and_enter_picks() {
        let mut q = QuickOpen::default();
        q.edit.replace_text("a");
        q.requery([Path::new("/r/a.md"), Path::new("/r/ab.md"), Path::new("/r/b.md")].into_iter());
        assert_eq!(q.hits.len(), 2);
        assert_eq!(q.chosen(), Some(Path::new("/r/a.md")));
        q.step(-1);
        assert_eq!(q.chosen(), Some(Path::new("/r/ab.md")));
        q.step(1);
        assert_eq!(q.chosen(), Some(Path::new("/r/a.md")));
        q.close();
        assert!(!q.active() && q.hits.is_empty() && !q.edit.focused);
    }

    #[test]
    fn exact_folder_beats_scattered_and_shallow_beats_deep() {
        let paths = [
            Path::new("/Users/me/GitHub/runemd/docs/spec.md"),
            Path::new("/Users/me/GitHub/rune-md-spike/README.md"),
            Path::new("/Users/me/GitHub/runemd/README.md"),
            Path::new("/Users/me/GitHub/runemd/docs/deep/more.md"),
        ];
        let hits = rank("runemd", paths.iter().copied(), 10);
        assert_eq!(hits[0], Path::new("/Users/me/GitHub/runemd/README.md"));
        assert_eq!(hits[1], Path::new("/Users/me/GitHub/runemd/docs/spec.md"));
        assert_eq!(hits[2], Path::new("/Users/me/GitHub/runemd/docs/deep/more.md"));
        assert_eq!(hits[3], Path::new("/Users/me/GitHub/rune-md-spike/README.md"));
        let named = [Path::new("/x/runemd/a.md"), Path::new("/y/runemd.md")];
        assert_eq!(rank("runemd", named.iter().copied(), 10)[0], Path::new("/y/runemd.md"), "il nome vince sulla cartella");
    }

    #[test]
    fn every_space_separated_word_must_match_in_any_order() {
        let paths = [Path::new("/r/docs/plan.md"), Path::new("/r/docs/notes.md"), Path::new("/r/plan/x.txt")];
        assert_eq!(rank("plan md", paths.iter().copied(), 10), [Path::new("/r/docs/plan.md")]);
        assert_eq!(rank("md plan", paths.iter().copied(), 10), [Path::new("/r/docs/plan.md")]);
        let tasks = [Path::new("/r/task/18/a.md"), Path::new("/r/task/19/a.md")];
        assert_eq!(rank("task 18", tasks.iter().copied(), 10), [Path::new("/r/task/18/a.md")]);
    }

    #[test]
    fn refresh_keeps_the_chosen_file_by_path() {
        let mut q = QuickOpen::default();
        q.edit.replace_text("md");
        let names = ["/r/a.md", "/r/b.md", "/r/c.md", "/r/d.md"].map(Path::new);
        q.requery(names.iter().copied());
        q.step(2);
        let chosen = q.chosen().unwrap().to_path_buf();
        // Un file nuovo entra davanti: la riga scelta resta lo stesso file.
        let more = ["/r/a.md", "/r/0.md", "/r/b.md", "/r/c.md", "/r/d.md"].map(Path::new);
        q.refresh(more.iter().copied());
        assert_eq!(q.chosen(), Some(chosen.as_path()));
        // Sparisce: torna alla prima. Con la ricerca cambiata si riparte dalla prima.
        let fewer: Vec<&Path> = more.iter().copied().filter(|p| **p != *chosen).collect();
        q.refresh(fewer.iter().copied());
        assert_eq!(q.selected, 0);
        q.step(1);
        q.requery(more.iter().copied());
        assert_eq!(q.selected, 0);
    }
}
