//! Le impostazioni (spec, "Impostazioni"): cartelle osservate, font, dimensione, tema e fuoco in
//! fondo al pannello, salvate con `rune::prefs` in `~/Library/Application Support/RuneMD/prefs.json`.
//!
//! Ogni campo ha la sua chiave: un valore che manca o che non si legge (un'altra versione
//! dell'app) prende il default, gli altri restano.

use std::path::{Path, PathBuf};

use rune::prefs::{Prefs, PrefsError};
use serde::{Deserialize, Serialize};

/// L'id dell'app per `Prefs::open`: la stessa cartella dell'indice (`index::cache_path`).
pub const APP_ID: &str = "RuneMD";

/// Il font del documento di default (DESIGN.md, `typography.doc-body`).
pub const DEFAULT_FONT: &str = "Inter";
/// La dimensione del corpo di default, pt (DESIGN.md, `typography.doc-body`).
pub const DEFAULT_SIZE: f32 = 15.5;
/// Le dimensioni del corpo fra cui scelgono "−" e "+", pt. Contiene `DEFAULT_SIZE`.
pub const SIZES: [f32; 7] = [13.0, 14.0, 15.5, 17.0, 18.5, 20.0, 22.0];

/// "Intera": la colonna del testo segue la larghezza del pannello, meno i margini delle righe.
pub const COLUMN_FULL: f32 = 0.0;
/// Le larghezze della colonna del testo fra cui sceglie il cursore "Larghezza del testo", pt:
/// da 680 (DESIGN.md, `spacing.column`) a passi di 80, l'ultima "Intera".
pub const COLUMNS: [f32; 9] = [680.0, 760.0, 840.0, 920.0, 1000.0, 1080.0, 1160.0, 1240.0, COLUMN_FULL];
/// La larghezza della colonna di default, pt.
pub const DEFAULT_COLUMN: f32 = 680.0;

/// Il passo di `COLUMNS` più vicino al punto `t` (0..1) del cursore.
pub fn column_at(t: f32) -> f32 {
    let last = COLUMNS.len() - 1;
    COLUMNS[(t.clamp(0.0, 1.0) * last as f32).round() as usize]
}

/// Il punto del cursore (0..1) della larghezza `column`.
pub fn column_t(column: f32) -> f32 {
    let at = COLUMNS.iter().position(|c| *c == column).unwrap_or(0);
    at as f32 / (COLUMNS.len() - 1) as f32
}

/// Come il pannello scrive la larghezza: "680 pt", o "Intera".
pub fn column_label(column: f32) -> String {
    if column == COLUMN_FULL {
        "Intera".to_owned()
    } else {
        format!("{column:.0} pt")
    }
}

/// La larghezza massima della colonna: `None` con "Intera".
pub fn max_width(column: f32) -> Option<f32> {
    (column != COLUMN_FULL).then_some(column)
}

/// Le cartelle osservate di default, relative alla home.
pub const DEFAULT_ROOTS: [&str; 1] = ["Desktop/GitHub"];

/// Il tema: quello del sistema, o chiaro o scuro sempre.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThemePref {
    #[default]
    System,
    Light,
    Dark,
}

impl ThemePref {
    /// Scuro o no, dato il tema del sistema.
    pub fn dark(self, system_dark: bool) -> bool {
        match self {
            ThemePref::System => system_dark,
            ThemePref::Light => false,
            ThemePref::Dark => true,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    /// Le cartelle osservate, come le ha scelte l'utente (non canoniche).
    pub roots: Vec<PathBuf>,
    /// La famiglia del corpo e dei titoli; il codice resta monospaziato.
    pub font: String,
    /// La dimensione del corpo, pt; titoli, tabelle e codice scalano con lei.
    pub size: f32,
    pub theme: ThemePref,
    /// Il fuoco in fondo al pannello (decisione di Matteo, `docs/decisions/2026-11-14-fuoco-tema-chiaro.md`;
    /// sempre acceso dalla v1.1): spento, niente fiamme, né a riposo né col testo nuovo, e
    /// l'etichetta "N paragrafi nuovi più in basso" resta.
    pub fire: bool,
    /// La larghezza della colonna del testo, pt, uno di `COLUMNS` (`COLUMN_FULL` è "Intera").
    pub column: f32,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            roots: default_roots(),
            font: DEFAULT_FONT.to_owned(),
            size: DEFAULT_SIZE,
            theme: ThemePref::System,
            fire: true,
            column: DEFAULT_COLUMN,
        }
    }
}

