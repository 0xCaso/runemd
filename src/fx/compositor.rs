use std::collections::HashMap;

use rune::compose::{Compositor, Frame};
use rune::render::{EffectSite, Instance, KIND_EFFECT};
use rune_fx::{FxFrame, FxLayer, FxOptions, FxRenderer, FxSlot, Rect};

use super::marks::Mark;
use super::{dither, fire, ring};
use crate::App;

/// Un solo compositor per la finestra. A riposo (nessun layer, nessun anello acceso) il frame è
/// quello di default di Rune e `animating()` è falso: niente frame, niente costo.
///
/// Anelli, fuoco e dither non passano da `set`: la vista mette uno slot `ring::slot` solo mentre
/// il file è "sta scrivendo", uno `fire::slot` solo mentre c'è testo nuovo sotto la vista e uno
/// `dither::slot` solo mentre l'ultimo blocco sta arrivando; qui ogni slot diventa il layer del
/// suo rect.
#[derive(Default)]
pub struct FxCompositor {
    layers: HashMap<u32, FxLayer>,
    fx: Option<FxRenderer>,
    /// L'ultimo frame aveva un layer che si muove da solo: la finestra tiene il display link.
    moving: bool,
    /// L'ultimo frame ha disegnato almeno un layer.
    drew: bool,
}

/// Il rect di uno slot in pt.
fn rect_pt(i: &Instance) -> Rect {
    let k = i.params[3].max(f32::EPSILON);
    let [x, y, w, h] = i.rect.map(|v| v / k);
    Rect::new(x, y, w, h)
}

/// Il layer dell'anello per uno slot `ring::slot`, al tempo `clock` di Rune.
fn ring_layer(i: &Instance, clock: f64) -> FxLayer {
    let radius = i.radii.iter().fold(0.0f32, |a, v| a.max(v.abs())) / i.params[3].max(f32::EPSILON);
    let (t, reduced) = ring::slot_time(i.uv, clock);
    ring::layer_rounded(rect_pt(i), radius, t, reduced, ring::slot_dark(i.uv))
}

/// Il layer del fuoco per uno slot `fire::slot`: il rect dello slot tocca il fondo del pannello
/// e ne ha la larghezza.
fn fire_layer(i: &Instance, clock: f64) -> FxLayer {
    let s = fire::slot_state(i.uv, clock);
    fire::layer(rect_pt(i), s.t, s.dark, s.reduced, s.active_window)
}

/// Il layer del dither per uno slot `dither::slot` e se pulsa (rune-fx non lo conta come
/// animato: il tono è fermo dentro un passo).
fn dither_layer(i: &Instance, clock: f64) -> (FxLayer, bool) {
    let s = dither::slot_state(i.uv, clock);
    (dither::layer(rect_pt(i), s.t, s.dark, s.reduced), s.moving())
}

impl FxCompositor {
    /// Accende (`Some`) o spegne (`None`) il layer di uno slot.
    #[allow(dead_code)]
    pub fn set(&mut self, mark: Mark, layer: Option<FxLayer>) {
        match layer {
            Some(l) => self.layers.insert(mark.id(), l),
            None => self.layers.remove(&mark.id()),
        };
    }
}

impl Compositor<App> for FxCompositor {
    fn animating(&self) -> bool {
        self.moving
    }

    fn draws_own(&self) -> bool {
        self.drew
    }

    fn render(&mut self, f: &mut Frame<'_, App>) {
        let (device, queue, size) = (f.device, f.queue, f.size);
        // Il renderer nasce al primo frame; le pipeline si compilano solo quando un layer le usa.
        let fx = self.fx.get_or_insert_with(|| FxRenderer::new(device, f.format, FxOptions { dither: true }));
        // A riposo il frame resta quello di Rune, senza nemmeno scorrere le istanze.
        if self.layers.is_empty() && !f.rt.state.effects_lit() {
            (self.moving, self.drew) = (false, false);
            f.draw_ui();
            return;
        }
        let clock = f.rt.clock;
        let mut at: Vec<u32> = Vec::new();
        let mut layers: Vec<FxLayer> = Vec::new();
        let mut slots: Vec<FxSlot> = Vec::new();
        let mut pulsing = false;
        for (n, i) in f.rt.instances.iter().enumerate() {
            if i.params[1] != KIND_EFFECT {
                continue;
            }
            let mark = f.rt.marks.get(n).copied().unwrap_or(0);
            let layer = if Mark::is_ring(mark) {
                ring_layer(i, clock)
            } else if mark == Mark::Fire.id() {
                fire_layer(i, clock)
            } else if mark == Mark::Dither.id() {
                let (layer, moving) = dither_layer(i, clock);
                pulsing |= moving;
                layer
            } else if let Some(l) = self.layers.get(&mark) {
                *l
            } else {
                continue;
            };
            at.push(n as u32);
            layers.push(layer);
            slots.push(FxSlot::default().scale(i.params[3]).clip(i.clip, i.fade));
        }
        // Fermo appena nessun layer si muove: un anello con "riduci movimento" non tiene il
        // display link, e uno spento non c'è più fra le istanze. Il dither che pulsa sì.
        self.moving = pulsing || layers.iter().any(FxLayer::is_animating);
        self.drew = !layers.is_empty();
        fx.set_srgb_blend(f.srgb_blend());
        fx.add_format(device, wgpu::TextureFormat::Rgba16Float);
        fx.prepare_with_slots(device, queue, &FxFrame { viewport: [size.0, size.1], scale: 1.0, layers: &layers, backdrop: None, sources: &[] }, &slots);
        let clear = f.rt.clear_color();
        f.ui.prepare(device, queue, &mut f.rt.text.atlas, &f.rt.instances, size);
        f.ui.draw_with(f.encoder, f.target, clear, &mut |pass, s: &EffectSite<'_>| {
            if let Some(k) = at.iter().position(|n| *n == s.index) {
                fx.draw_layer_in(pass, s.format, s.origin, k);
            }
        });
    }
}
