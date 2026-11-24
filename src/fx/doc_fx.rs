//! Full forge: the document's rune-fx effects (docs/decisions/2026-11-18-rune-fx-markdown.md,
//! directions 1–7 of the mockup). Six are on by default (docs/decisions/2026-11-18-full-forge-effects.md):
//! steel, forged, engraved, ember, glass and tasks; electric is a Settings switch, off by default.
//!
//! `RUNEMD_FORGE` stays as a lab override: `off` (or `0`, `none`) turns every direction off, one
//! name alone (`glass`), several (`steel,ember`) or all (`all` or `1`): [`Directions`]. Off, the
//! view builds exactly the tree it built before.
//!
//! Metal frames are rare (constraint 3): code, tables, callouts, front matter and images get a quiet
//! 1 pt edge in `line`, drawn by Rune, not rune-fx; only the block kind in [`METAL_BLOCK`] (none by default; the lab sets one) keeps metal.
//!
//! The view puts a [`slot`] on the element a [`Piece`] dresses (a code card, a callout, a rule, a
//! task bead…); the compositor turns each slot into the layer of [`layer`] on its rect. Every piece
//! but the charged edge is still: drawn once per frame, it never holds the display link. The
//! charged edge moves with the metal's `ring::METAL_FRAME` wake-ups (paced, no view run).

use rune::{Color, El};
use rune_fx::{DitherParams, DitherPattern, ElectricParams, FillInput, FireParams, FireStyle, FlutedGlassParams, Fx, FxBlend, FxLayer, LiquidMetalParams, Rect, Rgb, Shape, Stops};

use super::Mark;
use crate::ui::theme::Theme;

/// Which forge directions are on (the defaults, or `RUNEMD_FORGE`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Directions {
    /// 1 · Steel: a still gunmetal edge on code, frontmatter and callouts; the rule engraved.
    pub steel: bool,
    /// 2 · Forged: the gunmetal edge, wider, on tables and images too.
    pub forged: bool,
    /// 3 · Engraved: liquid chrome on the rim of every heading's letters; the rule a chrome rod.
    pub engraved: bool,
    /// 4 · Ember: links underlined in ember; the rule a still strip of fire.
    pub ember: bool,
    /// 5 · Glass: dithered callout bands, fluted glass quotes, glass image captions.
    pub glass: bool,
    /// 6 · Tasks: chrome task beads and a metal progress tube.
    pub tasks: bool,
    /// 7 · Electric: a charged edge on Warning and Caution callouts.
    pub electric: bool,
}

impl Directions {
    /// The names `RUNEMD_FORGE` takes, in the mockup's order.
    pub const NAMES: [&'static str; 7] = ["steel", "forged", "engraved", "ember", "glass", "tasks", "electric"];

    /// Every direction on: "Full forge".
    pub const ALL: Directions = Directions { steel: true, forged: true, engraved: true, ember: true, glass: true, tasks: true, electric: true };

    /// The six on by default: every direction but electric (a Settings switch, `with_electric`).
    pub const DEFAULT: Directions = Directions { electric: false, ..Directions::ALL };

    /// `RUNEMD_FORGE` as read (a lab override): missing or empty is `None` (the defaults); `off`,
    /// `0` or `none` is every direction off; `all` or `1` is every direction; else a comma list of
    /// [`Directions::NAMES`] (case and spaces ignored; unknown names are skipped).
    pub fn parse(value: Option<&str>) -> Option<Directions> {
        let v = value.map(str::trim).filter(|v| !v.is_empty())?;
        if v.eq_ignore_ascii_case("all") || v == "1" {
            return Some(Directions::ALL);
        }
        if v.eq_ignore_ascii_case("off") || v.eq_ignore_ascii_case("none") || v == "0" {
            return Some(Directions::default());
        }
        let mut d = Directions::default();
        for name in v.split(',').map(|n| n.trim().to_ascii_lowercase()) {
            match name.as_str() {
                "steel" => d.steel = true,
                "forged" => d.forged = true,
                "engraved" => d.engraved = true,
                "ember" => d.ember = true,
                "glass" => d.glass = true,
                "tasks" => d.tasks = true,
                "electric" => d.electric = true,
                "" => {}
                other => eprintln!("runemd: RUNEMD_FORGE: no direction `{other}` (one of {}, all or off)", Directions::NAMES.join(", ")),
            }
        }
        Some(d)
    }

    /// The lab override from the environment, read once at startup.
    pub fn from_env() -> Option<Directions> {
        Directions::parse(std::env::var("RUNEMD_FORGE").ok().as_deref())
    }

    /// What is on: the lab override as set, else the defaults with the Settings switch `electric`.
    pub fn effective(lab: Option<Directions>, electric: bool) -> Directions {
        lab.unwrap_or(Directions { electric, ..Directions::DEFAULT })
    }

    /// At least one direction is on.
    pub fn any(self) -> bool {
        self != Directions::default()
    }

    /// Every direction is on: the edges are chrome, not gunmetal (the mockup's "Full forge").
    pub fn full(self) -> bool {
        self == Directions::ALL
    }

    /// The metal edge on code, frontmatter and callouts: its width, or none.
    pub fn edge(self) -> Option<Piece> {
        let chrome = self.full();
        match (self.forged, self.steel) {
            (true, _) => Some(Piece::Edge { width: FORGED_W, chrome }),
            (false, true) => Some(Piece::Edge { width: STEEL_W, chrome }),
            _ => None,
        }
    }

    /// The edge on tables and images: only Forged has it.
    pub fn wide_edge(self) -> Option<Piece> {
        self.forged.then_some(Piece::Edge { width: FORGED_W, chrome: self.full() })
    }

    /// The frame of a block of `kind`: the metal edge only if `kind` is the `metal` one, a quiet
    /// edge for the others, none when no direction edges that kind.
    pub fn frame(self, kind: BlockKind, metal: Option<BlockKind>) -> Option<Frame> {
        let piece = match kind {
            BlockKind::Table | BlockKind::Image => self.wide_edge(),
            BlockKind::Code | BlockKind::Callout | BlockKind::Frontmatter => self.edge(),
        }?;
        Some(if metal == Some(kind) { Frame::Metal(piece) } else { Frame::Quiet })
    }

    /// What a thematic break (`---`) becomes, or `None` for the kit's hairline. With several
    /// directions on, the chrome rod wins over the fire, which wins over the engraved line.
    pub fn rule(self) -> Option<Rule> {
        if self.engraved {
            Some(Rule::Rod)
        } else if self.ember {
            Some(Rule::Fire)
        } else if self.steel {
            Some(Rule::Engraved)
        } else {
            None
        }
    }
}

/// The block kinds a forged edge frames.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockKind {
    Code,
    Callout,
    Table,
    Frontmatter,
    Image,
}

