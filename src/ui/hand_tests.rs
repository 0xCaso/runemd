//! Feedback 2026-11-23 F1: the hand cursor wherever an element has a hover state
//! (docs/feedback/2026-11-23-json-images-chrome.md).

use std::path::Path;

use rune::{Cursor, Env, Key, KeyEvent, Mods, Runtime};

use crate::app::App;
use crate::doc::document::Document;

const DOC: &str = "# Title\n\nA [link](https://example.com) and `code`.\n\n```bash\nnpm start\n```\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\n## Second\n\n- [ ] task\n- [x] done\n";

fn open() -> Runtime<App> {
    let mut state = App::bare(crate::index::Library::load(Vec::new(), None));
    state.push_tab(Document::from_source(Path::new("one.md"), DOC));
    state.push_tab(Document::from_source(Path::new("two.md"), DOC));
    let mut rt = Runtime::new(state, crate::view, (1200.0, 900.0), Env::new(true, 2.0));
    rt.on_key = Some(crate::on_key);
    rt.frame(0.0);
    settle(&mut rt);
    rt
}

fn settle(rt: &mut Runtime<App>) {
    for _ in 0..2000 {
        if !rt.frame(1.0 / 120.0) {
            return;
        }
    }
}

fn cmd(rt: &mut Runtime<App>, c: char) {
    rt.key(KeyEvent { key: Key::Command(c), mods: Mods { cmd: true, ..Mods::default() } });
    settle(rt);
}

/// Every visible element that is pressable with a hover state shows the hand under the pointer
/// (or the cursor an element on the way up chose on purpose, like a column's resize edge).
fn sweep(rt: &mut Runtime<App>, screen: &str) -> usize {
    let hands: Vec<_> = rt.elements().into_iter().filter(|e| e.hand && !e.exiting && e.visible[2] > 2.0 && e.visible[3] > 2.0).collect();
    let mut wrong = Vec::new();
    for e in &hands {
        let (x, y) = (e.visible[0] + e.visible[2] / 2.0, e.visible[1] + e.visible[3] / 2.0);
        rt.pointer_move(x, y);
        rt.frame(1.0 / 60.0);
        let c = rt.cursor();
        if c != Cursor::Pointer {
            wrong.push(format!("{} ({:?}) at {x},{y}: {c:?}", e.key.as_deref().unwrap_or(&e.path), e.role));
        }
    }
    assert!(wrong.is_empty(), "{screen}: no hand over\n{}", wrong.join("\n"));
    hands.len()
}

#[test]
fn every_pressable_with_a_hover_shows_the_hand() {
    let mut rt = open();
    let n = sweep(&mut rt, "reading");
    assert!(n >= 4, "the tabs, the sidebar and the doc have pressables with a hover: {n}");
    cmd(&mut rt, ',');
    sweep(&mut rt, "settings");
}

/// A pressable with no hover state keeps the arrow: the hand promises the hover's click.
#[test]
fn the_reading_text_keeps_its_own_cursor() {
    let mut rt = open();
    let body = rt.elements().into_iter().find(|e| e.text.as_deref().is_some_and(|t| t.starts_with("A "))).expect("the paragraph");
    rt.pointer_move(body.visible[0] + 2.0, body.visible[1] + body.visible[3] / 2.0);
    rt.frame(1.0 / 60.0);
    assert_ne!(rt.cursor(), Cursor::Pointer, "plain text is not a hand");
}
