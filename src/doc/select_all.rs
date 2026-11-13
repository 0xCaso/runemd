//! Lo stato del Cmd+A sul documento: dati puri (id dei blocchi e byte), condivisi tra il
//! documento e le closure della vista. La logica che lo legge e lo scrive è in `ui::doc_view`.

use std::ops::Range;
use std::sync::{Mutex, PoisonError};

/// Se la selezione corrente viene da un Cmd+A (`Document::select_all`, condiviso con le closure
/// della vista).
#[derive(Debug, Default)]
pub struct SelectAllFlag(Mutex<SelectAll>);

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub enum SelectAll {
    #[default]
    No,
    /// Cmd+A premuto, la selezione che ne segue non è ancora arrivata.
    Pending,
    /// La selezione fatta dal Cmd+A, con le righe ai suoi capi.
    All(Ends),
}

/// I capi di una selezione: per la prima e l'ultima riga toccate, l'id del blocco e i byte
/// selezionati del suo testo; e se la selezione è arrivata troncata al tetto di copia di Rune
/// (allora l'ultima riga non è la fine).
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
    /// Lo stesso capo di `was`: stessa riga, e il testo intero o selezionato come allora (un
    /// ricaricamento può allungare il blocco sotto una selezione che non è cambiata).
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