impl BlockKind {
    /// The names `RUNEMD_METAL_BLOCK` takes (lab only: the variant shots).
    fn parse(name: &str) -> Option<Option<BlockKind>> {
        Some(match name.trim().to_ascii_lowercase().as_str() {
            "none" | "off" | "" => None,
            "code" => Some(BlockKind::Code),
            "callout" | "callouts" => Some(BlockKind::Callout),
            "table" | "tables" => Some(BlockKind::Table),
            "frontmatter" => Some(BlockKind::Frontmatter),
            "image" | "images" => Some(BlockKind::Image),
            _ => return None,
        })
    }
}

/// The one block kind that keeps a metal frame (constraint 3: at most one): none. Code blocks had
/// it (docs/decisions/2026-11-18-full-forge-round-2.md, constraint 1) until Matteo asked for the
/// table's border on them (review R2, docs/decisions/2026-11-23-t3-paths-review.md): every block
/// gets the quiet edge.
pub const METAL_BLOCK: Option<BlockKind> = None;

/// [`METAL_BLOCK`], or `RUNEMD_METAL_BLOCK` (`none`, `code`, `callouts`, `tables`, `frontmatter`,
/// `images`) in the lab, read once at startup.
pub fn metal_block() -> Option<BlockKind> {
    metal_block_from(std::env::var("RUNEMD_METAL_BLOCK").ok().as_deref())
}

fn metal_block_from(value: Option<&str>) -> Option<BlockKind> {
    match value.map(BlockKind::parse) {
        None => METAL_BLOCK,
        Some(Some(kind)) => kind,
        Some(None) => {
            eprintln!("runemd: RUNEMD_METAL_BLOCK: one of none, code, callouts, tables, frontmatter, images");
            METAL_BLOCK
        }
    }
}

/// How a block is framed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Frame {
    /// The still metal edge (a rune-fx slot).
    Metal(Piece),
    /// A [`QUIET_W`] outline in the theme's `line`: Rune draws it, no effect layer.
    Quiet,
}

/// The quiet edge, pt.
pub const QUIET_W: f32 = 1.0;

/// Width of Steel's edge, pt (mockup 1: 1.25 px of gunmetal).
pub const STEEL_W: f32 = 1.25;
/// Width of Forged's edge, pt (mockup 2: the moving metal's 2 pt, still).
pub const FORGED_W: f32 = 2.0;
/// How far the charged edge draws outside its callout, pt (its glow).
pub const ELECTRIC_BLEED: f32 = 8.0;

