//! HeMi Launcher (installs and updates the HeMi apps): when it is on this PC, About offers it for the updates. It writes
//! where it is to HKCU\Software\HeMi\HeMiLauncher (value Path) at each start.

use crate::sys::wide;
use std::path::PathBuf;
use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_SZ, RegGetValueW};

/// The exe of the launcher, when it is still there.
pub fn exe() -> Option<PathBuf> {
    let (key, name) = (wide(r"Software\HeMi\HeMiLauncher"), wide("Path"));
    let mut buf = vec![0u16; 1024];
    let mut size = (buf.len() * 2) as u32;
    // SAFETY: NUL-terminated names that live for the call; `size` is the size of `buf` in bytes.
    let r = unsafe {
        RegGetValueW(HKEY_CURRENT_USER, key.as_ptr(), name.as_ptr(), RRF_RT_REG_SZ, std::ptr::null_mut(), buf.as_mut_ptr().cast(), &mut size)
    };
    if r != 0 {
        return None;
    }
    let len = buf.iter().position(|&c| c == 0).unwrap_or(0);
    let path = PathBuf::from(String::from_utf16_lossy(&buf[..len]));
    // only HeMi Launcher itself is started from this value, never another program it might point to
    let named_right = path.file_name().is_some_and(|n| n.eq_ignore_ascii_case("HeMiLauncher.exe"));
    (named_right && path.is_file()).then_some(path)
}

/// Opens the launcher (the running one comes to the front) and lets it look for updates.
pub fn open() -> std::io::Result<()> {
    let exe = exe().ok_or_else(|| std::io::Error::from(std::io::ErrorKind::NotFound))?;
    std::process::Command::new(exe).arg("--check").spawn().map(drop)
}
