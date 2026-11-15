//! Contrasto WCAG dei token di `DESIGN.md` (sezione "Contrasto"). Solo test.
//!
//! La finestra disegna su `Bgra8Unorm` (`srgb_blend`): un colore con alpha si compone sullo sfondo
//! nello spazio sRGB codificato, come nei browser. Per misurare un token con alpha lo si compone
//! così sul suo sfondo, poi si confrontano le luminanze relative.
//!
//! `cargo test --profile perf contrast_table -- --nocapture` stampa la tabella di `DESIGN.md`.

use rune::Color;

use super::theme::{Theme, focus_ring};

/// `fg` composto su `bg` (opaco) in sRGB codificato.
pub fn over(fg: Color, bg: Color) -> Color {
    super::theme::flatten(fg, bg)
}

/// Rapporto di contrasto WCAG 2 fra `fg` composto su `bg` e `bg`.
pub fn ratio(fg: Color, bg: Color) -> f32 {
    let (a, b) = (over(fg, bg).luminance(), bg.luminance());
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

/// Il fondo della sidebar misurato nella finestra vera, vetro di sistema più tinta `glass` al 60 %
/// (14/11/2026, media di 40×40 px; di due misure la più difficile; DESIGN.md "Vetro vero al 60 %"):
/// `[scuro, chiaro]`, ognuno sul fondo peggiore della scrivania (bianco nel tema scuro, nero nel
/// chiaro) e sullo sfondo Sonoma.
pub const GLASS_GROUNDS: [[&str; 2]; 2] = [["#3e3f40", "#374618"], ["#999a9c", "#becb9d"]];

/// I fondi su cui si misura: il pannello (anche sotto `sel`) e il vetro misurato `glass` (uno di
/// `GLASS_GROUNDS`), anche sotto `sel`. In ordine: panel, panel + sel, vetro, vetro + sel.
pub fn grounds(t: &Theme, glass: Color) -> [Color; 4] {
    [t.panel, over(t.sel, t.panel), glass, over(t.sel, glass)]
}

/// I fondi del vetro misurati per il tema `t`.
pub fn glass_grounds(t: &Theme) -> [Color; 2] {
    GLASS_GROUNDS[usize::from(!t.dark)].map(|h| rune::color::hex(h).expect("esadecimale"))
}

/// Una riga della tabella: cosa si misura, il colore e lo sfondo, e il minimo richiesto.
pub struct Row {
    pub what: &'static str,
    pub fg: Color,
    pub bg: Color,
    pub min: f32,
}

/// I colori che non sono token: l'anello del risultato attivo di Cerca, il fuoco della tastiera e
/// il nome doppio della riga scelta (seconda riga di Recenti e di ⌘P).
pub struct Rings {
    pub active: Color,
    pub focus: Color,
    pub chosen: Color,
    /// Il testo piccolo dentro le righe premibili della sidebar, che prendono `sel` in hover: il
    /// tempo di Recenti, il nome doppio, il link "Apri Impostazioni di Sistema"
    /// (`sidebar::row_meta`).
    pub row_meta: Color,
}

impl Rings {
    /// Quelli dell'app: anello attivo in `fg2` (`markdown_style`), fuoco da [`focus_ring`] con
    /// l'accento di sistema predefinito (blu).
    pub fn of(t: &Theme) -> Rings {
        let glass = t.on_glass();
        Rings { active: t.fg2, focus: focus_ring(rune::SystemAccent::DEFAULT.focus(t.dark), t.dark), chosen: glass.fg2, row_meta: super::sidebar::row_meta(&glass) }
    }
}

/// I segni e i testi del tema `t`, ognuno sul suo sfondo, con il minimo (4,5 testo, 3 segni con
/// significato, 1 le linee solo decorative e i riempimenti). `glass` è il fondo del vetro misurato
/// (`glass_grounds`); sul vetro il testo usa `Theme::on_glass`.
pub fn rows(t: &Theme, rings: &Rings, glass: Color) -> Vec<Row> {
    let [panel, panel_sel, glass, glass_sel] = grounds(t, glass);
    let g = t.on_glass();
    let mark = 3.0;
    let row = |what, fg, bg, min| Row { what, fg, bg, min };
    vec![
        row("fg su panel", t.fg, panel, 4.5),
        row("body su panel", t.body, panel, 4.5),
        row("fg2 su panel", t.fg2, panel, 4.5),
        row("fg3 su panel", t.fg3, panel, 4.5),
        row("fg su vetro", t.fg, glass, 4.5),
        row("fg3 su vetro", g.fg3, glass, 4.5),
        row("fg2 su vetro", g.fg2, glass, 4.5),
        row("fg3 su vetro + sel (hover)", g.fg3, glass_sel, 4.5),
        row("fg2 su vetro + sel (hover)", g.fg2, glass_sel, 4.5),
        row("nome doppio della riga scelta su vetro + sel (fg3 prima, fg2 dopo)", rings.chosen, glass_sel, 4.5),
        row("testo piccolo delle righe premibili (tempo di Recenti, nome doppio, link) su vetro + sel (hover)", rings.row_meta, glass_sel, 4.5),
        row("Recenti, seconda riga (cartella o repo e ora) su vetro", rings.row_meta, glass, 4.5),
        row("Recenti, seconda riga (cartella o repo e ora) su vetro + sel (hover)", rings.row_meta, glass_sel, 4.5),
        row("Recenti, nome (fg) su vetro + sel (hover)", t.fg, glass_sel, 4.5),
        row("titolo dell'albero: nome del repo (fg) su vetro", t.fg, glass, 4.5),
        row("titolo dell'albero: branch (fg3) su vetro", g.fg3, glass, 4.5),
        row("titolo dell'albero: icone repo e branch (fg3) su vetro", g.fg3, glass, mark),
        row("cartelle salvate: nome della radice (fg) su vetro + sel (hover)", t.fg, glass_sel, 4.5),
        row("cartelle salvate: nome della radice non salvata (fg2) su vetro", rings.row_meta, glass, 4.5),
        row("cartelle salvate: nome della radice non salvata (fg2) su vetro + sel (hover)", rings.row_meta, glass_sel, 4.5),
        row("cartelle salvate: branch della radice (fg3) su vetro + sel (hover)", g.fg3, glass_sel, 4.5),
        row("cartelle salvate: freccia, icone repo, cartella, branch e + (fg3) su vetro + sel (hover)", g.fg3, glass_sel, mark),
        row("cartelle salvate: \"Salva\" e \"Togli\" (fg2) su vetro", rings.row_meta, glass, 4.5),
        row("cartelle salvate: \"Salva\" e \"Togli\" (fg2) su vetro + sel (hover della riga)", rings.row_meta, glass_sel, 4.5),
        row("cartelle salvate: \"Aggiungi cartella\" (fg2) su vetro", rings.row_meta, glass, 4.5),
        row("cartelle salvate: \"Aggiungi cartella\" (fg2) su vetro + sel (hover)", rings.row_meta, glass_sel, 4.5),
        row("cartelle salvate: bordo di \"Salva\" e \"Togli\" (line) su vetro, decorativo", t.line, glass, 1.0),
        row("titolo evidenziato dell'indice (fg) su panel + sel (hover)", t.fg, panel_sel, 4.5),
        row("titoli dell'indice (fg2) su panel + sel (hover)", t.fg2, panel_sel, 4.5),
        row("mark su panel (anello Cerca, segno cambiato)", t.mark, panel, mark),
        row("mark su sel (anello Cerca, lato interno)", t.mark, panel_sel, mark),
        row("anello Cerca attivo su sel", rings.active, panel_sel, 3.0),
        row("fuoco tastiera su vetro", rings.focus, glass, mark),
        row("fuoco tastiera su panel", rings.focus, panel, mark),
        // Impostazioni (Cmd+,, `ui::settings`), sul pannello.
        row("impostazioni: scelta fatta (fg) su panel + sel", t.fg, panel_sel, 4.5),
        row("impostazioni: spunta della scelta (fg) su panel + sel", t.fg, panel_sel, mark),
        row("impostazioni: scelte e bottoni (fg2) su panel + sel (hover)", t.fg2, panel_sel, 4.5),
        row("impostazioni: casella spenta, bordo fg3 su panel", t.fg3, panel, mark),
        row("impostazioni: casella accesa, fg2 su panel", t.fg2, panel, mark),
        row("impostazioni: spunta della casella (panel) su fg2", t.panel, t.fg2, mark),
        row("impostazioni: icona cartella (fg3) su panel", t.fg3, panel, mark),
        row("impostazioni: casella spenta, bordo fg3 su panel + sel (hover)", t.fg3, panel_sel, mark),
        row("impostazioni: casella accesa, fg2 su panel + sel (hover)", t.fg2, panel_sel, mark),
        row("impostazioni: interruttore del fuoco (fg2) su panel + sel (hover)", t.fg2, panel_sel, 4.5),
        row("impostazioni: \"Già osservata\" (fg2) su panel", t.fg2, panel, 4.5),
        row("etichetta del fuoco spento (fg2) su panel", t.fg2, panel, 4.5),
        row("frontmatter: testo (fg2) su panel + sel a metà", t.meta().text, over(t.meta().fill, panel), 4.5),
        row("× della scheda (fg2) su panel + sel (scheda davanti o in hover)", t.fg2, panel_sel, mark),
        row("× della scheda (fg2) in hover su panel + sel + sel", t.fg2, over(t.sel, panel_sel), mark),
        row("pulsante ↔ della larghezza (fg2) su panel", t.fg2, panel, mark),
        row("pulsante ↔ della larghezza (fg2) su panel + sel (hover, popover aperto)", t.fg2, panel_sel, mark),
        row("popover della larghezza: titolo (fg2) su panel", t.fg2, panel, 4.5),
        row("popover della larghezza: valore (fg) su panel", t.fg, panel, 4.5),
        row("popover della larghezza: scorciatoie (fg3) su panel", t.fg3, panel, 4.5),
        row("popover della larghezza: binario (fg3) su panel", t.fg3, panel, mark),
        // Scorciatoie (pulsante ⌘ e popover, `ui::shortcuts`), sul pannello.
        row("pulsante ⌘ delle scorciatoie (fg2) su panel", t.fg2, panel, mark),
        row("pulsante ⌘ delle scorciatoie (fg2) su panel + sel (hover, popover aperto)", t.fg2, panel_sel, mark),
        row("popover delle scorciatoie: titolo e righe (fg2) su panel", t.fg2, panel, 4.5),
        row("popover delle scorciatoie: gruppi (fg3) su panel", t.fg3, panel, 4.5),
        row("popover delle scorciatoie: tasti (fg2) su panel", t.fg2, panel, 4.5),
        row("popover delle scorciatoie: bordo dei tasti (line) su panel, decorativo", t.line, panel, 1.0),
        // ⌘P compatto (`ui::quick_open::panel`), sul pannello: righe con hover e scelta `sel`.
        row("⌘P: nome (fg) su panel + sel (scelta, hover)", t.fg, panel_sel, 4.5),
        row("⌘P: repo o cartella e ora (fg2) su panel", t.fg2, panel, 4.5),
        row("⌘P: repo o cartella e ora (fg2) su panel + sel (scelta, hover)", t.fg2, panel_sel, 4.5),
        row("⌘P: icona del file (fg2) su panel + sel", t.fg2, panel_sel, mark),
        row("⌘P: tasti in fondo (fg2) e parole (fg3) su panel", t.fg3, panel, 4.5),
        row("⌘P: segnaposto del campo (fg2) su panel", t.fg2, panel, 4.5),
        // Caret "sta scrivendo" (`fx::caret`), sul pannello: al passo più tenue del battito, anche
        // sopra una selezione.
        row("caret sta scrivendo (fg al passo più tenue) su panel", crate::fx::caret::dimmest(t), panel, 4.5),
        row("caret sta scrivendo (fg al passo più tenue) su panel + sel", crate::fx::caret::dimmest(t), panel_sel, 4.5),
        row("line su panel (linee decorative)", t.line, panel, 1.0),
        row("line su vetro (bordo campi, decorativo)", t.line, glass, 1.0),
        row("sel su panel (selezione, hover)", t.sel, panel, 1.0),
        row("sel accanto al codice in linea", over(t.sel, panel), over(inline_code(t), panel), 1.0),
    ]
}

/// Il riempimento del codice in linea: `MarkdownStyle::neutral` usa il testo del kit all'8 %.
pub fn inline_code(t: &Theme) -> Color {
    rune::tok::theme(t.dark).text.alpha(0.08)
}

#[cfg(test)]
mod tests {
    use rune::oklcha;

    use super::*;

    /// Gli anelli prima del fix: attivo in `fg3`, fuoco del sistema al 50 %.
    fn rings_before(dark: bool) -> Rings {
        Rings { active: before(dark).fg3, focus: rune::SystemAccent::DEFAULT.focus(dark), chosen: before(dark).fg3, row_meta: before(dark).fg3 }
    }

    /// Prima e dopo sul vetro misurato `k` (0: scrivania peggiore, 1: Sonoma).
    fn table(dark: bool, k: usize) -> (Vec<Row>, Vec<Row>) {
        let now = Theme::for_mode(dark);
        let glass = glass_grounds(&now)[k];
        (rows(&before(dark), &rings_before(dark), glass), rows(&now, &Rings::of(&now), glass))
    }

    /// I token prima del fix del 14/11/2026 (DESIGN.md, "Contrasto", colonna "prima").
    fn before(dark: bool) -> Theme {
        let now = Theme::for_mode(dark);
        if dark {
            Theme {
                fg2: oklcha(0.80, 0.006, 270.0, 1.0),
                fg3: oklcha(0.69, 0.008, 270.0, 1.0),
                line: oklcha(1.0, 0.0, 0.0, 0.08),
                sel: oklcha(1.0, 0.0, 0.0, 0.09),
                mark: oklcha(1.0, 0.0, 0.0, 0.08),
                ..now
            }
        } else {
            Theme { mark: oklcha(0.0, 0.0, 0.0, 0.09), ..now }
        }
    }

    #[test]
    fn contrast_table() {
        for dark in [true, false] {
            for (k, ground) in GLASS_GROUNDS[usize::from(!dark)].iter().enumerate() {
                println!("{} {ground}", if dark { "scuro" } else { "chiaro" });
                let (old, new) = table(dark, k);
                for (b, a) in old.iter().zip(new) {
                    println!("| {} | {:.2} | {:.2} | {} |", a.what, ratio(b.fg, b.bg), ratio(a.fg, a.bg), a.min);
                }
            }
        }
    }

    #[test]
    fn every_row_meets_its_minimum() {
        for dark in [true, false] {
            for k in 0..2 {
                for r in table(dark, k).1 {
                    let c = ratio(r.fg, r.bg);
                    assert!(c >= r.min, "{} ({}, vetro {k}): {c:.2} < {}", r.what, if dark { "scuro" } else { "chiaro" }, r.min);
                }
            }
        }
    }

    #[test]
    fn light_theme_gets_no_worse() {
        let (old, new) = table(false, 0);
        for (b, a) in old.iter().zip(new) {
            assert!(ratio(a.fg, a.bg) >= ratio(b.fg, b.bg) - 1e-4, "{}", a.what);
        }
    }

    /// Le linee decorative si vedono sul Retina e `sel` si stacca dal codice in linea più di prima.
    #[test]
    fn dark_lines_and_selection_are_visible() {
        let (t, old) = (Theme::for_mode(true), before(true));
        assert!(ratio(t.line, t.panel) >= 1.35, "line ≥ 12 % circa");
        let code = over(inline_code(&t), t.panel);
        assert!(ratio(t.sel, code) > ratio(old.sel, code) + 0.05);
    }

    /// Il risultato attivo di Cerca resta più marcato degli altri.
    #[test]
    fn active_find_ring_is_stronger() {
        for dark in [true, false] {
            let t = Theme::for_mode(dark);
            assert!(ratio(Rings::of(&t).active, t.panel) > ratio(t.mark, t.panel) + 2.0);
        }
    }

    /// Sul vetro la gerarchia resta: `fg` > `fg2` > `fg3` in contrasto, sui due fondi misurati.
    #[test]
    fn glass_text_keeps_its_hierarchy() {
        for dark in [true, false] {
            let t = Theme::for_mode(dark);
            let g = t.on_glass();
            for bg in glass_grounds(&t) {
                assert!(ratio(t.fg, bg) > ratio(g.fg2, bg) && ratio(g.fg2, bg) > ratio(g.fg3, bg) + 0.3, "scuro {dark}");
            }
            assert_eq!((g.fg, g.body, g.panel, g.glass), (t.fg, t.body, t.panel, t.glass), "solo fg2 e fg3 cambiano");
        }
    }

    #[test]
    fn blending_matches_known_values() {
        assert!((ratio(Color::WHITE, Color::BLACK) - 21.0).abs() < 0.01);
        assert_eq!(ratio(Color::WHITE.alpha(0.0), Color::BLACK), 1.0);
        // Bianco al 50 % sul nero, in sRGB codificato: grigio 0,5 → luminanza 0,214.
        assert!((over(Color::WHITE.alpha(0.5), Color::BLACK).luminance() - 0.214).abs() < 0.001);
    }
}
