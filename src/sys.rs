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
pub fn new_window(file: Option<&std::path::Path>) -> std::io::Result<()> {
    let mut cmd = std::process::Command::new(std::env::current_exe()?);
    if let Some(f) = file {
        cmd.arg(f);
    }
    cmd.spawn().map(drop)
}

/// %SystemRoot%\explorer.exe by the Windows folder itself – never a program named so in the current folder or PATH.
pub fn explorer() -> std::path::PathBuf {
    use windows_sys::Win32::System::SystemInformation::GetWindowsDirectoryW;
    let mut buf = [0u16; 260];
    // SAFETY: the buffer is local and its length in characters is passed.
    let n = unsafe { GetWindowsDirectoryW(buf.as_mut_ptr(), buf.len() as u32) } as usize;
    let dir = if n > 0 && n < buf.len() { String::from_utf16_lossy(&buf[..n]) } else { r"C:\Windows".into() };
    std::path::PathBuf::from(dir).join("explorer.exe")
}

/// Shows a folder in File Explorer.
pub fn open_folder(dir: &std::path::Path) -> std::io::Result<()> {
    std::process::Command::new(explorer()).arg(dir).spawn().map(drop)
}

/// Opens a web page in the default browser (HeMiPad itself never goes online: only the browser does, when the user asks).
pub fn open_url(url: &str) {
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    let (verb, target) = (wide("open"), wide(url));
    // SAFETY: the strings are NUL-terminated and live for the call; the other pointers may be null.
    unsafe { ShellExecuteW(std::ptr::null_mut(), verb.as_ptr(), target.as_ptr(), null(), null(), SW_SHOWNORMAL) };
}

/// The time and the date as Notepad inserts them with F5 ("14:05 12.11.2026" on a Czech Windows).
pub fn time_date() -> String {
    let mut t = [0u16; 64];
    let mut d = [0u16; 64];
    // SAFETY: the buffers are local and their length is passed, so the API writes inside them.
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

/// Windows 11 (build 22000 and newer): its new context menu shows the File Explorer item only under "Show more options".
pub fn windows_11() -> bool {
    use windows_sys::Win32::System::Registry::{HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ, RegGetValueW};
    let k = wide(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion");
    let n = wide("CurrentBuildNumber");
    let mut buf = [0u16; 32];
    let mut size = std::mem::size_of_val(&buf) as u32;
    // SAFETY: the names are NUL-terminated and live for the call; `size` is the size of `buf` in bytes.
    let r = unsafe { RegGetValueW(HKEY_LOCAL_MACHINE, k.as_ptr(), n.as_ptr(), RRF_RT_REG_SZ, std::ptr::null_mut(), buf.as_mut_ptr().cast(), &mut size) };
    if r != windows_sys::Win32::Foundation::ERROR_SUCCESS {
        return false;
    }
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..end]).trim().parse::<u32>().is_ok_and(|b| b >= 22000)
}
