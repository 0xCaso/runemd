use std::collections::HashMap;

use rune::compose::{Compositor, Frame};
use rune::render::{EffectSite, KIND_EFFECT};
use rune_fx::{FxFrame, FxLayer, FxOptions, FxRenderer, FxSlot};

use super::marks::Mark;
use crate::Doc;

/// Un solo compositor per la finestra. A riposo (nessun layer) il frame è quello di default di
/// Rune e `animating()` è falso: niente frame, niente costo.
#[derive(Default)]
pub struct FxCompositor {
    layers: HashMap<u32, FxLayer>,
    fx: Option<FxRenderer>,
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

impl Compositor<Doc> for FxCompositor {
    fn animating(&self) -> bool {
        !self.layers.is_empty()
    }

    fn draws_own(&self) -> bool {
        !self.layers.is_empty()
    }

    fn render(&mut self, f: &mut Frame<'_, Doc>) {
        let (device, queue, size) = (f.device, f.queue, f.size);
        // Il renderer nasce al primo frame; le pipeline si compilano solo quando un layer le usa.
        let fx = self.fx.get_or_insert_with(|| FxRenderer::new(device, f.format, FxOptions { dither: true }));
        if self.layers.is_empty() {
            f.draw_ui();
            return;
        }
        let mut at: Vec<u32> = Vec::new();
        let mut layers: Vec<FxLayer> = Vec::new();
        let mut slots: Vec<FxSlot> = Vec::new();
        for (n, i) in f.rt.instances.iter().enumerate() {
            if i.params[1] != KIND_EFFECT {
                continue;
            }
            let mark = f.rt.marks.get(n).copied().unwrap_or(0);
            let Some(layer) = self.layers.get(&mark) else { continue };
            at.push(n as u32);
            layers.push(*layer);
            slots.push(FxSlot::default().scale(i.params[3]).clip(i.clip, i.fade));
        }
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
