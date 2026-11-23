use std::collections::HashMap;

use rune::compose::{Compositor, Frame};
use rune::render::{EffectPhase, EffectSite, Instance, KIND_EFFECT, KIND_GLYPH};
use rune_fx::{FxBlend, FxFrame, FxLayer, FxOptions, FxRenderer, FxSlot, Rect};

use super::marks::Mark;
use super::{caret, doc_fx, fire, forge, ring};
use crate::App;

/// A single compositor for the window. Without effects (no layer, no slot) the frame is
/// Rune's default one and `animating()` is false: no frames, no cost. The fire at
/// rest, the calm wordmark fire and the moving metal edges (past a ring's entrance) do not hold the
/// display link: the `fire::REST_FRAME` and `ring::METAL_FRAME` wake-ups redraw them.
///
/// Rings, fire and caret do not go through `set`: the view puts a `ring::slot` slot only while
/// the file "is writing" or on the calm edge (active tab, ⌘P panel), a `fire::slot` always in reading mode (at rest or lit) and a
/// `caret::slot` only while the file is being written and the point of the last edit is known; here every slot becomes the layer of
/// its rect.
#[derive(Default)]
pub struct FxCompositor {
    layers: HashMap<u32, FxLayer>,
    fx: Option<FxRenderer>,
    /// The last frame had a layer that moves on its own: the window holds the display link.
    moving: bool,
    /// The last frame drew at least one layer.
    drew: bool,
    /// The fire's fall state: whether the last frame's fire was lit (new text below the view), and the
    /// clock at which it went out, from where it goes back to rest (`fire::Phase::Falling`).
    fire: FireFall,
    /// The headings' glyph mask (`doc_fx::Piece::Chrome`): a viewport-sized texture, its view
    /// and size, made on the first chrome heading and again when the viewport changes.
    mask: Option<(wgpu::Texture, wgpu::TextureView, (u32, u32))>,
}

/// How far back from a chrome slot the glyphs of its heading are looked for: the heading's own
/// instances come right before the slot, and a heading is a few lines of text.
const MASK_SCAN: usize = 1024;

/// The glyphs a chrome slot at `at` covers, white: the heading's text drawn right before it
/// (selection and Find marks between them are skipped). The walk ends at the first glyph outside
/// the slot, which belongs to an earlier block.
pub fn heading_glyphs(instances: &[Instance], at: usize, out: &mut Vec<Instance>) {
    let [x, y, w, h] = instances[at].rect;
    let inside = |i: &Instance| {
        let (cx, cy) = (i.rect[0] + i.rect[2] / 2.0, i.rect[1] + i.rect[3] / 2.0);
        cx >= x && cx <= x + w && cy >= y && cy <= y + h
    };
    for i in instances[..at].iter().rev().take(MASK_SCAN) {
        if i.params[1] != KIND_GLYPH {
            continue;
        }
        if !inside(i) {
            break;
        }
        out.push(Instance { fill: [1.0; 4], ..*i });
    }
}

/// A slot's rect in pt.
fn rect_pt(i: &Instance) -> Rect {
    let k = i.params[3].max(f32::EPSILON);
    let [x, y, w, h] = i.rect.map(|v| v / k);
    Rect::new(x, y, w, h)
}

/// The lab's clock (s) for the slot `mark`, in the lab's test state (`--lab` with a time): the rings
/// and the fire are drawn at the lab's time, lit at `lab::REST_MS` (`App::ring_of`, `App::fire_of`).
/// The empty screen's agent line is not part of the test state: it stays on Rune's clock.
fn lab_clock(mark: u32) -> Option<f64> {
    let on_lab = mark == Mark::Fire.id() || (Mark::is_ring(mark) && mark != Mark::ForgeLine.id());
    crate::lab::lab_time().filter(|_| on_lab).map(|t| t / 1000.0)
}

/// The ring layer (calm edge or writing ring) for a `ring::slot` slot with id `mark`, at Rune's time
/// `clock`. In the lab's test state it is the frame at the lab's time, "reduce motion" or not: the
/// scrub always shows it.
/// `active`: the window is active; inactive, every edge is still (rune-fx, constraint 2).
#[cfg(test)]
fn ring_layer(i: &Instance, mark: u32, clock: f64, active: bool) -> FxLayer {
    ring_layer_paced(i, mark, clock, active).0
}

