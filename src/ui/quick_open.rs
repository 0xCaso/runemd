//! Apri veloce (Cmd+P): un pannello largo 560 al centro della finestra, sopra il contenuto
//! (docs/decisions/2026-11-15-quick-open-compatto.md; mockup `2026-11-15-quick-open-raycast`,
//! variante "compatto"). In alto il campo, sotto i risultati su due righe come Recenti (nome;
//! repo o cartella e ora), in fondo i tasti. Con la ricerca vuota mostra Recenti. Il campo
//! "Apri file ⌘P" della sidebar apre il pannello. Clic singolo: anteprima, il pannello resta;
//! doppio clic: scheda fissa e il pannello si chiude. Esc o un clic fuori lo chiudono.

use std::cmp::Reverse;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use rune::{Color, Key, KeyEvent, TextEdit, Tx, TextStyle, col, row, text, text_input, icon, El, ScrollSpec, el};
use rune_kit::glyphs;

use super::sidebar::{Hooks, Motion, RECENT_LINE_GAP, RECENT_META_LH, RECENT_NAME_LH, RECENT_ROW, pressable, row_meta, small_font, ui_font};
use super::theme::{FIELD_H, FIELD_MARGIN_BOTTOM, FIELD_MARGIN_X, HAIRLINE, ICON, KBD_PAD_X, KBD_PAD_Y, KBD_TEXT, KBD_WEIGHT, R_FIELD, R_KBD, R_PANEL, R_ROW, ROW_GAP, ROW_PAD, SECTION_PAD_BOTTOM, SECTION_PAD_TOP, TAB_BAR_H, FRAME, Theme, UI_SMALL, UI_WEIGHT};

/// Risultati al massimo (ipotesi: oltre non si scorre, si scrive di più).
pub const MAX_RESULTS: usize = 50;

/// Il pannello: largo 560 (meno con la finestra stretta, `QUICK_SIDE` dai bordi), in alto a
/// `QUICK_TOP` dalla finestra, sotto la barra delle schede (mockup: 72).
pub const QUICK_W: f32 = 560.0;
const QUICK_TOP: f32 = FRAME + TAB_BAR_H + 20.0;
const QUICK_SIDE: f32 = 16.0;
/// Il campo in alto (48, testo 15), le righe (`RECENT_ROW`, margine 6, 2 fra le righe), il fondo (36).
const QUICK_HEAD: f32 = 48.0;
const QUICK_TEXT: f32 = 15.0;
const QUICK_PAD: f32 = 6.0;
const QUICK_ROW_GAP: f32 = 2.0;
const QUICK_FOOT: f32 = 36.0;
/// Righe in vista prima che la lista scorra.
const QUICK_VISIBLE: f32 = 8.0;
/// Il velo sopra il contenuto: `panel` al 60 % (mockup).
const SCRIM: f32 = 0.6;


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

/// Lo stato di Cmd+P: il pannello aperto, il campo, i risultati e la riga scelta con le frecce.
#[derive(Default)]
pub struct QuickOpen {
    pub open: bool,
    pub edit: TextEdit,
    pub hits: Vec<PathBuf>,
    pub selected: usize,
}

impl QuickOpen {
    /// Il pannello si vede.
    pub fn active(&self) -> bool {
        self.open
    }

    /// La ricerca è vuota: il pannello mostra Recenti.
    pub fn empty_query(&self) -> bool {
        self.edit.text.trim().is_empty()
    }

    /// Cmd+P o il campo della sidebar: il pannello si apre col fuoco nel campo.
    pub fn show(&mut self) {
        self.open = true;
        self.edit.focused = true;
    }

    /// Esc, un clic fuori o un file scelto: il pannello si chiude, ricerca vuota.
    pub fn close(&mut self) {
        self.open = false;
        self.edit = TextEdit::default();
        self.hits.clear();
        self.selected = 0;
    }