/// A thematic break, forged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rule {
    /// Steel: a dark line with a light line under it, cut into the page (no effect).
    Engraved,
    /// Engraved: a 3 pt chrome rod.
    Rod,
    /// Ember: an 18 pt strip of still fire.
    Fire,
}

/// What a slot draws.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Piece {
    /// A still metal band `width` outside the element: gunmetal, or chrome in Full forge.
    Edge { width: f32, chrome: bool },
    /// A chrome rod filling the element (a rule).
    Rod,
    /// A still strip of fire filling the element (a rule).
    Fire,
    /// A dither band filling the element, `density` 0..1 of ink (a callout's side).
    Dither { density: f32 },
    /// Fluted glass over what is behind the element (a quote).
    Fluted,
    /// Frosted fluted glass over the image under the element (an image caption).
    Caption,
    /// A task bead: chrome when checked, a gunmetal ring when not.
    Bead { checked: bool },
    /// A gunmetal tube filling the element (task progress).
    Tube,
    /// The chrome inside the tube.
    TubeFill,
    /// A charged edge around the element: amber, or red with `red` (Warning, Caution).
    Electric { red: bool },
    /// Liquid chrome on the rim of a heading's letters: it fills the element, and the compositor clips
    /// it to a band inside the edge of the glyphs drawn before the slot (`FillParams::masked_rim`).
    /// It moves on `ring::METAL_FRAME`.
    Chrome,
}

impl Piece {
    fn code(self) -> (f32, f32) {
        match self {
            Piece::Edge { width, chrome } => (1.0, if chrome { -width } else { width }),
            Piece::Rod => (2.0, 0.0),
            Piece::Fire => (3.0, 0.0),
            Piece::Dither { density } => (4.0, density),
            Piece::Fluted => (5.0, 0.0),
            Piece::Caption => (6.0, 0.0),
            Piece::Bead { checked } => (7.0, f32::from(u8::from(checked))),
            Piece::Tube => (8.0, 0.0),
            Piece::TubeFill => (9.0, 0.0),
            Piece::Electric { red } => (10.0, f32::from(u8::from(red))),
            Piece::Chrome => (11.0, 0.0),
        }
    }

    fn decode(kind: f32, p: f32) -> Option<Piece> {
        Some(match kind.round() as i32 {
            1 => Piece::Edge { width: p.abs(), chrome: p < 0.0 },
            2 => Piece::Rod,
            3 => Piece::Fire,
            4 => Piece::Dither { density: p },
            5 => Piece::Fluted,
            6 => Piece::Caption,
            7 => Piece::Bead { checked: p > 0.5 },
            8 => Piece::Tube,
            9 => Piece::TubeFill,
            10 => Piece::Electric { red: p > 0.5 },
            11 => Piece::Chrome,
            _ => return None,
        })
    }

    /// How far it draws outside its element, pt.
    pub fn bleed(self) -> f32 {
        match self {
            Piece::Edge { width, .. } => width,
            Piece::Electric { .. } => ELECTRIC_BLEED,
            _ => 0.0,
        }
    }

    /// It reads the pixels behind it (glass): its slot asks for a backdrop copy.
    pub fn backdrop(self) -> bool {
        matches!(self, Piece::Fluted | Piece::Caption)
    }
}

/// The theme and motion a slot was built with.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Look {
    pub dark: bool,
    pub reduced: bool,
}

/// `effect_data` of a slot: kind, its parameter, the theme, "reduce motion".
pub fn data(piece: Piece, look: Look) -> [f32; 4] {
    let (kind, p) = piece.code();
    [kind, p, f32::from(u8::from(!look.dark)), f32::from(u8::from(look.reduced))]
}

/// From the `effect_data` of a [`slot`]: the piece and the look, or `None` for another payload.
pub fn read(data: [f32; 4]) -> Option<(Piece, Look)> {
    Some((Piece::decode(data[0], data[1])?, Look { dark: data[2] < 0.5, reduced: data[3] > 0.5 }))
}

/// Makes `el` the slot of `piece`: the compositor paints [`layer`] on its rect and radius.
pub fn slot(el: El, piece: Piece, look: Look) -> El {
    let el = el.effect(Mark::DocFx.id()).effect_data(data(piece, look));
    let el = if piece.bleed() > 0.0 { el.effect_bleed(piece.bleed()) } else { el };
    if piece.backdrop() || blend(piece, look).needs_backdrop() { el.effect_backdrop(true) } else { el }
}

/// How a piece composites: the charged edge multiplies on the light theme, where its additive
/// glow is light on light and vanishes; everything else is plain source-over.
pub fn blend(piece: Piece, look: Look) -> FxBlend {
    match piece {
        Piece::Electric { .. } if !look.dark => FxBlend::Multiply,
        _ => FxBlend::Normal,
    }
}

