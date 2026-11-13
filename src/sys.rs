//! Small Windows helpers: strings, a new window, the time and date of F5, the font of the editor.

use std::ptr::null;
use windows_sys::Win32::Globalization::{DATE_SHORTDATE, GetDateFormatEx, GetTimeFormatEx, TIME_NOSECONDS};

pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn from_wide(buf: &[u16]) -> String {
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..end])
}

/// Starts another HeMiPad (File → New window), with a file or empty.
pub fn new_window(file: Option<&std::path::Path>) {
    if let Ok(exe) = std::env::current_exe() {
        let mut cmd = std::process::Command::new(exe);
        if let Some(f) = file {
            cmd.arg(f);
        }
        let _ = cmd.spawn();
    }
}

/// The time and the date as Notepad inserts them with F5 ("14:05 12.11.2026" on a Czech Windows).
/// Opens a web page in the default browser (HeMiPad itself never goes online: only the browser does, when the user asks).
pub fn open_url(url: &str) {
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    let (verb, target) = (wide("open"), wide(url));
    unsafe { ShellExecuteW(std::ptr::null_mut(), verb.as_ptr(), target.as_ptr(), null(), null(), SW_SHOWNORMAL) };
}

pub fn time_date() -> String {
    let mut t = [0u16; 64];
    let mut d = [0u16; 64];
    unsafe {
        GetTimeFormatEx(null(), TIME_NOSECONDS, null(), null(), t.as_mut_ptr(), t.len() as i32);
        GetDateFormatEx(null(), DATE_SHORTDATE, null(), null(), d.as_mut_ptr(), d.len() as i32, null());
    }
    format!("{} {}", from_wide(&t), from_wide(&d))
}

// ---------------------------------------------------------------------- fonts

/// The font of the editor and of printing: Consolas 11 pt, as Notepad (not changeable – kept simple).
#[derive(Clone, PartialEq, Debug)]
pub struct FontChoice {
    pub face: String,
    pub size: f32,
    pub bold: bool,
    pub italic: bool,
}

impl Default for FontChoice {
    fn default() -> Self {
        FontChoice { face: "Consolas".into(), size: 11.0, bold: false, italic: false }
    }
}