    /// Dopo un cambio dell'indice con la ricerca vuota: i risultati sono `paths` (Recenti), la
    /// riga scelta resta per percorso se `keep`.
    pub fn set_hits(&mut self, paths: Vec<PathBuf>, keep: bool) {
        let before = self.chosen().map(Path::to_path_buf);
        self.hits = paths;
        self.selected = before.filter(|_| keep).and_then(|b| self.hits.iter().position(|h| *h == b)).unwrap_or(0);
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

/// Il campo "Apri file ⌘P" della sidebar: un pulsante con l'aspetto del campo, che apre il pannello.
pub(super) fn launcher<S: 'static>(hooks: Hooks<S>, theme: &Theme) -> El {
    let kbd = kbd("⌘P", theme);
    let hint = text("Apri file").key("quick-launch").font(ui_font()).color(theme.fg2).grow(1.0).shrink().min_w(0.0).truncate();
    field_row(icon(&glyphs::SEARCH, ICON).color(theme.fg2), hint, kbd, theme).role("button").label("Apri file").on_press(move |s: &mut S| {
        (hooks.get_mut)(s).quick.show();
        (hooks.query)(s);
    })
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

/// Un tasto del fondo del pannello e cosa fa: tasti in `fg2` dentro un bordo `line`, parola in `fg3`.
fn hint(keys: &[&'static str], what: &'static str, theme: &Theme) -> El {
    let mut r = row().gap(4.0).center_y();
    for k in keys {
        r = r.child(el().border(HAIRLINE, theme.line).radius(R_KBD).px(KBD_PAD_X).py(KBD_PAD_Y).child(text(*k).font(TextStyle::new(KBD_TEXT, KBD_WEIGHT)).color(theme.fg2)));
    }
    r.child(text(what).font(TextStyle::new(UI_SMALL, UI_WEIGHT)).color(theme.fg3).ml(2.0))
}

/// Una riga dei risultati: icona del file, sopra il nome (`fg`, tagliato in mezzo), sotto repo o
/// cartella e l'ora (in [`row_meta`] anche in hover e scelta). Clic: anteprima e la riga diventa
/// la scelta, il pannello resta; doppio clic: scheda fissa e il pannello si chiude.
#[allow(clippy::too_many_arguments)]
fn result_row<S: 'static>(sb: &super::sidebar::Sidebar, hooks: Hooks<S>, i: usize, r: &super::sidebar::RecentRow, label: &str, theme: &Theme, motion: Motion) -> El {
    let selected = i == sb.quick.selected;
    let name = text(r.name.clone()).font(ui_font().lh(RECENT_NAME_LH)).color(theme.fg).truncate_middle().w_full().min_w(0.0);
    let place = text(r.qualifier.clone().unwrap_or_default()).font(TextStyle::new(UI_SMALL, UI_WEIGHT).lh(RECENT_META_LH)).color(row_meta(theme)).truncate().grow(1.0).shrink().min_w(0.0);
    let time = text(label.to_owned()).font(small_font().lh(RECENT_META_LH)).color(row_meta(theme));
    let lines = col().grow(1.0).shrink().min_w(0.0).gap(RECENT_LINE_GAP).child(name).child(row().w_full().gap(ROW_GAP).center_y().child(place).child(time));
    let glyph = icon(&glyphs::FILE, ICON).color(if selected { theme.fg } else { theme.fg2 });
    let (open, pin) = (r.path.clone(), r.path.clone());
    let mut e = row()
        .w_full()
        .h(RECENT_ROW)
        .px(10.0)
        .gap(10.0)
        .center_y()
        .child(glyph)
        .child(lines)
        .on_double_press_tx(move |s: &mut S, tx: &mut Tx| {
            (hooks.get_mut)(s).quick.close();
            (hooks.pin)(s, pin.clone(), tx);
        })
        .on_press_tx(move |s: &mut S, tx: &mut Tx| {
            (hooks.get_mut)(s).quick.selected = i;
            (hooks.open)(s, open.clone(), tx);
        });
    if selected {
        e = e.fill(theme.sel);
    }
    let key = row_key(&r.path);
    pressable(e, key.clone(), sb, hooks, theme, motion, !selected).radius(R_ROW).key(key)
}

/// Il pannello di Cmd+P, da montare alla radice: un velo `panel` al 60 % su tutta la finestra e
/// sopra, centrato, il pannello (`panel`, raggio `R_PANEL`, bordo e ombra del pannello). Il
/// pannello è modale: un clic fuori o Esc lo chiudono. Vuoto se è chiuso.
pub fn panel<S: 'static>(s: &S, hooks: Hooks<S>, theme: &Theme, reduced: bool, hover: bool) -> Vec<El> {
    let sb = (hooks.get)(s);
    if !sb.quick.active() {
        return Vec::new();
    }
    let motion = Motion { reduced, hover };
    let input = text_input(&sb.quick.edit, TextStyle::new(QUICK_TEXT, UI_WEIGHT))
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
    let head = row().key("quick-head").w_full().h(QUICK_HEAD).px(14.0).gap(10.0).center_y().child(icon(&glyphs::SEARCH, ICON).color(theme.fg2)).child(input);
    let line = || el().w_full().h(HAIRLINE).fill(theme.line);
    let mut list = col().key("quick-rows").w_full().padding(QUICK_PAD).gap(QUICK_ROW_GAP);
    if sb.quick_rows.is_empty() {
        let none = if sb.quick.empty_query() { "Nessun file recente" } else { "Nessun file" };
        list = list.child(text(none).key("quick-none").font(small_font()).color(theme.fg3).px(ROW_PAD).pt(SECTION_PAD_TOP).pb(SECTION_PAD_BOTTOM));
    }
    for (i, r) in sb.quick_rows.iter().enumerate() {
        let label = sb.quick_labels.get(i).map(String::as_str).unwrap_or("");
        list = list.child(result_row(sb, hooks, i, r, label, theme, motion));
    }
    let list_max = QUICK_PAD * 2.0 + QUICK_VISIBLE * RECENT_ROW + (QUICK_VISIBLE - 1.0) * QUICK_ROW_GAP;
    let scroll = col().key("quick").w_full().max_h(list_max).shrink().min_h(0.0).scroll(ScrollSpec { stick_bottom: false }).child(list);
    let n = sb.quick_rows.len();
    let count = text(if n == 1 { "1 file".to_owned() } else { format!("{n} file") }).key("quick-count").font(small_font()).color(theme.fg3);
    let foot = row()
        .key("quick-foot")
        .w_full()
        .h(QUICK_FOOT)
        .px(14.0)
        .gap(16.0)
        .center_y()
        .child(hint(&["↑", "↓"], "scegli", theme))
        .child(hint(&["↵"], "apri", theme))
        .child(hint(&["Esc"], "chiudi", theme))
        .child(el().grow(1.0))
        .child(count);
    let surface = col()
        .key("quick-panel")
        .role("dialog")
        .label("Apri file")
        .w_full()
        .max_w(QUICK_W)
        .fill(theme.panel)
        .radius(R_PANEL)
        .clip_rounded(R_PANEL)
        .border(HAIRLINE, theme.fg.alpha(0.10))
        .shadow(10.0, 30.0, Color::BLACK.alpha(0.35))
        .child(head)
        .child(line())
        .child(scroll)
        .child(line())
        .child(foot)
        .modal(true, move |s: &mut S| (hooks.get_mut)(s).quick.close());
    let veil = col().key("quick-scrim").absolute(0.0, 0.0).w_full().h_full().fill(theme.panel.alpha(SCRIM)).items_center().pt(QUICK_TOP).px(QUICK_SIDE).child(surface);
    vec![veil]
}

#[cfg(test)]
mod tests {
    use super::*;

