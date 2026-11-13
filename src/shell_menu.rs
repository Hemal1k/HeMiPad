//! The optional item "Open in HeMiPad" in the File Explorer menu of files (the classic menu; in Windows 11
//! under "Show more options"). Only in the registry of the user (HKCU\Software\Classes), so no administrator
//! rights are needed; off by default, switched in Settings.

use crate::i18n::tr;
use crate::sys::wide;
use std::ptr::{null, null_mut};
use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
use windows_sys::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_WRITE, REG_SZ, RRF_RT_REG_SZ, RegCloseKey, RegCreateKeyExW, RegDeleteKeyW,
    RegDeleteTreeW, RegGetValueW, RegSetValueExW,
};

/// The key of the item, for every file type.
const KEY: &str = r"Software\Classes\*\shell\HeMiPad";

fn text() -> &'static str {
    tr("Open in HeMiPad", "Otevřít v HeMiPadu")
}

fn command() -> Option<(String, String)> {
    let exe = std::env::current_exe().ok()?.display().to_string();
    Some((format!("\"{exe}\" \"%1\""), format!("\"{exe}\",0")))
}

fn read(key: &str) -> Option<String> {
    let k = wide(key);
    let mut buf = vec![0u16; 2048];
    let mut size = (buf.len() * 2) as u32;
    let r = unsafe { RegGetValueW(HKEY_CURRENT_USER, k.as_ptr(), null(), RRF_RT_REG_SZ, null_mut(), buf.as_mut_ptr() as _, &mut size) };
    (r == ERROR_SUCCESS).then(|| {
        let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        String::from_utf16_lossy(&buf[..end])
    })
}

fn write(key: &str, name: Option<&str>, value: &str) -> Result<(), String> {
    let k = wide(key);
    let n = name.map(wide);
    let v = wide(value);
    unsafe {
        let mut hkey: HKEY = null_mut();
        let r = RegCreateKeyExW(HKEY_CURRENT_USER, k.as_ptr(), 0, null(), 0, KEY_WRITE, null(), &mut hkey, null_mut());
        if r != ERROR_SUCCESS {
            return Err(write_error(r));
        }
        let r = RegSetValueExW(hkey, n.as_ref().map_or(null(), |n| n.as_ptr()), 0, REG_SZ, v.as_ptr() as *const u8, (v.len() * 2) as u32);
        RegCloseKey(hkey);
        if r != ERROR_SUCCESS {
            return Err(write_error(r));
        }
    }
    Ok(())
}

fn write_error(r: u32) -> String {
    format!("{} ({r}).", tr("Writing to the registry failed", "Zápis do registru se nepodařil – chyba"))
}

pub fn is_on() -> bool {
    read(&format!(r"{KEY}\command")).is_some()
}

pub fn set(on: bool) -> Result<(), String> {
    if on {
        let (cmd, icon) = command().ok_or(tr("Could not find out where HeMiPad is.", "Nepodařilo se zjistit, kde je HeMiPad."))?;
        write(KEY, None, text())?;
        write(KEY, Some("Icon"), &icon)?;
        write(&format!(r"{KEY}\command"), None, &cmd)?;
    } else {
        let k = wide(KEY);
        let r = unsafe { RegDeleteTreeW(HKEY_CURRENT_USER, k.as_ptr()) };
        if r != ERROR_SUCCESS && r != ERROR_FILE_NOT_FOUND {
            return Err(format!("{} ({r}).", tr("The item could not be removed", "Položku se nepodařilo odebrat – chyba")));
        }
        let _ = unsafe { RegDeleteKeyW(HKEY_CURRENT_USER, k.as_ptr()) };
    }
    Ok(())
}

/// The item is on but points to another place (HeMiPad was moved) or has the text of the other language: write it again.
pub fn repair() {
    if let (true, Some((cmd, _))) = (is_on(), command())
        && (read(&format!(r"{KEY}\command")).as_deref() != Some(cmd.as_str()) || read(KEY).as_deref() != Some(text()))
    {
        let _ = set(true);
    }
}
