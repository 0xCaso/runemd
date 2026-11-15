//! Document fonts: the choices in the settings panel (first Inter, New York and SF Pro,
//! then the installed families, from CoreText) and the Rune family for each.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use rune::Family;

/// The first choices, in this order.
pub const FIRST: [&str; 3] = ["Inter", "New York", "SF Pro"];

/// The installed families, in alphabetical order, without the hidden system families (the name
/// starts with "."), from `CTFontManagerCopyAvailableFontFamilyNames`.
pub fn installed() -> Vec<String> {
    let mut names = platform::family_names();
    names.retain(|n| !n.starts_with('.') && !n.trim().is_empty());
    names.sort_by_key(|n| n.to_lowercase());
    names.dedup();
    names
}

/// The panel's choices: `FIRST`, then the installed ones not already there. New York is there only if it is
/// installed under its public name (Apple's package): the system copy has a hidden
/// name (".New York") that Rune cannot find by name, and the choice would fall back to Inter.
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

/// The Rune family for the chosen name. Inter is the app's sans slot (in the bundle). SF Pro
/// tries the public name (installed by Apple's package), then the system font ("System
/// Font", which Rune can load). A name that is not found falls back to Inter (the sans slot).
pub fn family(name: &str) -> Family {
    match name {
        "Inter" => Family::Sans,
        "New York" => Family::Named("New York"),
        "SF Pro" => Family::Named("SF Pro, System Font"),
        other => Family::Named(intern(other)),
    }
}

/// `Family::Named` wants a `&'static str`: one name per family, kept for the whole life
/// of the app (there are few, and only the chosen ones).
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

/// Family and size of the document body.
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
        // SAFETY: the function returns a CFArrayRef of CFStringRef with a +1 (Copy), which is
        // toll-free bridged to NSArray<NSString>: `from_raw` takes that reference.
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
            "New York only if installed under its name"
        );
    }

    #[test]
    fn inter_is_the_sans_slot_and_others_are_named() {
        assert_eq!(family("Inter"), Family::Sans);
        assert_eq!(family("Georgia"), Family::Named("Georgia"));
        assert!(
            matches!(family("Georgia"), Family::Named(a) if std::ptr::eq(a, match family("Georgia") { Family::Named(b) => b, _ => "" })),
            "one name, a single copy"
        );
    }
}