/// `DEFAULT_ROOTS` sotto la home; nessuna senza `HOME`.
pub fn default_roots() -> Vec<PathBuf> {
    let Some(home) = std::env::var_os("HOME") else {
        return Vec::new();
    };
    DEFAULT_ROOTS
        .iter()
        .map(|r| PathBuf::from(&home).join(r))
        .collect()
}

const ROOTS: &str = "roots";
const FONT: &str = "font";
const SIZE: &str = "size";
const THEME: &str = "theme";
const FIRE: &str = "fire";
const COLUMN: &str = "column";

impl Settings {
    /// Le impostazioni salvate in `prefs`; i valori che mancano o non valgono prendono il default.
    pub fn load(prefs: &Prefs) -> Settings {
        let d = Settings::default();
        Settings {
            roots: prefs.get(ROOTS).unwrap_or(d.roots),
            font: prefs
                .get::<String>(FONT)
                .filter(|f| !f.trim().is_empty())
                .unwrap_or(d.font),
            size: prefs
                .get::<f32>(SIZE)
                .filter(|s| (SIZES[0]..=SIZES[SIZES.len() - 1]).contains(s))
                .unwrap_or(d.size),
            theme: prefs.get(THEME).unwrap_or(d.theme),
            fire: prefs.get(FIRE).unwrap_or(d.fire),
            column: prefs
                .get::<f32>(COLUMN)
                .filter(|c| COLUMNS.contains(c))
                .unwrap_or(d.column),
        }
    }

    /// Scrive tutti i campi in `prefs` (e su disco, se `prefs` ha un file).
    pub fn save(&self, prefs: &mut Prefs) -> Result<(), PrefsError> {
        prefs.set(ROOTS, &self.roots)?;
        prefs.set(FONT, &self.font)?;
        prefs.set(SIZE, &self.size)?;
        prefs.set(THEME, &self.theme)?;
        prefs.set(FIRE, &self.fire)?;
        prefs.set(COLUMN, &self.column)
    }

    /// La larghezza dopo quella di adesso in `COLUMNS`, nella direzione `step` (−1 o +1); ferma
    /// ai capi.
    pub fn step_column(&self, step: i32) -> f32 {
        let at = COLUMNS.iter().position(|c| *c == self.column).unwrap_or(0) as i32;
        COLUMNS[(at + step).clamp(0, COLUMNS.len() as i32 - 1) as usize]
    }

    /// Aggiunge `root` se non c'è già. Vero se è cambiato.
    pub fn add_root(&mut self, root: &Path) -> bool {
        if self.roots.iter().any(|r| same(r, root)) {
            return false;
        }
        self.roots.push(root.to_owned());
        true
    }

    /// Toglie `root`. Vero se c'era.
    pub fn remove_root(&mut self, root: &Path) -> bool {
        let before = self.roots.len();
        self.roots.retain(|r| !same(r, root));
        self.roots.len() != before
    }

    /// La dimensione dopo quella di adesso nella direzione `step` (−1 o +1); ferma ai capi.
    pub fn step_size(&self, step: i32) -> f32 {
        let at = SIZES
            .iter()
            .position(|s| *s >= self.size)
            .unwrap_or(SIZES.len() - 1);
        let at = if SIZES[at] > self.size && step > 0 {
            at
        } else {
            (at as i32 + step).clamp(0, SIZES.len() as i32 - 1) as usize
        };
        SIZES[at]
    }
}