/// [`blend`] from a slot's `effect_data`.
pub fn blend_of(data: [f32; 4]) -> FxBlend {
    read(data).map_or(FxBlend::Normal, |(piece, look)| blend(piece, look))
}

/// Gunmetal: the chrome, darker, with no flow (mockups 1 and 2).
pub fn gunmetal(dark: bool) -> LiquidMetalParams {
    let chrome = LiquidMetalParams::chrome();
    let (tint, light) = if dark { (0x7a808a, 0xc4c8ce) } else { (0x5d626b, 0x9da2aa) };
    LiquidMetalParams { tint: rune_fx::rgb(tint), light: rune_fx::rgb(light), flow: 0.0, ..chrome }
}

/// The still chrome of rods, beads and Full forge's edges: the moving edge's metal (`ring::metal`)
/// with no flow.
pub fn chrome(dark: bool) -> LiquidMetalParams {
    LiquidMetalParams { flow: 0.0, ..super::ring::metal(dark) }
}

/// The heading chrome (round 2, mockup "Release 0.3 plan"): the moving chrome of the edges in the
/// dark theme, where the letters read light (text-mode remap `0.38 + 0.62 L`); in the light theme
/// a dark chrome, so the letters keep their contrast on the light panel (remap `0.6 L`).
/// Each glyph's rim is `HEADING_BEVEL` pt wide (the mask's slope, `FillParams::masked`).
pub fn heading_metal(dark: bool) -> LiquidMetalParams {
    let chrome =
        LiquidMetalParams { bevel: HEADING_BEVEL, speed: HEADING_SPEED, ..LiquidMetalParams::chrome() };
    if dark {
        return chrome;
    }
    LiquidMetalParams { tint: rune_fx::rgb(0xb4b9c0), light: rune_fx::rgb(0xc8ccd2), contrast: 0.7, ..chrome }
}

/// How fast the heading chrome flows at rest. The edges' 0.35 reads as still on a heading, which
/// moved only while scrolling (feedback F4); Matteo picked "lento", 1.2, from the 2026-11-23 videos.
pub const HEADING_SPEED: f32 = 1.2;

/// The rolled rim of a heading's letters, pt.
pub const HEADING_BEVEL: f32 = 1.0;

/// The chrome is only a band this wide (pt) inside each glyph's edge; the letter's inside keeps
/// the text colour (M1, Matteo chose the outline over a full chrome fill or none).
pub const HEADING_RIM: f32 = 0.9;

/// A piece is a heading's chrome: the compositor builds its glyph mask.
pub fn is_chrome(data: [f32; 4]) -> bool {
    matches!(read(data), Some((Piece::Chrome, _)))
}

/// The ember of links and the charged edge's amber (mockup 4): ≥ 3:1 on the panel in both themes.
pub fn ember(dark: bool) -> Color {
    rune::color::hex(if dark { "#ff8a3d" } else { "#c2410c" }).unwrap_or(Color { r: 1.0, g: 0.4, b: 0.1, a: 1.0 })
}

/// A colour as rune-fx's linear RGB.
fn rgb(c: Color) -> Rgb {
    Rgb([c.r, c.g, c.b])
}

