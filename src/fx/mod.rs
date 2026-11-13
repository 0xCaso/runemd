//! Effetti GPU di RuneMD: un solo compositor rune-fx per tutta la finestra.

mod compositor;
mod marks;

pub use compositor::FxCompositor;
// Usato dai task che accendono i layer (anelli, fuoco, dither).
#[allow(unused_imports)]
pub use marks::Mark;
// `FxLayer` è il layer disegnabile di rune-fx, usato così com'è.
#[allow(unused_imports)]
pub use rune_fx::FxLayer;
