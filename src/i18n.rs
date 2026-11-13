//! Languages: English; Czech when the display language of Windows is Czech.
//! The user can also pick the language by hand (View → Language), which is remembered.

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

/// The build date (YYYY-MM-DD from build.rs) as Czech or English write it.
pub fn build_date() -> String {
    let iso = env!("HEMIPAD_BUILD_DATE");
    let mut parts = iso.split('-');
    let (Some(y), Some(m), Some(d)) = (parts.next(), parts.next(), parts.next()) else { return iso.into() };
    if cz() {
        format!("{d}.{m}.{y}")
    } else {
        const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
        let month = m.parse::<usize>().ok().and_then(|m| MONTHS.get(m.wrapping_sub(1))).copied().unwrap_or(m);
        format!("{} {month} {y}", d.trim_start_matches('0'))
    }
}