/// The layer of a slot with `effect_data` `data` on `rect` (pt) with corner `radius`, at Rune's clock
/// `clock` (s), and whether it is paced (the charged edge: it moves on `ring::METAL_FRAME`, not the
/// display link). Still while `active` is false (inactive window) or with "reduce motion".
pub fn layer(rect: Rect, radius: f32, data: [f32; 4], clock: f64, active: bool) -> Option<(FxLayer, bool)> {
    let (piece, look) = read(data)?;
    let theme = Theme::for_mode(look.dark);
    let still = |l: FxLayer| l.at(0.0).reduced(true);
    let layer = match piece {
        Piece::Edge { width, chrome: c } => {
            let metal = if c { chrome(look.dark) } else { gunmetal(look.dark) };
            let r = rect.inflate(width);
            still(Fx::fill(metal.border(width)).on(Shape::rounded(r.x, r.y, r.w, r.h, radius + width)))
        }
        Piece::Rod => still(Fx::fill(chrome(look.dark)).on(Shape::capsule(rect.x, rect.y, rect.w, rect.h))),
        Piece::Fire => {
            let fire = FireParams { style: FireStyle::Top, height: rect.h, embers: 0, ..FireParams::default() };
            still(Fx::fire(fire).on(Shape::rounded(rect.x, rect.y, rect.w, rect.h, 0.0)).seed(7))
        }
        Piece::Dither { density } => {
            let fill = DitherParams { pattern: DitherPattern::Bayer4, input: FillInput::Linear, colors: Stops::new(&[rgb(theme.panel), rgb(theme.fg2)]), pixel: 1.0, angle: std::f32::consts::FRAC_PI_2, bias: density * 2.0 - 1.0, contrast: 0.0, ..DitherParams::bayer() };
            still(Fx::fill(fill).on(Shape::rounded(rect.x, rect.y, rect.w, rect.h, radius)))
        }
        Piece::Fluted => {
            let glass = FlutedGlassParams { rib: 10.0, refraction: 3.0, frost: 0.0, highlight: 0.35, tint: rgb(theme.fg), tint_amount: 0.04, ..FlutedGlassParams::reeded() };
            still(Fx::fill(glass).on(Shape::rounded(rect.x, rect.y, rect.w, rect.h, radius)))
        }
        Piece::Caption => {
            let glass = FlutedGlassParams { rib: 6.0, refraction: 5.0, frost: 0.4, highlight: 0.4, tint: rune_fx::rgb(0x0b0d12), tint_amount: 0.45, ..FlutedGlassParams::reeded() };
            still(Fx::fill(glass).on(Shape::rounded(rect.x, rect.y, rect.w, rect.h, radius)))
        }
        Piece::Bead { checked: true } => still(Fx::fill(chrome(look.dark).border(BEAD_RING)).on(Shape::rounded(rect.x, rect.y, rect.w, rect.h, radius))),
        Piece::Bead { checked: false } => still(Fx::fill(gunmetal(look.dark).border(BEAD_RING)).on(Shape::rounded(rect.x, rect.y, rect.w, rect.h, radius))),
        Piece::Tube => still(Fx::fill(gunmetal(look.dark)).on(Shape::capsule(rect.x, rect.y, rect.w, rect.h))),
        Piece::TubeFill => still(Fx::fill(chrome(look.dark)).on(Shape::capsule(rect.x, rect.y, rect.w, rect.h))),
        Piece::Electric { red } => {
            // Light: the dark theme's glow is light on light and vanishes, so a deeper arc with a
            // dark core (ember's light value for Warning).
            let (color, core) = match (red, look.dark) {
                (false, true) => (0xffb347, 0xfff4e0),
                (true, true) => (0xff5a4f, 0xfff4e0),
                (false, false) => (0xc2410c, 0x7c2d12),
                (true, false) => (0xdc2626, 0x7f1d1d),
            };
            let color = rune_fx::rgb(color);
            let p = ElectricParams { color, core: rune_fx::rgb(core), cool: color, ..ElectricParams::charged() };
            let frozen = look.reduced || !active;
            // The clock wraps once an hour, like the calm edge's liquid time (f32 precision).
            let t = if frozen { 0.0 } else { clock.max(0.0).rem_euclid(3600.0) as f32 };
            let l = Fx::electric(p).on(Shape::rounded(rect.x, rect.y, rect.w, rect.h, radius)).at(t).reduced(frozen);
            return Some((l, true));
        }
        Piece::Chrome => {
            let frozen = look.reduced || !active;
            let t = if frozen { 0.0 } else { clock.max(0.0).rem_euclid(3600.0) as f32 };
            let fill = rune_fx::FillParams::from(heading_metal(look.dark)).masked_rim(HEADING_RIM);
            let l = Fx::fill(fill).on(Shape::rounded(rect.x, rect.y, rect.w, rect.h, 0.0)).at(t).reduced(frozen);
            return Some((l, true));
        }
    };
    Some((layer, false))
}

/// The ring of an unchecked bead, pt (the kit's checkbox border is 1).
pub const BEAD_RING: f32 = 1.25;

/// A thematic break as written: three or more `-`, `*` or `_` (the same one), spaces allowed.
pub fn is_rule(source: &str) -> bool {
    let s = source.trim();
    let Some(c) = s.chars().next().filter(|c| matches!(c, '-' | '*' | '_')) else { return false };
    !s.contains('\n') && s.chars().all(|x| x == c || x == ' ' || x == '\t') && s.chars().filter(|x| *x == c).count() >= 3
}

/// A GFM table block: its second line is the delimiter row (`| --- | :-: |`).
pub fn is_table(source: &str) -> bool {
    let mut lines = source.lines();
    let (Some(head), Some(delim)) = (lines.next(), lines.next()) else { return false };
    let d = delim.trim();
    head.contains('|') && d.contains('-') && d.chars().all(|c| matches!(c, '|' | '-' | ':' | ' ' | '\t'))
}

