//! Languages: English; Czech when the display language of Windows is Czech.
//! The user can also pick the language by hand (Settings → Language), which is remembered.

use std::sync::atomic::{AtomicU8, Ordering};
use windows_sys::Win32::Globalization::GetUserDefaultUILanguage;

/// The language picked by hand: 0 = as Windows, 1 = English, 2 = Czech.
static CHOICE: AtomicU8 = AtomicU8::new(0);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Choice {
    Windows,
    English,
    Czech,
}

impl Choice {
    pub fn key(self) -> &'static str {
        match self {
            Choice::Windows => "auto",
            Choice::English => "en",
            Choice::Czech => "cs",
        }
    }

    pub fn from_key(k: &str) -> Choice {
        match k {
            "en" => Choice::English,
            "cs" => Choice::Czech,
            _ => Choice::Windows,
        }
    }
}

pub fn set(c: Choice) {
    CHOICE.store(c as u8, Ordering::Relaxed);
}

/// True when Windows shows its texts in Czech (primary language 0x05).
pub fn windows_cz() -> bool {
    // Debug builds only: HEMIPAD_LANG=en / cs to test the other language on this PC
    if cfg!(debug_assertions) {
        match std::env::var("HEMIPAD_LANG").as_deref() {
            Ok("en") => return false,
            Ok("cs") => return true,
            _ => {}
        }
    }
    (unsafe { GetUserDefaultUILanguage() } & 0x3FF) == 0x05
}

/// True when the app speaks Czech.
pub fn cz() -> bool {
    match CHOICE.load(Ordering::Relaxed) {
        1 => false,
        2 => true,
        _ => windows_cz(),
    }
}

/// The text in the language of the app.
pub fn tr<'a>(en: &'a str, cs: &'a str) -> &'a str {
    if cz() { cs } else { en }
}


/// The form for a count, by the Czech rules: 1 → `one`, 2–4 → `few`, anything else → `many`
/// ("1 znak", "3 znaky", "5 znaků"). For English give the plural as both `few` and `many`.
pub fn plural<'a>(n: usize, one: &'a str, few: &'a str, many: &'a str) -> &'a str {
    match n {
        1 => one,
        2..=4 => few,
        _ => many,
    }
}
