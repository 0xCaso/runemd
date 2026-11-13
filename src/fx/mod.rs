//! Effetti GPU di RuneMD: un solo compositor rune-fx per tutta la finestra.

mod compositor;
mod marks;
pub mod ring;

pub use compositor::FxCompositor;
pub use marks::Mark;
// `FxLayer` è il layer disegnabile di rune-fx, usato così com'è.
#[allow(unused_imports)]
pub use rune_fx::FxLayer;