/// The items of a flat task list (every line `- [ ] text` or `- [x] text`, same marker, no
/// nesting): (checked, text). `None` for any other block: the kit draws it.
pub fn flat_tasks(source: &str) -> Option<Vec<(bool, &str)>> {
    let mut items = Vec::new();
    for line in source.trim_end().lines() {
        let rest = line.strip_prefix("- ").or_else(|| line.strip_prefix("* ")).or_else(|| line.strip_prefix("+ "))?;
        let (checked, text) = if let Some(t) = rest.strip_prefix("[ ] ") {
            (false, t)
        } else {
            (true, rest.strip_prefix("[x] ").or_else(|| rest.strip_prefix("[X] "))?)
        };
        items.push((checked, text));
    }
    (!items.is_empty()).then_some(items)
}

/// The images in a block's source (`![alt](src)` and `![alt](src "title")`): each `src` as written.
pub fn image_sources(source: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut rest = source;
    while let Some(at) = rest.find("![") {
        rest = &rest[at + 2..];
        let Some(close) = rest.find("](") else { break };
        let after = &rest[close + 2..];
        let Some(end) = after.find(')') else { break };
        let inner = after[..end].trim();
        let src = inner.split_whitespace().next().unwrap_or("").trim_start_matches('<').trim_end_matches('>');
        if !src.is_empty() {
            out.push(src);
        }
        rest = &after[end..];
    }
    out
}

/// Where an image's `src` points for a document at `doc`: a path relative to the document's folder,
/// or an absolute one. URLs and data are not loaded (`None`).
pub fn image_path(doc: &std::path::Path, src: &str) -> Option<std::path::PathBuf> {
    if src.contains("://") || src.starts_with("data:") || src.starts_with('#') {
        return None;
    }
    let p = std::path::Path::new(src);
    Some(if p.is_absolute() { p.to_path_buf() } else { doc.parent()?.join(p) })
}

#[cfg(test)]
mod tests {
    #[test]
    fn heading_chrome_flows_visibly_faster_than_the_edges() {
        for dark in [false, true] {
            let heading = super::heading_metal(dark);
            assert_eq!(heading.speed, super::HEADING_SPEED);
            assert!(heading.speed >= 3.0 * rune_fx::LiquidMetalParams::chrome().speed);
            assert!(heading.flow > 0.0, "a heading with no flow would not move at rest");
        }
    }

    use super::*;

    #[test]
    fn the_charged_edge_multiplies_on_light_only() {
        let light = Look { dark: false, reduced: false };
        let dark = Look { dark: true, reduced: false };
        assert_eq!(blend_of(data(Piece::Electric { red: true }, light)), FxBlend::Multiply);
        assert_eq!(blend_of(data(Piece::Electric { red: false }, dark)), FxBlend::Normal);
        assert_eq!(blend_of(data(Piece::Rod, light)), FxBlend::Normal);
    }

    /// Constraint 1: with no `RUNEMD_FORGE` the six are on and electric follows Settings.
    #[test]
    fn six_are_on_by_default_and_electric_follows_settings() {
        assert_eq!(Directions::parse(None), None);
        assert_eq!(Directions::parse(Some("  ")), None);
        let d = Directions::effective(None, false);
        assert!(d.steel && d.forged && d.engraved && d.ember && d.glass && d.tasks && !d.electric, "{d:?}");
        assert!(Directions::effective(None, true).full(), "the switch adds electric: all seven");
    }

    /// `RUNEMD_FORGE=off` turns all off, the switch included; any lab value wins over Settings.
    #[test]
    fn the_lab_override_turns_all_off() {
        for v in ["off", "OFF", "0", "none"] {
            let d = Directions::effective(Directions::parse(Some(v)), true);
            assert!(!d.any(), "{v}");
        }
        let d = Directions::effective(Directions::parse(Some("nope")), true);
        assert!(!d.any(), "an unknown name alone: nothing on");
        assert_eq!(Directions::effective(Directions::parse(Some("glass")), true), Directions { glass: true, ..Default::default() });
    }

    #[test]
    fn all_and_1_turn_every_direction_on() {
        assert_eq!(Directions::parse(Some("all")), Some(Directions::ALL));
        assert_eq!(Directions::parse(Some("ALL")), Some(Directions::ALL));
        assert_eq!(Directions::parse(Some("1")), Some(Directions::ALL));
        assert!(Directions::ALL.full());
    }

    /// Constraint 3: one block kind at most keeps metal; the rest get the quiet edge.
    #[test]
    fn only_the_metal_block_kind_keeps_metal() {
        let kinds = [BlockKind::Code, BlockKind::Callout, BlockKind::Table, BlockKind::Frontmatter, BlockKind::Image];
        let d = Directions::DEFAULT;
        for k in kinds {
            assert_eq!(d.frame(k, None), Some(Frame::Quiet), "{k:?}");
        }
        for metal in kinds {
            let metals = kinds.iter().filter(|k| matches!(d.frame(**k, Some(metal)), Some(Frame::Metal(_)))).count();
            assert_eq!(metals, 1, "{metal:?}");
        }
        assert_eq!(Directions::default().frame(BlockKind::Code, Some(BlockKind::Code)), None, "off: no frame");
        assert_eq!(Directions { steel: true, ..Default::default() }.frame(BlockKind::Table, None), None, "tables: Forged only");
        assert_eq!(metal_block_from(None), METAL_BLOCK);
        assert_eq!(metal_block_from(Some("code")), Some(BlockKind::Code));
        assert_eq!(metal_block_from(Some("callouts")), Some(BlockKind::Callout));
        assert_eq!(metal_block_from(Some("none")), None);
    }

