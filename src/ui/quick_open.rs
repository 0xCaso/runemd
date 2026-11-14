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


/// Quanto pesa un nome che combacia sopra un percorso che combacia: il nome vince sempre.
const BASENAME_BONUS: i32 = 10_000;
/// Del percorso si guardano le ultime cartelle: il resto (`/Users/…`) non dice cosa cercare.
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

/// Il punteggio di un percorso: il nome del file prima, poi le ultime cartelle.
fn score_path(needle: &[u8], path: &Path) -> Option<i32> {
    let full = path.as_os_str().as_bytes();
    let name_at = full.iter().rposition(|&b| b == b'/').map_or(0, |i| i + 1);
    if let Some(s) = score(needle, &full[name_at..]) {
        return Some(BASENAME_BONUS + s);
    }
    let mut cut = name_at;
    for _ in 1..TAIL_PARTS {
        cut = full[..cut.saturating_sub(1)].iter().rposition(|&b| b == b'/').unwrap_or(0);
        if cut == 0 {
            break;
        }
    }
    score(needle, &full[cut..])
}

/// I `limit` percorsi che meglio combaciano con `query`, dal migliore; a parità il più corto e poi
/// l'ordine alfabetico. Una ricerca vuota non dà niente. Gira sul thread della UI a ogni tasto:
/// niente copie per i percorsi che non combaciano.
pub fn rank<'a>(query: &str, paths: impl Iterator<Item = &'a Path>, limit: usize) -> Vec<PathBuf> {
    let needle: Vec<u8> = query.trim().to_lowercase().into_bytes();
    if needle.is_empty() || limit == 0 {
        return Vec::new();
    }
    let mut hits: Vec<(i32, &Path)> = paths.filter_map(|p| score_path(&needle, p).map(|s| (s, p))).collect();
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

    /// Ricalcola i risultati dai nomi dell'indice; la scelta torna alla prima riga.
    pub fn requery<'a>(&mut self, names: impl Iterator<Item = &'a Path>) {
        self.hits = rank(&self.edit.text, names, MAX_RESULTS);
        self.selected = 0;
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
    let kbd = el().key("kbd").border(HAIRLINE, theme.line).radius(R_KBD).px(KBD_PAD_X).py(KBD_PAD_Y).child(text("⌘P").font(TextStyle::new(KBD_TEXT, KBD_WEIGHT)).color(theme.fg2));
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
        .child(icon(&glyphs::SEARCH, ICON).color(theme.fg2))
        .child(input)
        .child(kbd)
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
}
