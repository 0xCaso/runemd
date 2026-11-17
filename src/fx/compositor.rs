use std::collections::HashMap;

use rune::compose::{Compositor, Frame};
use rune::render::{EffectSite, Instance, KIND_EFFECT};
use rune_fx::{FxFrame, FxLayer, FxOptions, FxRenderer, FxSlot, Rect};

use super::marks::Mark;
use super::{caret, fire, forge, ring};
use crate::App;

/// A single compositor for the window. Without effects (no layer, no slot) the frame is
/// Rune's default one and `animating()` is false: no frames, no cost. The fire at
/// rest and the calm wordmark fire do not hold the display link: the `fire::REST_FRAME` wake-ups redraw them.
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
fn ring_layer(i: &Instance, mark: u32, clock: f64) -> FxLayer {
    let radius = i.radii.iter().fold(0.0f32, |a, v| a.max(v.abs())) / i.params[3].max(f32::EPSILON);
    if ring::slot_calm(i.uv) {
        return ring::calm_layer(rect_pt(i), radius, ring::slot_dark(i.uv), ring::slot_calm_look(i.uv));
    }
    let lab = lab_clock(mark);
    let (t, reduced) = ring::slot_time(i.uv, lab.unwrap_or(clock));
    ring::layer_rounded(rect_pt(i), radius, t, reduced && lab.is_none(), ring::slot_dark(i.uv))
}

/// Each ring layer in the last frame of `rt`, as the compositor paints it: (slot id, layer). Only for
/// tests.
#[cfg(test)]
pub fn ring_layers(rt: &rune::Runtime<App>) -> Vec<(u32, FxLayer)> {
    rt.instances
        .iter()
        .enumerate()
        .filter(|(_, i)| i.params[1] == KIND_EFFECT)
        .filter_map(|(n, i)| rt.marks.get(n).copied().filter(|m| Mark::is_ring(*m)).map(|m| (m, ring_layer(i, m, rt.clock))))
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
        .map(|(m, i)| match ring_layer(i, m, rt.clock).fx {
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
    fn fire_layer(&mut self, i: &Instance, clock: f64) -> (FxLayer, bool) {
        if let Some(clock) = lab_clock(Mark::Fire.id()) {
            let s = fire::slot_state(i.uv, clock);
            *self = FireFall::default();
            let phase = if s.lit { fire::Phase::Rising(s.t) } else { fire::Phase::Rest };
            return (fire::layer(rect_pt(i), phase, clock as f32, s.dark, false, true, s.look), false);
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
        let layer = fire::layer(rect_pt(i), phase, clock as f32, s.dark, s.reduced, s.active_window, s.look);
        let paced = fire::paced(phase);
        if paced {
            self.out = None;
        }
        (layer, paced)
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
                ring_layer(i, mark, clock)
            } else if mark == Mark::Fire.id() {
                let (layer, rests) = self.fire.fire_layer(i, clock);
                // At rest the flames move with the `fire::REST_FRAME` wake-ups (`App::fire_rests`),
                // not with the display link.
                if rests {
                    paced.push(layers.len());
                }
                saw_fire = true;
                layer
            } else if mark == Mark::Forge.id() {
                // The empty screen's wordmark: the slot is the letters' box (`fx::forge`).
                // Calm, it moves with the same `fire::REST_FRAME` wake-ups (`App::fire_rests`).
                let s = forge::slot_state(i.uv, clock);
                if forge::paced(s.motion, s.t) {
                    paced.push(layers.len());
                }
                forge::layer(rect_pt(i), s.motion, s.t, clock as f32)
            } else if mark == Mark::Caret.id() {
                let (layer, moving) = caret_layer(i, clock);
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