    /// Review R2 (record 2026-11-23-t3-paths-review): by default no block keeps metal; code blocks
    /// get the table's quiet edge, the same width and colour. `RUNEMD_METAL_BLOCK=code` brings the
    /// metal back in the lab.
    #[test]
    fn code_blocks_get_the_tables_edge_by_default() {
        let d = Directions::DEFAULT;
        assert_eq!(METAL_BLOCK, None);
        assert_eq!(metal_block_from(None), None, "no lab override: no metal");
        assert_eq!(d.frame(BlockKind::Code, METAL_BLOCK), d.frame(BlockKind::Table, METAL_BLOCK));
        for k in [BlockKind::Code, BlockKind::Callout, BlockKind::Table, BlockKind::Frontmatter, BlockKind::Image] {
            assert_eq!(d.frame(k, METAL_BLOCK), Some(Frame::Quiet), "{k:?}");
        }
        assert_eq!(d.frame(BlockKind::Code, metal_block_from(Some("code"))), Some(Frame::Metal(Piece::Edge { width: FORGED_W, chrome: false })));
    }

    #[test]
    fn each_direction_turns_on_alone() {
        for name in Directions::NAMES {
            let d = Directions::parse(Some(name)).unwrap();
            assert!(d.any() && !d.full(), "{name}");
            let on = [d.steel, d.forged, d.engraved, d.ember, d.glass, d.tasks, d.electric];
            assert_eq!(on.iter().filter(|x| **x).count(), 1, "{name}");
        }
        let d = Directions::parse(Some(" Steel , ember,unknown ")).unwrap();
        assert!(d.steel && d.ember && !d.forged && !d.glass);
        // Every name listed is every direction: Full forge.
        assert!(Directions::parse(Some(&Directions::NAMES.join(","))).unwrap().full());
    }

    #[test]
    fn edges_are_gunmetal_until_full_forge() {
        assert_eq!(Directions::default().edge(), None);
        assert_eq!(Directions { steel: true, ..Default::default() }.edge(), Some(Piece::Edge { width: STEEL_W, chrome: false }));
        assert_eq!(Directions { steel: true, forged: true, ..Default::default() }.edge(), Some(Piece::Edge { width: FORGED_W, chrome: false }));
        assert_eq!(Directions { steel: true, ..Default::default() }.wide_edge(), None);
        assert_eq!(Directions::ALL.edge(), Some(Piece::Edge { width: FORGED_W, chrome: true }));
    }

    #[test]
    fn the_rule_picks_one_direction() {
        assert_eq!(Directions::default().rule(), None);
        assert_eq!(Directions { steel: true, ..Default::default() }.rule(), Some(Rule::Engraved));
        assert_eq!(Directions { steel: true, ember: true, ..Default::default() }.rule(), Some(Rule::Fire));
        assert_eq!(Directions::ALL.rule(), Some(Rule::Rod));
    }

    #[test]
    fn slot_data_round_trips() {
        let pieces = [
            Piece::Edge { width: 1.25, chrome: false },
            Piece::Edge { width: 2.0, chrome: true },
            Piece::Rod,
            Piece::Fire,
            Piece::Dither { density: 0.55 },
            Piece::Fluted,
            Piece::Caption,
            Piece::Bead { checked: true },
            Piece::Bead { checked: false },
            Piece::Tube,
            Piece::TubeFill,
            Piece::Electric { red: true },
            Piece::Electric { red: false },
            Piece::Chrome,
        ];
        for p in pieces {
            for look in [Look { dark: true, reduced: false }, Look { dark: false, reduced: true }] {
                assert_eq!(read(data(p, look)), Some((p, look)), "{p:?}");
            }
        }
        assert_eq!(read([0.0; 4]), None, "a slot without a piece");
    }