/// [`ring_layer`], and whether it is paced: a calm edge, or a writing ring past its entrance, moves
/// with the `ring::METAL_FRAME` wake-ups (`App::metal_moves`), not the display link. The entrance
/// holds the display link, so the fade is smooth.
fn ring_layer_paced(i: &Instance, mark: u32, clock: f64, active: bool) -> (FxLayer, bool) {
    let radius = i.radii.iter().fold(0.0f32, |a, v| a.max(v.abs())) / i.params[3].max(f32::EPSILON);
    let lab = lab_clock(mark);
    let dark = ring::slot_dark(i.uv);
    if ring::slot_calm(i.uv) {
        let still = (ring::slot_calm_reduced(i.uv) || !active) && lab.is_none();
        return (ring::calm_layer(rect_pt(i), radius, lab.unwrap_or(clock), still, dark), true);
    }
    let (t, reduced) = ring::slot_time(i.uv, lab.unwrap_or(clock));
    (ring::layer_rounded(rect_pt(i), radius, t, (reduced || !active) && lab.is_none(), dark), ring::entered(t))
}

/// Each tab ring in the last frame of `rt` as the compositor paints it: (slot id, device-pixel clip,
/// layer). Only for tests of the front tab's closed ring.
#[cfg(test)]
pub fn tab_ring_slots(rt: &rune::Runtime<App>) -> Vec<(u32, [f32; 4], FxLayer)> {
    rt.instances
        .iter()
        .enumerate()
        .filter(|(_, i)| i.params[1] == KIND_EFFECT)
        .filter_map(|(n, i)| rt.marks.get(n).copied().filter(|m| Mark::is_tab(*m)).map(|m| (m, i)))
        .map(|(m, i)| (m, i.clip, ring_layer(i, m, rt.clock, rt.state.window_active)))
        .collect()
}

/// Each ring layer in the last frame of `rt`, as the compositor paints it: (slot id, layer). Only for
/// tests.
#[cfg(test)]
pub fn ring_layers(rt: &rune::Runtime<App>) -> Vec<(u32, FxLayer)> {
    rt.instances
        .iter()
        .enumerate()
        .filter(|(_, i)| i.params[1] == KIND_EFFECT)
        .filter_map(|(n, i)| rt.marks.get(n).copied().filter(|m| Mark::is_ring(*m)).map(|m| (m, ring_layer(i, m, rt.clock, rt.state.window_active))))
        .collect()
}

/// The fire layer in the last frame of `rt`, as `compositor` paints it (its fall state carries over
/// between calls). Only for tests.
#[cfg(test)]
pub fn fire_layer(rt: &rune::Runtime<App>, compositor: &mut FxCompositor) -> Option<FxLayer> {
    let n = (0..rt.instances.len()).find(|n| rt.instances[*n].params[1] == KIND_EFFECT && rt.marks.get(*n).copied() == Some(Mark::Fire.id()))?;
    Some(compositor.fire.fire_layer(&rt.instances[n], rt.clock).0)
}

/// The metal edges in the last frame of `rt`, in drawing order: (slot id, calm). Only for tests
/// of which places get the calm or the moving edge (metal-tab).
#[cfg(test)]
pub fn edges(rt: &rune::Runtime<App>) -> Vec<(u32, bool)> {
    rt.instances
        .iter()
        .enumerate()
        .filter(|(_, i)| i.params[1] == KIND_EFFECT)
        .filter_map(|(n, i)| rt.marks.get(n).copied().filter(|m| Mark::is_ring(*m)).map(|m| (m, ring::slot_calm(i.uv))))
        .collect()
}

/// Each metal edge in the last frame of `rt`, as the compositor paints it: (slot id, fill). Only
/// for tests of the calm and writing widths and metals (metal-tab).
#[cfg(test)]
pub fn edge_fills(rt: &rune::Runtime<App>) -> Vec<(u32, rune_fx::FillParams)> {
    rt.instances
        .iter()
        .enumerate()
        .filter(|(_, i)| i.params[1] == KIND_EFFECT)
        .filter_map(|(n, i)| rt.marks.get(n).copied().filter(|m| Mark::is_ring(*m)).map(|m| (m, i)))
        .map(|(m, i)| match ring_layer(i, m, rt.clock, rt.state.window_active).fx {
            rune_fx::Fx::Fill(p) => (m, p),
            other => panic!("not a fill: {other:?}"),
        })
        .collect()
}