    mod panel {
        use rune::{Env, Key, KeyEvent, Mods, Runtime};

        use crate::app::App;
        use crate::doc::document::Document;
        use crate::index::{Entry, Index, Library};

        fn find(rt: &Runtime<App>, key: &str) -> Option<rune::ElementInfo> {
            rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key))
        }

        fn settle(rt: &mut Runtime<App>) {
            for _ in 0..2000 {
                if !rt.frame(1.0 / 120.0) {
                    return;
                }
            }
            panic!("la vista non si ferma");
        }

        fn click(rt: &mut Runtime<App>, x: f32, y: f32) {
            rt.pointer_move(x, y);
            rt.pointer_down(x, y);
            rt.pointer_up(x, y);
            rt.frame(1.0 / 60.0);
        }

        fn centre(rt: &Runtime<App>, key: &str) -> (f32, f32) {
            let r = find(rt, key).unwrap_or_else(|| panic!("{key}")).rect;
            (r[0] + r[2] / 2.0, r[1] + r[3] / 2.0)
        }

        fn cmd_p() -> KeyEvent {
            KeyEvent { key: Key::Command('p'), mods: Mods { cmd: true, ..Mods::default() } }
        }

        /// Un'app 1200 × 900 con alfa.md aperto e beta.md, gamma.md nell'indice.
        fn open() -> (tempfile::TempDir, Runtime<App>) {
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path().canonicalize().unwrap();
            let mut entries = Vec::new();
            for n in ["alfa", "beta", "gamma"] {
                let p = root.join(format!("{n}.md"));
                std::fs::write(&p, format!("# {n}\n")).unwrap();
                entries.push(Entry { path: p, mtime: std::time::SystemTime::now() });
            }
            let mut state = App::bare(Library::load(Vec::new(), None));
            state.library.index = Index::from_entries(entries);
            state.push_tab(Document::from_source(&root.join("alfa.md"), "# alfa\n"));
            let mut rt = Runtime::new(state, crate::view, (1200.0, 900.0), Env::new(true, 2.0));
            rt.on_key = Some(crate::on_key);
            rt.frame(0.0);
            settle(&mut rt);
            (dir, rt)
        }