    #[test]
    fn heading_chrome_is_masked_paced_and_still_when_reduced_or_inactive() {
        let rect = Rect::new(0.0, 40.0, 600.0, 36.0);
        let moving = Look { dark: true, reduced: false };
        let (l, paced) = layer(rect, 0.0, data(Piece::Chrome, moving), 12.5, true).unwrap();
        assert!(l.is_animating() && paced, "it moves on the metal's wake-ups, not the display link");
        assert!(l.fx.cost().source, "clipped to the glyph mask");
        let rune_fx::Fx::Fill(fill) = l.fx else { panic!("a fill") };
        assert!(fill.mask && fill.rim == HEADING_RIM, "only the rim of the letters (M1)");
        assert!(is_chrome(data(Piece::Chrome, moving)) && !is_chrome(data(Piece::Rod, moving)));
        let (l, _) = layer(rect, 0.0, data(Piece::Chrome, Look { dark: false, reduced: true }), 12.5, true).unwrap();
        assert!(!l.is_animating(), "still with reduce motion");
        let (l, _) = layer(rect, 0.0, data(Piece::Chrome, moving), 12.5, false).unwrap();
        assert!(!l.is_animating(), "still in an inactive window");
    }

    #[test]
    fn only_the_charged_edge_moves_and_it_is_paced() {
        let rect = Rect::new(10.0, 10.0, 300.0, 80.0);
        let look = Look { dark: true, reduced: false };
        for p in [Piece::Edge { width: 2.0, chrome: false }, Piece::Rod, Piece::Fire, Piece::Dither { density: 0.4 }, Piece::Fluted, Piece::Caption, Piece::Bead { checked: true }, Piece::Tube, Piece::TubeFill] {
            let (l, paced) = layer(rect, 6.0, data(p, look), 12.5, true).unwrap();
            assert!(!l.is_animating(), "{p:?} is still");
            assert!(!paced, "{p:?}");
        }
        let (l, paced) = layer(rect, 6.0, data(Piece::Electric { red: false }, look), 12.5, true).unwrap();
        assert!(l.is_animating() && paced);
        let (l, _) = layer(rect, 6.0, data(Piece::Electric { red: true }, Look { dark: true, reduced: true }), 12.5, true).unwrap();
        assert!(!l.is_animating(), "reduce motion holds it still");
        let (l, _) = layer(rect, 6.0, data(Piece::Electric { red: true }, look), 12.5, false).unwrap();
        assert!(!l.is_animating(), "an inactive window holds it still");
    }

    #[test]
    fn edges_sit_outside_the_element() {
        let rect = Rect::new(10.0, 10.0, 300.0, 80.0);
        let (l, _) = layer(rect, 6.0, data(Piece::Edge { width: 2.0, chrome: false }, Look::default()), 0.0, true).unwrap();
        assert_eq!(l.shape.rect, rect.inflate(2.0));
        assert_eq!(l.shape.radius, 8.0);
    }

    #[test]
    fn glass_reads_the_backdrop() {
        assert!(Piece::Fluted.backdrop() && Piece::Caption.backdrop());
        let (l, _) = layer(Rect::new(0.0, 0.0, 100.0, 40.0), 4.0, data(Piece::Caption, Look::default()), 0.0, true).unwrap();
        assert!(l.fx.needs_backdrop());
        assert!(!Piece::Rod.backdrop());
    }

    #[test]
    fn rules_and_tables_are_recognised() {
        for r in ["---", "***", "___", "- - -", "-----"] {
            assert!(is_rule(r), "{r}");
        }
        for r in ["--", "-*-", "text", "---\nmore", "# h"] {
            assert!(!is_rule(r), "{r}");
        }
        assert!(is_table("| a | b |\n| --- | :-: |\n| 1 | 2 |"));
        assert!(is_table("a | b\n---|---"));
        assert!(!is_table("a paragraph\nwith two lines"));
        assert!(!is_table("| only a head |"));
    }

    #[test]
    fn flat_task_lists_only() {
        assert_eq!(flat_tasks("- [ ] one\n- [x] two\n"), Some(vec![(false, "one"), (true, "two")]));
        assert_eq!(flat_tasks("- [ ] one\n  - [ ] nested"), None);
        assert_eq!(flat_tasks("- [ ] one\n- plain"), None);
        assert_eq!(flat_tasks("1. [ ] numbered"), None);
        assert_eq!(flat_tasks(""), None);
    }

    #[test]
    fn image_sources_are_found_and_resolved() {
        assert_eq!(image_sources("![Forge](images/a.png) and ![](b.png \"t\")"), vec!["images/a.png", "b.png"]);
        assert_eq!(image_sources("no images [link](x)"), Vec::<&str>::new());
        let doc = std::path::Path::new("/docs/note.md");
        assert_eq!(image_path(doc, "images/a.png"), Some("/docs/images/a.png".into()));
        assert_eq!(image_path(doc, "/abs/b.png"), Some("/abs/b.png".into()));
        assert_eq!(image_path(doc, "https://x/y.png"), None);
    }
}
