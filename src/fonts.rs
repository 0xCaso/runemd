//! I font del documento: le scelte del pannello delle impostazioni (prima Inter, New York e SF Pro,
//! poi le famiglie installate, da CoreText) e la famiglia di Rune per ognuna.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use rune::Family;

/// Le prime scelte, in quest'ordine (piano, task 27).
pub const FIRST: [&str; 3] = ["Inter", "New York", "SF Pro"];

/// Le famiglie installate, in ordine alfabetico, senza le famiglie nascoste di sistema (il nome
/// comincia con "."), da `CTFontManagerCopyAvailableFontFamilyNames`.
pub fn installed() -> Vec<String> {
    let mut names = platform::family_names();
    names.retain(|n| !n.starts_with('.') && !n.trim().is_empty());
    names.sort_by_key(|n| n.to_lowercase());
    names.dedup();
    names
}

/// Le scelte del pannello: `FIRST`, poi le installate che non ci sono già. New York c'è solo se è
/// installato col suo nome pubblico (il pacchetto di Apple): la copia di sistema ha un nome
/// nascosto (".New York") che Rune non trova per nome, e la scelta ricadrebbe su Inter.
pub fn choices(installed: &[String]) -> Vec<String> {
    let mut out: Vec<String> = FIRST
        .iter()
        .filter(|f| **f != "New York" || installed.iter().any(|n| n == *f))
        .map(|s| (*s).to_owned())
        .collect();
    out.extend(
        installed
            .iter()
            .filter(|n| !FIRST.contains(&n.as_str()))
            .cloned(),
    );
    out
}

/// La famiglia di Rune per il nome scelto. Inter è lo slot sans dell'app (nel bundle). SF Pro
/// prova il nome pubblico (installato dal pacchetto di Apple), poi il font di sistema ("System
/// Font", che Rune sa caricare). Un nome che non si trova ricade su Inter (lo slot sans).
pub fn family(name: &str) -> Family {
    match name {
        "Inter" => Family::Sans,
        "New York" => Family::Named("New York"),
        "SF Pro" => Family::Named("SF Pro, System Font"),
        other => Family::Named(intern(other)),
    }
}

/// `Family::Named` vuole un `&'static str`: un nome per famiglia, tenuto per tutta la vita
/// dell'app (sono poche, e solo quelle scelte).
fn intern(name: &str) -> &'static str {
    static NAMES: OnceLock<Mutex<HashMap<String, &'static str>>> = OnceLock::new();
    let mut names = NAMES
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    names
        .entry(name.to_owned())
        .or_insert_with(|| Box::leak(name.to_owned().into_boxed_str()))
}

/// Famiglia e dimensione del corpo del documento.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Typeface {
    pub family: Family,
    pub size: f32,
}

impl Typeface {
    pub fn of(name: &str, size: f32) -> Typeface {
        Typeface {
            family: family(name),
            size,
        }
    }
}

#[cfg(target_vendor = "apple")]
mod platform {
    use std::ffi::c_void;

    use objc2::rc::Retained;
    use objc2_foundation::{NSArray, NSString};

    #[link(name = "CoreText", kind = "framework")]
    unsafe extern "C" {
        fn CTFontManagerCopyAvailableFontFamilyNames() -> *mut c_void;
    }

    pub fn family_names() -> Vec<String> {
        // SAFETY: la funzione restituisce un CFArrayRef di CFStringRef con un +1 (Copy), che è
        // toll-free bridged a NSArray<NSString>: `from_raw` prende quel riferimento.
        let array: Option<Retained<NSArray<NSString>>> =
            unsafe { Retained::from_raw(CTFontManagerCopyAvailableFontFamilyNames().cast()) };
        array
            .map(|a| a.iter().map(|s| s.to_string()).collect())
            .unwrap_or_default()
    }
}

#[cfg(not(target_vendor = "apple"))]
mod platform {
    pub fn family_names() -> Vec<String> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installed_lists_visible_families_sorted() {
        let names = installed();
        assert!(
            names.iter().any(|n| n == "Helvetica" || n == "Menlo"),
            "{names:?}"
        );
        assert!(!names.iter().any(|n| n.starts_with('.')));
        let lower: Vec<String> = names.iter().map(|n| n.to_lowercase()).collect();
        assert!(lower.windows(2).all(|w| w[0] <= w[1]));
    }

    #[test]
    fn choices_start_with_inter_new_york_sf_pro() {
        let c = choices(&[
            "Arial".into(),
            "New York".into(),
            "SF Pro".into(),
            "Zapfino".into(),
        ]);
        assert_eq!(c, ["Inter", "New York", "SF Pro", "Arial", "Zapfino"]);
        let c = choices(&["Arial".into()]);
        assert_eq!(
            c,
            ["Inter", "SF Pro", "Arial"],
            "New York solo se installato col suo nome"
        );
    }

    #[test]
    fn inter_is_the_sans_slot_and_others_are_named() {
        assert_eq!(family("Inter"), Family::Sans);
        assert_eq!(family("Georgia"), Family::Named("Georgia"));
        assert!(
            matches!(family("Georgia"), Family::Named(a) if std::ptr::eq(a, match family("Georgia") { Family::Named(b) => b, _ => "" })),
            "un nome, una sola copia"
        );
    }
}