        fn tabs(rt: &Runtime<App>) -> Vec<(String, bool)> {
            rt.state.tabs.iter().map(|t| (t.title(), t.preview)).collect()
        }

        /// Vincoli 1, 3, 6: ⌘P apre un pannello largo 560 al centro della finestra, sopra il
        /// contenuto (il documento non si sposta); in fondo i tasti; Esc e il clic fuori chiudono.
        #[test]
        fn cmd_p_opens_a_560_panel_in_the_centre() {
            let (_d, mut rt) = open();
            let doc_before = find(&rt, "main").unwrap().rect;
            rt.key(cmd_p());
            settle(&mut rt);
            let p = find(&rt, "quick-panel").expect("pannello").rect;
            assert_eq!(p[2], super::super::QUICK_W);
            assert!((p[0] + p[2] / 2.0 - 600.0).abs() < 1.0, "centrato: {p:?}");
            assert!(p[1] > 44.0, "sotto la barra delle schede: {p:?}");
            assert_eq!(find(&rt, "main").unwrap().rect, doc_before, "il documento non si sposta");
            let foot: Vec<String> = rt.elements().into_iter().filter_map(|e| e.text).collect();
            for k in ["↑", "↓", "↵", "Esc", "scegli", "apri", "chiudi"] {
                assert!(foot.iter().any(|t| t == k), "manca {k} in fondo");
            }
            assert!(rt.state.sidebar.quick.edit.focused, "fuoco nel campo");

            rt.key(KeyEvent { key: Key::Escape, mods: Mods::default() });
            settle(&mut rt);
            assert!(find(&rt, "quick-panel").is_none(), "Esc chiude");

            rt.key(cmd_p());
            settle(&mut rt);
            click(&mut rt, 600.0, 860.0);
            settle(&mut rt);
            assert!(find(&rt, "quick-panel").is_none(), "il clic fuori chiude");

            // Il campo della sidebar apre lo stesso pannello.
            let (x, y) = centre(&rt, "open-file");
            click(&mut rt, x, y);
            settle(&mut rt);
            assert!(find(&rt, "quick-panel").is_some());
        }

        /// Vincoli 2 e 4: righe su due righe (nome; repo o cartella e ora); clic singolo apre in
        /// anteprima e il pannello resta, doppio clic fissa la scheda e chiude.
        #[test]
        fn a_double_click_pins_the_tab() {
            let (_d, mut rt) = open();
            rt.key(cmd_p());
            settle(&mut rt);
            rt.update(|s: &mut App, _| {
                s.sidebar.quick.edit.replace_text("beta");
                s.requery_sidebar();
            });
            settle(&mut rt);
            let rows: Vec<String> = rt.state.sidebar.quick_rows.iter().map(|r| r.name.clone()).collect();
            assert_eq!(rows, ["beta.md"]);
            assert!(rt.state.sidebar.quick_rows[0].qualifier.is_some(), "seconda riga: repo o cartella");
            assert!(!rt.state.sidebar.quick_labels[0].is_empty(), "seconda riga: l'ora");
            let key = rt.elements().into_iter().filter_map(|e| e.key).find(|k| k.starts_with("quick-/")).expect("riga");

            let (x, y) = centre(&rt, &key);
            click(&mut rt, x, y);
            settle(&mut rt);
            assert_eq!(tabs(&rt), [("alfa.md".to_owned(), false), ("beta.md".to_owned(), true)], "clic singolo: anteprima");
            assert!(find(&rt, "quick-panel").is_some(), "il pannello resta");

            click(&mut rt, x, y);
            click(&mut rt, x, y);
            settle(&mut rt);
            assert_eq!(tabs(&rt), [("alfa.md".to_owned(), false), ("beta.md".to_owned(), false)], "doppio clic: fissa");
            assert!(find(&rt, "quick-panel").is_none(), "e chiude");
        }
    }

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
