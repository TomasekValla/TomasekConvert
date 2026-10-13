//! Jazyk rozhraní (angličtina je výchozí). Makra `tr!` a `trf!` vybírají text podle jazyka.
use std::sync::atomic::{AtomicBool, Ordering};

static ENGLISH: AtomicBool = AtomicBool::new(true);

pub fn set_english(en: bool) {
    ENGLISH.store(en, Ordering::Relaxed);
}

pub fn is_en() -> bool {
    ENGLISH.load(Ordering::Relaxed)
}

/// `tr!("česky", "english")` -> &'static str podle zvoleného jazyka.
#[macro_export]
macro_rules! tr {
    ($cs:expr, $en:expr) => {
        if $crate::i18n::is_en() {
            $en
        } else {
            $cs
        }
    };
}

/// `trf!("česky {x}", "english {x}", args…)` -> String (obě šablony se kontrolují při kompilaci).
#[macro_export]
macro_rules! trf {
    ($cs:literal, $en:literal $(, $arg:expr)* $(,)?) => {
        if $crate::i18n::is_en() {
            format!($en $(, $arg)*)
        } else {
            format!($cs $(, $arg)*)
        }
    };
}
