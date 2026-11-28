//! The optional log (Settings → Logging, off by default – without it no file is written): %AppData%\HeMi\HeMiPad\Logs.
//! Only what HeMiPad did – the paths of opened and saved files, the changes of the File Explorer menu and the errors –
//! never the text of a file.
//! The same in all HeMi apps: %AppData%\HeMi\<app>\Logs\log-2026-11-28.txt, one file a day (a day over 1 MB goes on in
//! log-2026-11-28-2.txt …), the last week kept (at most 10 MB), the local time, the version and Windows when it is
//! switched on, panics with where in the code, the folders of the user written as %USERPROFILE%, %AppData% and
//! %LocalAppData%.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

static ON: AtomicBool = AtomicBool::new(false);
/// One writer at a time.
static LOCK: Mutex<()> = Mutex::new(());

/// %AppData%\HeMi\HeMiPad\Logs
fn dir() -> Option<PathBuf> {
    Some(crate::settings::dir()?.join("Logs"))
}

/// The newest log (for "Open log"); today's file when there is none yet.
pub fn file() -> Option<PathBuf> {
    let dir = dir()?;
    newest(&dir).or_else(|| Some(today(&dir, &now().0)))
}

/// Logging on or off (from the settings); the change itself is written too.
pub fn set_enabled(on: bool) {
    let was = ON.load(Ordering::Relaxed);
    if was && !on {
        write("Logging off");
    }
    ON.store(on, Ordering::Relaxed);
    if on && !was {
        write(&format!("Logging on – HeMiPad {} · {}", env!("CARGO_PKG_VERSION"), system()));
    }
}

/// A panic into the log too (with where in the code), then as before.
pub fn catch_panics() {
    let before = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        write(&format!("PANIC: {info}"));
        before(info);
    }));
}

/// A line with the date and time, when logging is on. Best effort: a log that cannot be written is left out silently.
pub fn write(text: &str) {
    if !ON.load(Ordering::Relaxed) {
        return;
    }
    let Some(dir) = dir() else { return };
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _ = std::fs::create_dir_all(&dir);
    let (day, time) = now();
    clean_up(&dir, &day);
    let line = format!("{day} {time}  {}\r\n", scrub(text));
    if let Ok(mut out) = std::fs::OpenOptions::new().create(true).append(true).open(today(&dir, &day)) {
        let _ = out.write_all(line.as_bytes());
    }
}

// ---------------------------------------------------------------- the files: one a day

/// A part of a day's log has at most about this much; then the day goes on in the next part (-2, -3 …).
const PART: u64 = 1024 * 1024;
/// The logs of the last week stay, together at most this much.
const KEEP: Duration = Duration::from_secs(7 * 24 * 3600);
const KEEP_BYTES: u64 = 10 * 1024 * 1024;
/// The day of the last clean-up (it runs at the first line of a day).
static CLEANED: Mutex<String> = Mutex::new(String::new());

/// The file of today's lines: log-2026-11-28.txt, once it has 1 MB log-2026-11-28-2.txt, -3 …
fn today(dir: &Path, day: &str) -> PathBuf {
    let mut n = 1;
    loop {
        let p = dir.join(if n == 1 { format!("log-{day}.txt") } else { format!("log-{day}-{n}.txt") });
        if n >= 99 || !std::fs::metadata(&p).is_ok_and(|m| m.len() >= PART) {
            return p;
        }
        n += 1;
    }
}

/// The logs in the folder: (name, path, size, age).
fn logs(dir: &Path) -> Vec<(String, PathBuf, u64, Option<Duration>)> {
    let Ok(read) = std::fs::read_dir(dir) else { return Vec::new() };
    read.flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let m = e.metadata().ok()?;
            let age = m.modified().ok().and_then(|t| t.elapsed().ok());
            (name.starts_with("log-") && name.ends_with(".txt") && m.is_file()).then(|| (name, e.path(), m.len(), age))
        })
        .collect()
}

/// The log written last.
fn newest(dir: &Path) -> Option<PathBuf> {
    logs(dir).into_iter().min_by_key(|l| l.3.unwrap_or(Duration::MAX)).map(|l| l.1)
}

/// Once a day: the logs older than a week, the oldest beyond 10 MB together, and log.txt of older versions.
fn clean_up(dir: &Path, day: &str) {
    {
        let mut last = CLEANED.lock().unwrap_or_else(|e| e.into_inner());
        if *last == day {
            return;
        }
        *last = day.to_string();
    }
    if let Some(home) = dir.parent() {
        for old in ["log.txt", "log.old.txt"] {
            let _ = std::fs::remove_file(home.join(old));
        }
    }
    let mut all = logs(dir);
    all.sort_by(|a, b| b.0.cmp(&a.0)); // newest first: the date is in the name
    let mut total = 0;
    for (_, path, len, age) in all {
        total += len;
        if age.is_some_and(|a| a > KEEP) || total > KEEP_BYTES {
            let _ = std::fs::remove_file(path);
        }
    }
}

