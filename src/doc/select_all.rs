//! The state of Cmd+A on the document: plain data (block ids and bytes), shared between the
//! document and the view's closures. The logic that reads and writes it is in `ui::doc_view`.

use std::ops::Range;
use std::sync::{Mutex, PoisonError};

/// Whether the current selection comes from a Cmd+A (`Document::select_all`, shared with the
/// view's closures).
#[derive(Debug, Default)]
pub struct SelectAllFlag(Mutex<SelectAll>);

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub enum SelectAll {
    #[default]
    No,
    /// Cmd+A pressed, the selection that follows has not arrived yet.
    Pending,
    /// The selection made by Cmd+A, with the rows at its ends.
    All(Ends),
}

/// The ends of a selection: for the first and last rows touched, the block id and the
/// selected bytes of its text; and whether the selection arrived truncated at Rune's copy cap
/// (then the last row is not the end).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ends {
    pub first: End,
    pub last: End,
    pub capped: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct End {
    pub row: u64,
    pub range: Range<usize>,
    pub whole: bool,
}

impl End {
    /// The same end as `was`: same row, and the whole or selected text as then (a
    /// reload can lengthen the block under a selection that has not changed).
    pub fn holds(&self, was: &End) -> bool {
        self.row == was.row && (self.whole || self.range == was.range)
    }
}

impl SelectAllFlag {
    pub fn get(&self) -> SelectAll {
        self.0.lock().unwrap_or_else(PoisonError::into_inner).clone()
    }

    pub fn set(&self, next: SelectAll) {
        *self.0.lock().unwrap_or_else(PoisonError::into_inner) = next;
    }
}
