//! New file (Cmd+N): in the sidebar the "New file" field takes the place of "Open file" and asks
//! for the name. Return creates (`App::create_from_sidebar`); Esc or a click outside close without creating.

use rune::{El, Key, KeyEvent, TextEdit, Tx, icon, text_input};
use rune_kit::glyphs;

use super::quick_open::{field_row, kbd};
use super::sidebar::{Hooks, Sidebar, ui_font};
use super::theme::{ICON, Theme};
use crate::edit::new_file::normalize;

/// The state of the Cmd+N field.
#[derive(Default)]
pub struct NewFile {
    pub edit: TextEdit,
    pub open: bool,
}

impl NewFile {
    /// Cmd+N: the field opens, empty, with focus.
    pub fn show(&mut self) {
        self.edit = TextEdit::default();
        self.edit.focused = true;
        self.open = true;
    }

    /// Esc, a click outside or a created file: the field closes and empties.
    pub fn close(&mut self) {
        self.edit = TextEdit::default();
        self.open = false;
    }

    /// The field lost focus: it closes. True if it was open.
    pub fn blur_close(&mut self) -> bool {
        if self.open && !self.edit.focused {
            self.close();
            return true;
        }
        false
    }

    /// What Return asks to create: the typed name, if it is valid.
    pub fn name(&self) -> Option<String> {
        normalize(&self.edit.text).map(|_| self.edit.text.trim().to_owned())
    }
}

/// The "New file ⌘N" field, built like the Cmd+P one.
pub(super) fn field<S: 'static>(sb: &Sidebar, hooks: Hooks<S>, theme: &Theme) -> El {
    let input = text_input(&sb.new_file.edit, ui_font())
        .single_line()
        .key("new-file-input")
        .grow(1.0)
        .shrink()
        .min_w(0.0)
        .color(theme.fg)
        .placeholder("New file", theme.fg2)
        .caret(theme.fg, theme.sel)
        .label("New file")
        .on_edit(move |s: &mut S| &mut (hooks.get_mut)(s).new_file.edit)
        .on_input_key(move |s: &mut S, ev: &KeyEvent, tx: &mut Tx| on_key(s, hooks, ev, tx));
    field_row(
        icon(&glyphs::FILE, ICON).color(theme.fg2),
        input,
        kbd("⌘N", theme),
        theme,
    )
}

/// The field's keys, before the editor. True if it takes them.
fn on_key<S: 'static>(s: &mut S, hooks: Hooks<S>, ev: &KeyEvent, tx: &mut Tx) -> bool {
    if (hooks.get)(s).new_file.edit.preedit().is_some() {
        return false;
    }
    let repeat = tx.key_repeat();
    match &ev.key {
        Key::Escape => {
            if !repeat {
                (hooks.get_mut)(s).new_file.close();
            }
            true
        }
        Key::Enter if !ev.mods.alt && !ev.mods.shift => {
            // An empty or invalid name does nothing and the field stays open.
            if !repeat
                && let Some(name) = (hooks.get)(s).new_file.name()
                && (hooks.create)(s, name, tx)
            {
                (hooks.get_mut)(s).new_file.close();
            }
            true
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn typed(text: &str) -> NewFile {
        let mut n = NewFile::default();
        n.show();
        n.edit.replace_text(text);
        n
    }

    #[test]
    fn show_focuses_an_empty_field_and_close_resets_it() {
        let mut n = typed("draft");
        assert!(n.open && n.edit.focused);
        n.show();
        assert!(n.edit.text.is_empty(), "reopens empty");
        n.close();
        assert!(!n.open && !n.edit.focused);
    }

    #[test]
    fn enter_only_names_valid_input_and_trims() {
        assert_eq!(typed("  draft ").name().as_deref(), Some("draft"));
        for bad in ["", "   ", "a/b", ".hidden"] {
            assert_eq!(typed(bad).name(), None, "{bad:?}");
        }
    }

    #[test]
    fn blur_closes_the_field_but_focus_keeps_it() {
        let mut n = typed("x");
        assert!(!n.blur_close(), "stays open with focus");
        n.edit.focused = false;
        assert!(n.blur_close() && !n.open);
        assert!(!n.blur_close());
    }
}
