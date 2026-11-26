//! The optional log (Settings → Logging, off by default): %AppData%\HeMi\HeMiPad\log.txt. Only what HeMiPad did –
//! the paths of opened and saved files, the changes of the File Explorer menu and the errors – never the text of a
//! file. Kept under 1 MB: when it grows over, the older half is dropped.

use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

static ON: AtomicBool = AtomicBool::new(false);

/// The size the log may reach; then only its newer half stays.
const MAX: usize = 1 << 20;

pub fn set_enabled(on: bool) {
    ON.store(on, Ordering::Relaxed);
}

/// %AppData%\HeMi\HeMiPad\log.txt (it may not exist).
pub fn file() -> Option<PathBuf> {
    Some(crate::settings::dir()?.join("log.txt"))
}

/// One line in the log, with the date and the time, when logging is on. A failure is not shown: the log must
/// never get in the way of the work.
pub fn write(line: &str) {
    if !ON.load(Ordering::Relaxed) {
        return;
    }
    let Some(f) = file() else { return };
    if let Some(dir) = f.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let entry = format!("{} {}\r\n", now(), line.replace(['\r', '\n'], " "));
    if std::fs::metadata(&f).is_ok_and(|m| m.len() as usize + entry.len() > MAX)
        && let Ok(old) = std::fs::read(&f)
    {
        let _ = std::fs::write(&f, trim(&old, MAX / 2));
    }
    if let Ok(mut out) = std::fs::OpenOptions::new().create(true).append(true).open(&f) {
        let _ = out.write_all(entry.as_bytes());
    }
}

/// The end of the log of at most `keep` bytes, from the start of a line.
fn trim(log: &[u8], keep: usize) -> &[u8] {
    if log.len() <= keep {
        return log;
    }
    let tail = &log[log.len() - keep..];
    match tail.iter().position(|&b| b == b'\n') {
        Some(i) => &tail[i + 1..],
        None => &[],
    }
}

/// "2026-11-26 14:05:09" in the local time.
fn now() -> String {
    use windows_sys::Win32::System::SystemInformation::GetLocalTime;
    // SAFETY: GetLocalTime only fills the plain struct it is given.
    let t = unsafe {
        let mut t = std::mem::zeroed();
        GetLocalTime(&mut t);
        t
    };
    format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02}", t.wYear, t.wMonth, t.wDay, t.wHour, t.wMinute, t.wSecond)
}

#[cfg(test)]
mod tests {
    use super::trim;

    #[test]
    fn trim_keeps_whole_lines_of_the_end() {
        assert_eq!(trim(b"a\r\nb\r\n", 100), b"a\r\nb\r\n");
        assert_eq!(trim(b"aaaa\r\nbb\r\ncc\r\n", 8), b"cc\r\n");
        assert_eq!(trim(b"aaaaaaaa", 4), b"");
    }
}