// ---------------------------------------------------------------- what is written

/// The local date and time, ("2026-11-26", "14:03:05").
fn now() -> (String, String) {
    use windows_sys::Win32::System::SystemInformation::GetLocalTime;
    // SAFETY: GetLocalTime only fills the SYSTEMTIME it is given.
    let t = unsafe {
        let mut t = std::mem::zeroed();
        GetLocalTime(&mut t);
        t
    };
    (format!("{:04}-{:02}-{:02}", t.wYear, t.wMonth, t.wDay), format!("{:02}:{:02}:{:02}", t.wHour, t.wMinute, t.wSecond))
}

/// "Windows 11 25H2, build 26200.7171, x86_64" – a log sent from another PC says where it ran.
fn system() -> String {
    let build = reg("CurrentBuild", false).unwrap_or_default();
    let mut product = reg("ProductName", false).unwrap_or_else(|| "Windows".into());
    if build.parse::<u32>().is_ok_and(|b| b >= 22000) {
        product = product.replace("Windows 10", "Windows 11"); // the registry still says 10
    }
    let display = reg("DisplayVersion", false).unwrap_or_default();
    let ubr = reg("UBR", true).unwrap_or_default();
    format!("{product} {display}, build {build}.{ubr}, {}", std::env::consts::ARCH)
}

/// A value of HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion (a text, or a number when `dword`).
fn reg(name: &str, dword: bool) -> Option<String> {
    use windows_sys::Win32::System::Registry::{HKEY_LOCAL_MACHINE, RRF_RT_REG_DWORD, RRF_RT_REG_SZ, RegGetValueW};
    let key: Vec<u16> = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion".encode_utf16().chain([0]).collect();
    let value: Vec<u16> = name.encode_utf16().chain([0]).collect();
    let mut buf = [0u16; 128];
    let mut size = std::mem::size_of_val(&buf) as u32;
    let flags = if dword { RRF_RT_REG_DWORD } else { RRF_RT_REG_SZ };
    // SAFETY: null-terminated key and value names, a buffer and its size in bytes
    let r = unsafe {
        RegGetValueW(HKEY_LOCAL_MACHINE, key.as_ptr(), value.as_ptr(), flags, std::ptr::null_mut(), buf.as_mut_ptr().cast(), &mut size)
    };
    if r != 0 {
        return None;
    }
    if dword {
        return Some((u32::from(buf[0]) | (u32::from(buf[1]) << 16)).to_string());
    }
    let n = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    Some(String::from_utf16_lossy(&buf[..n]))
}

/// One line without the folders and the name of the user.
fn scrub(text: &str) -> String {
    let mut s = text.replace(['\r', '\n'], " ");
    // the longer folders first (both AppData folders are inside the profile)
    for (var, name) in [("LOCALAPPDATA", "%LocalAppData%"), ("APPDATA", "%AppData%"), ("USERPROFILE", "%USERPROFILE%")] {
        if let Some(v) = std::env::var_os(var).map(|v| v.to_string_lossy().trim_end_matches('\\').to_string())
            && v.len() > 3
        {
            s = replace_ci(&s, &v, name);
        }
    }
    // the name only as a folder of Users (a name like "HeMi" is also a part of the names of the apps)
    if let Ok(user) = std::env::var("USERNAME")
        && !user.is_empty()
    {
        s = replace_ci(&s, &format!(r"\Users\{user}"), r"\Users\<user>");
    }
    s
}

/// `needle` replaced in any case of its ASCII letters (paths of Windows).
fn replace_ci(hay: &str, needle: &str, with: &str) -> String {
    let mut out = String::with_capacity(hay.len());
    let mut i = 0;
    while i < hay.len() {
        if let Some(part) = hay.get(i..i + needle.len())
            && part.eq_ignore_ascii_case(needle)
        {
            out.push_str(with);
            i += needle.len();
            continue;
        }
        let Some(c) = hay[i..].chars().next() else { break };
        out.push(c);
        i += c.len_utf8();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_user_folder_is_left_out() {
        assert_eq!(replace_ci(r"C:\USERS\Jan\a.txt – č", r"c:\users\jan", "%USERPROFILE%"), r"%USERPROFILE%\a.txt – č");
        assert_eq!(scrub("a\r\nb"), "a  b");
    }

    #[test]
    fn one_file_a_day_in_parts() {
        let dir = std::env::temp_dir().join(format!("hemi-log-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(today(&dir, "2026-11-28"), dir.join("log-2026-11-28.txt"));
        std::fs::write(dir.join("log-2026-11-28.txt"), vec![b'x'; PART as usize]).unwrap();
        assert_eq!(today(&dir, "2026-11-28"), dir.join("log-2026-11-28-2.txt"));
        assert_eq!(newest(&dir), Some(dir.join("log-2026-11-28.txt")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_time_and_the_system() {
        let (day, time) = now();
        assert_eq!((day.len(), time.len()), (10, 8));
        assert!(system().contains("build"), "{}", system());
    }
}