/// The fire phase of a `fire::slot` slot, given when it last went out (`out`).
fn fire_phase(s: fire::SlotState, out: Option<f64>, clock: f64) -> fire::Phase {
    match (s.lit, out) {
        (true, _) => fire::Phase::Rising(s.t),
        (false, Some(at)) => fire::Phase::Falling((clock - at).max(0.0) as f32),
        (false, None) => fire::Phase::Rest,
    }
}

/// The caret layer for a `caret::slot` slot and whether it pulses (rune-fx does not count it as
/// animated: the opacity is still within a step).
fn caret_layer(i: &Instance, clock: f64) -> (FxLayer, bool) {
    let s = caret::slot_state(i.uv, clock);
    (caret::layer(rect_pt(i), s.t, s.dark, s.reduced), s.moving())
}

/// The fire's fall state between frames: lit in the last frame, and when it went out.
#[derive(Clone, Copy, Debug, Default)]
struct FireFall {
    lit: bool,
    out: Option<f64>,
}

impl FireFall {
    /// The fire layer for a `fire::slot` slot at Rune's time `clock`, and whether it is paced (redrawn
    /// by the `fire::REST_FRAME` wake-ups, not the display link). In the lab's test state it is the
    /// frame at the lab's time: lit from `lab::REST_MS`, at rest before, with no fall in between (the
    /// scrub may go back), still, and drawn as if the window were active and motion not reduced.
    fn fire_layer(&mut self, i: &Instance, clock: f64) -> (FxLayer, Option<FxLayer>, bool) {
        if let Some(clock) = lab_clock(Mark::Fire.id()) {
            let s = fire::slot_state(i.uv, clock);
            *self = FireFall::default();
            let phase = if s.lit { fire::Phase::Rising(s.t) } else { fire::Phase::Rest };
            let layer = fire::layer(rect_pt(i), phase, clock as f32, s.dark, false, true, s.look);
            return (layer, fire::tips(layer, s.dark), false);
        }
        // The slot rect touches the bottom of the panel and has its width.
        let s = fire::slot_state(i.uv, clock);
        if self.lit && !s.lit {
            self.out = Some(clock);
        } else if s.lit {
            self.out = None;
        }
        self.lit = s.lit;
        let phase = fire_phase(s, self.out, clock);
        let layer = fire::faded(fire::layer(rect_pt(i), phase, clock as f32, s.dark, s.reduced, s.active_window, s.look), s.fade);
        // The Brief fire (it fades) holds the display link for its whole show, not only its fade:
        // paced at 12 fps its first seconds stuttered, unevenly when other wake-ups crossed the
        // `REST_FRAME` ones (ledger S1). Only the Always fire rests on them.
        let paced = fire::paced(phase) && s.fade.is_none();
        if paced {
            self.out = None;
        }
        (layer, fire::tips(layer, s.dark), paced)
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
        // The renderer is born at the first frame; the pipelines compile only when a layer uses them.
        let fx = self.fx.get_or_insert_with(|| FxRenderer::new(device, f.format, FxOptions { dither: true }));
        // At rest the frame stays Rune's, without even walking the instances.
        if self.layers.is_empty() && !f.rt.state.effects_lit() {
            (self.moving, self.drew) = (false, false);
            f.draw_ui();
            return;
        }
        let clock = f.rt.clock;
        let mut on_lab: Vec<usize> = Vec::new();
        let mut at: Vec<u32> = Vec::new();
        let mut layers: Vec<FxLayer> = Vec::new();
        let mut slots: Vec<FxSlot> = Vec::new();
        let mut pulsing = false;
        let mut paced: Vec<usize> = Vec::new();
        let mut saw_fire = false;
        let mut tip_layer: Option<FxLayer> = None;
        let mut metal = false;
        let mut mask: Vec<Instance> = Vec::new();
        for (n, i) in f.rt.instances.iter().enumerate() {
            if i.params[1] != KIND_EFFECT {
                continue;
            }
            let mark = f.rt.marks.get(n).copied().unwrap_or(0);
            // The layers on the lab's clock stand still: each new `lab.json` wakes the app instead.
            if lab_clock(mark).is_some() {
                on_lab.push(layers.len());
            }
            let layer = if Mark::is_ring(mark) {
                let (layer, rests) = ring_layer_paced(i, mark, clock, f.rt.state.window_active);
                if rests {
                    paced.push(layers.len());
                }
                metal |= layer.is_animating();
                layer
            } else if mark == Mark::Fire.id() {
                let (layer, tips, rests) = self.fire.fire_layer(i, clock);
                // At rest the flames move with the `fire::REST_FRAME` wake-ups (`App::fire_rests`),
                // not with the display link; the Brief fade-out does hold it, so it is smooth.
                if rests {
                    paced.push(layers.len());
                }
                saw_fire = true;
                // The light theme's warm tips go on top, in the same slot, with Screen.
                if let Some(tips) = tips {
                    let k = layers.len() + 1;
                    if rests {
                        paced.push(k);
                    }
                    if lab_clock(mark).is_some() {
                        on_lab.push(k);
                    }
                    tip_layer = Some(tips);
                }
                layer
            } else if mark == Mark::Forge.id() {
                // The empty screen's wordmark: the slot is the letters' box (`fx::forge`).
                // Calm, it moves with the same `fire::REST_FRAME` wake-ups (`App::fire_rests`).
                let s = forge::slot_state(i.uv, clock);
                if forge::paced(s.motion, s.t) {
                    paced.push(layers.len());
                }
                forge::layer(rect_pt(i), s.motion, s.t, clock as f32)
            } else if mark == Mark::DocFx.id() {
                // Full forge (`fx::doc_fx`): still pieces, and the charged edge, which moves with the
                // metal's `ring::METAL_FRAME` wake-ups like a calm edge (no display link, no view run).
                let radius = i.radii.iter().fold(0.0f32, |a, v| a.max(v.abs())) / i.params[3].max(f32::EPSILON);
                let Some((layer, rests)) = doc_fx::layer(rect_pt(i), radius, i.uv, clock, f.rt.state.window_active) else { continue };
                // A heading's chrome samples the glyph mask (source 0), which covers the viewport.
                let layer = if doc_fx::is_chrome(i.uv) {
                    heading_glyphs(&f.rt.instances, n, &mut mask);
                    let k = i.params[3].max(f32::EPSILON);
                    layer.source(0, Rect::new(0.0, 0.0, size.0 as f32 / k, size.1 as f32 / k))
                } else {
                    layer
                };
                if rests {
                    paced.push(layers.len());
                }
                metal |= layer.is_animating();
                layer
            } else if mark == Mark::Caret.id() {
                let (layer, moving) = caret_layer(i, clock);
                pulsing |= moving;
                layer
            } else if let Some(l) = self.layers.get(&mark) {
                *l
            } else {
                continue;
            };
            let blend = if mark == Mark::DocFx.id() { doc_fx::blend_of(i.uv) } else { FxBlend::Normal };
            at.push(n as u32);
            layers.push(layer);
            let slot = FxSlot::default().scale(i.params[3]).clip(i.clip, i.fade).blend(blend);
            slots.push(slot);
            if let Some(tips) = tip_layer.take() {
                at.push(n as u32);
                layers.push(tips);
                slots.push(slot.blend(FxBlend::Screen));
            }
        }
        // The frame of the moving metal: `poll` paces the next one from here (`App::metal_due`).
        if metal {
            f.rt.state.metal_drawn.set(Some(clock));
        }
        // Without fire (source, fire off) the next one starts again from rest, without descent.
        if !saw_fire {
            self.fire = FireFall::default();
        }
        // Still as soon as no layer moves: a ring with "reduce motion" does not hold the
        // display link, and one that went out is no longer among the instances. The pulsing caret does.
        self.moving = pulsing || layers.iter().enumerate().any(|(k, l)| !paced.contains(&k) && !on_lab.contains(&k) && l.is_animating());
        self.drew = !layers.is_empty();
        fx.set_srgb_blend(f.srgb_blend());
        fx.add_format(device, wgpu::TextureFormat::Rgba16Float);
        if mask.is_empty() {
            fx.prepare_with_slots(device, queue, &FxFrame { viewport: [size.0, size.1], scale: 1.0, layers: &layers, backdrop: None, sources: &[] }, &slots);
        } else {
            if self.mask.as_ref().is_none_or(|m| m.2 != size) {
                let texture = device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("runemd heading mask"),
                    size: wgpu::Extent3d { width: size.0.max(1), height: size.1.max(1), depth_or_array_layers: 1 },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: f.format,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                });
                let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
                self.mask = Some((texture, view, size));
            }
            let view = self.mask.as_ref().map(|m| &m.1).expect("made above");
            fx.prepare_with_slots(device, queue, &FxFrame { viewport: [size.0, size.1], scale: 1.0, layers: &layers, backdrop: None, sources: &[view] }, &slots);
        }
        let clear = f.rt.clear_color();
        let mut hook = |phase: EffectPhase<'_, '_>, s: &EffectSite<'_>| {
            // One slot may carry more than one layer (the fire and its light-theme tips), in order.
            let Some(first) = at.iter().position(|n| *n == s.index) else { return };
            match phase {
                EffectPhase::Backdrop(encoder) => {
                    if let Some(b) = s.backdrop {
                        fx.blur_backdrop(device, queue, encoder, b, first);
                    }
                }
                EffectPhase::Draw(pass) => {
                    for (k, _) in at.iter().enumerate().filter(|(_, n)| **n == s.index) {
                        fx.draw_layer_in(pass, s.format, s.origin, k);
                    }
                }
            }
        };
        // Chrome headings: the frame's instances, then their glyphs in white, drawn into the mask
        // before the frame (the instance upload is flushed by `encode_layers_with_backdrop`).
        if let Some((_, view, _)) = self.mask.as_ref().filter(|_| !mask.is_empty()) {
            let ranges = f.ui.prepare_lists(device, queue, &mut f.rt.text.atlas, &[&f.rt.instances, &mask], size);
            f.ui.encode_layers_with_backdrop(f.encoder, &mut hook);
            {
                let mut pass = f.encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("runemd heading mask"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment { view, depth_slice: None, resolve_target: None, ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Store } })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                f.ui.draw_range(&mut pass, f.format, ranges[1].clone());
            }
            let c = wgpu::Color { r: clear[0] as f64, g: clear[1] as f64, b: clear[2] as f64, a: clear[3] as f64 };
            f.ui.draw_target_range_with(f.encoder, f.target, f.format, wgpu::LoadOp::Clear(c), ranges[0].clone(), &mut hook);
            return;
        }
        f.ui.prepare(device, queue, &mut f.rt.text.atlas, &f.rt.instances, size);
        // `draw_with` is this with no backdrop phase: glass and the light charged edge (Full forge)
        // read what is painted before them, and the pass splits only at their slots.
        f.ui.draw_with_backdrop(f.encoder, f.target, clear, &mut hook);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inst(kind: f32, rect: [f32; 4]) -> Instance {
        Instance { rect, radii: [0.0; 4], fill: [0.2, 0.3, 0.4, 1.0], border: [0.0; 4], params: [0.0, kind, 0.0, 2.0], uv: [0.0; 4], clip: [0.0; 4], fade: [0; 4] }
    }

    #[test]
    fn a_chrome_slot_masks_only_its_own_headings_glyphs() {
        let list = [
            inst(KIND_GLYPH, [10.0, 0.0, 8.0, 10.0]),   // the paragraph above
            inst(0.0, [0.0, 40.0, 400.0, 60.0]),        // a selection mark on the heading
            inst(KIND_GLYPH, [10.0, 50.0, 12.0, 30.0]), // the heading's letters
            inst(KIND_GLYPH, [30.0, 50.0, 12.0, 30.0]),
            inst(KIND_EFFECT, [0.0, 40.0, 400.0, 60.0]), // the chrome slot
        ];
        let mut out = Vec::new();
        heading_glyphs(&list, 4, &mut out);
        assert_eq!(out.len(), 2, "the two letters, not the paragraph or the mark");
        assert!(out.iter().all(|i| i.fill == [1.0; 4] && i.params[1] == KIND_GLYPH), "white glyphs");
    }

    /// The Brief fire holds the display link for its whole show, as for its fade: paced by the
    /// 12 fps `REST_FRAME` wake-ups its first seconds stuttered (ledger S1). The Always fire, which
    /// never fades, stays paced at rest.
    #[test]
    fn only_the_always_fire_is_paced_at_rest() {
        let look = fire::Look::default();
        let paced = |fade_at: Option<f64>, clock: f64| {
            let mut i = inst(KIND_EFFECT, [0.0, 600.0, 1200.0, 200.0]);
            i.uv = fire::slot_data(None, fade_at, false, false, true, look);
            FireFall::default().fire_layer(&i, clock).2
        };
        assert!(!paced(Some(15.0), 10.0), "Brief, in full");
        assert!(!paced(Some(15.0), 15.5), "Brief, fading");
        assert!(paced(None, 10.0), "Always");
    }
}