/// Due percorsi sono la stessa cartella (anche se uno è canonico e l'altro no).
fn same(a: &Path, b: &Path) -> bool {
    a == b
        || a.canonicalize()
            .ok()
            .zip(b.canonicalize().ok())
            .is_some_and(|(a, b)| a == b)
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::*;

    #[test]
    fn empty_prefs_give_the_defaults() {
        let s = Settings::load(&Prefs::memory());
        assert_eq!(s.font, "Inter");
        assert_eq!(s.size, 15.5);
        assert_eq!(s.theme, ThemePref::System);
        assert!(s.fire, "fuoco acceso di default");
        let home = PathBuf::from(std::env::var_os("HOME").unwrap());
        assert_eq!(s.roots, vec![home.join("Desktop/GitHub")]);
    }

    #[test]
    fn settings_round_trip_through_prefs() {
        let mut prefs = Prefs::memory();
        let s = Settings {
            roots: vec![PathBuf::from("/a"), PathBuf::from("/b c")],
            font: "New York".into(),
            size: 17.0,
            theme: ThemePref::Dark,
            fire: false,
            column: 920.0,
        };
        s.save(&mut prefs).unwrap();
        assert_eq!(Settings::load(&prefs), s);
    }

    #[test]
    fn settings_round_trip_through_a_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("prefs.json");
        let s = Settings {
            theme: ThemePref::Light,
            fire: false,
            ..Settings::default()
        };
        s.save(&mut Prefs::at(&path)).unwrap();
        assert_eq!(
            Settings::load(&Prefs::at(&path)),
            s,
            "persiste fra i riavvii"
        );
    }

    #[test]
    fn bad_values_fall_back_one_by_one() {
        let mut prefs = Prefs::memory();
        prefs.set(SIZE, &200.0f32).unwrap();
        prefs.set(FONT, "").unwrap();
        prefs.set(THEME, "Viola").unwrap();
        prefs.set(FIRE, &false).unwrap();
        let s = Settings::load(&prefs);
        assert_eq!(
            (s.size, s.font.as_str(), s.theme, s.fire),
            (15.5, "Inter", ThemePref::System, false)
        );
    }

    /// La larghezza del testo: 680 di default, passi da 80 fino a 1240 e poi "Intera" (0).
    #[test]
    fn text_width_steps_from_680_to_full() {
        let mut s = Settings::default();
        assert_eq!(s.column, 680.0);
        assert_eq!(COLUMNS[0], 680.0);
        assert_eq!(*COLUMNS.last().unwrap(), COLUMN_FULL);
        assert_eq!(s.step_column(-1), 680.0, "fermo al primo");
        assert_eq!(s.step_column(1), 760.0);
        s.column = 1240.0;
        assert_eq!(s.step_column(1), COLUMN_FULL);
        s.column = COLUMN_FULL;
        assert_eq!(s.step_column(1), COLUMN_FULL, "fermo all'ultimo");
        assert_eq!(s.step_column(-1), 1240.0);
        assert_eq!(column_at(0.0), 680.0);
        assert_eq!(column_at(1.0), COLUMN_FULL);
        assert_eq!(column_at(0.49), 1000.0, "il passo più vicino");
        assert_eq!(column_at(-3.0), 680.0, "sotto 0: il primo");
        assert_eq!(column_at(7.0), COLUMN_FULL, "sopra 1: Intera");
        assert_eq!(column_t(COLUMN_FULL), 1.0);
        assert_eq!(column_t(123.0), 0.0, "fuori passo: il primo");
        assert_eq!(column_label(680.0), "680 pt");
        assert_eq!(column_label(COLUMN_FULL), "Intera");
        assert_eq!(max_width(680.0), Some(680.0));
        assert_eq!(max_width(COLUMN_FULL), None);
    }

    #[test]
    fn text_width_persists_and_bad_values_fall_back() {
        let mut prefs = Prefs::memory();
        let s = Settings { column: COLUMN_FULL, ..Settings::default() };
        s.save(&mut prefs).unwrap();
        assert_eq!(Settings::load(&prefs).column, COLUMN_FULL);
        prefs.set(COLUMN, &700.0f32).unwrap();
        assert_eq!(Settings::load(&prefs).column, 680.0, "non è un passo");
    }

    #[test]
    fn theme_pref_overrides_the_system() {
        assert!(ThemePref::System.dark(true));
        assert!(!ThemePref::System.dark(false));
        assert!(!ThemePref::Light.dark(true));
        assert!(ThemePref::Dark.dark(false));
    }

    #[test]
    fn roots_add_once_and_remove() {
        let mut s = Settings {
            roots: Vec::new(),
            ..Settings::default()
        };
        assert!(s.add_root(Path::new("/x")));
        assert!(!s.add_root(Path::new("/x")), "già c'è");
        assert!(s.remove_root(Path::new("/x")));
        assert!(!s.remove_root(Path::new("/x")));
        assert!(s.roots.is_empty());
    }

    #[test]
    fn size_steps_stop_at_the_ends() {
        let s = |size| Settings {
            size,
            ..Settings::default()
        };
        assert_eq!(s(15.5).step_size(1), 17.0);
        assert_eq!(s(15.5).step_size(-1), 14.0);
        assert_eq!(s(13.0).step_size(-1), 13.0);
        assert_eq!(s(22.0).step_size(1), 22.0);
        assert_eq!(
            s(16.0).step_size(1),
            17.0,
            "un valore fuori scala va al passo dopo"
        );
        assert_eq!(s(16.0).step_size(-1), 15.5);
    }

    /// Aggiungere una cartella dalle impostazioni la passa al watcher (`Watcher::add_root`, con le
    /// sottocartelle): una scrittura sotto arriva come evento.
    #[test]
    fn adding_a_root_reaches_the_watcher() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        std::fs::create_dir_all(root.join("sub")).unwrap();
        let mut app = crate::App::new(
            crate::index::Library::load(Vec::new(), None),
            Settings {
                roots: Vec::new(),
                ..Settings::default()
            },
            Prefs::memory(),
        );
        app.live.start(std::sync::Arc::new(|| {}), Vec::new());
        assert!(app.add_root(&root));
        assert_eq!(app.library.roots(), std::slice::from_ref(&root));
        assert_eq!(app.settings.roots, std::slice::from_ref(&root));
        std::thread::sleep(Duration::from_millis(300));
        let file = root.join("sub/a.md");
        std::fs::write(&file, "# a\n").unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut seen = Vec::new();
        while Instant::now() < deadline
            && !seen.contains(&crate::watch::FsEvent::Changed(file.clone()))
        {
            app.live.drain(&[], Instant::now());
            seen.extend(app.live.drained().iter().cloned());
            std::thread::sleep(Duration::from_millis(16));
        }
        assert!(
            seen.contains(&crate::watch::FsEvent::Changed(file)),
            "{seen:?}"
        );
    }

    /// Togliere una cartella la toglie dal watcher, dalla libreria e da Recenti, subito.
    #[test]
    fn removing_a_root_drops_its_recent_rows() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let (a, b) = (root.join("a"), root.join("b"));
        for d in [&a, &b] {
            std::fs::create_dir_all(d).unwrap();
            std::fs::write(d.join("x.md"), "# x\n").unwrap();
        }
        let entries = crate::index::scan::scan(&[a.clone(), b.clone()]);
        let mut app = crate::App::new(
            crate::index::Library::load(vec![a.clone(), b.clone()], None),
            Settings {
                roots: vec![a.clone(), b.clone()],
                ..Settings::default()
            },
            Prefs::memory(),
        );
        app.library.index = crate::index::Index::from_entries(entries);
        app.live
            .start(std::sync::Arc::new(|| {}), vec![a.clone(), b.clone()]);
        app.sidebar
            .refresh(&app.library.index, &[], std::time::SystemTime::now());
        assert_eq!(app.sidebar.recent.len(), 2);
        assert!(app.remove_root(&a));
        assert_eq!(app.library.roots(), std::slice::from_ref(&b));
        assert_eq!(app.settings.roots, std::slice::from_ref(&b));
        assert!(!app.live.watched_roots().iter().any(|(r, _)| r == &a));
        assert_eq!(
            app.sidebar
                .recent
                .iter()
                .map(|r| r.path.clone())
                .collect::<Vec<_>>(),
            [b.join("x.md")]
        );
    }
}
