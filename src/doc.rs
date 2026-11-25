//! Reading and writing text files: the encoding (UTF-8 with or without BOM, UTF-16 LE / BE, ANSI, OEM)
//! and the line endings (CRLF, LF, CR). In memory the text always has "\n" only.

use crate::i18n::tr;
use std::path::Path;
use std::ptr::null_mut;
use windows_sys::Win32::Globalization::{
    CP_ACP, CP_OEMCP, MultiByteToWideChar, WC_NO_BEST_FIT_CHARS, WideCharToMultiByte,
};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Encoding {
    Utf8,
    Utf8Bom,
    Utf16Le,
    Utf16Be,
    Ansi,
    Oem,
}

impl Encoding {
    pub const ALL: [Encoding; 6] =
        [Encoding::Utf8, Encoding::Utf8Bom, Encoding::Utf16Le, Encoding::Utf16Be, Encoding::Ansi, Encoding::Oem];

    pub fn label(self) -> &'static str {
        match self {
            Encoding::Utf8 => "UTF-8",
            Encoding::Utf8Bom => tr("UTF-8 with BOM", "UTF-8 s BOM"),
            Encoding::Utf16Le => "UTF-16 LE",
            Encoding::Utf16Be => "UTF-16 BE",
            Encoding::Ansi => "ANSI",
            Encoding::Oem => "OEM",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Eol {
    CrLf,
    Lf,
    Cr,
}

impl Eol {
    pub const ALL: [Eol; 3] = [Eol::CrLf, Eol::Lf, Eol::Cr];

    pub fn label(self) -> &'static str {
        match self {
            Eol::CrLf => "Windows (CRLF)",
            Eol::Lf => "Unix (LF)",
            Eol::Cr => "Macintosh (CR)",
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Eol::CrLf => "\r\n",
            Eol::Lf => "\n",
            Eol::Cr => "\r",
        }
    }
}

pub struct Loaded {
    pub text: String,
    pub encoding: Encoding,
    pub eol: Eol,
    /// Some bytes could not be read in the encoding (they show as "�"; saving would change them).
    pub lossy: bool,
}

/// The most frequent line ending of the text (CRLF when there is none).
pub fn detect_eol(s: &str) -> Eol {
    let b = s.as_bytes();
    let (mut crlf, mut lf, mut cr) = (0usize, 0usize, 0usize);
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'\r' if b.get(i + 1) == Some(&b'\n') => {
                crlf += 1;
                i += 1;
            }
            b'\r' => cr += 1,
            b'\n' => lf += 1,
            _ => {}
        }
        i += 1;
    }
    if lf > crlf && lf >= cr {
        Eol::Lf
    } else if cr > crlf && cr > lf {
        Eol::Cr
    } else {
        Eol::CrLf
    }
}

/// Every line ending as "\n".
pub fn normalize(s: &str) -> String {
    if !s.contains('\r') {
        return s.to_string();
    }
    s.replace("\r\n", "\n").replace('\r', "\n")
}

/// Files bigger than this are not opened (the whole text is kept in memory several times).
pub const MAX_OPEN: u64 = 1 << 30;
/// Files bigger than this are opened only after a question (it takes a while and a lot of memory).
pub const ASK_OPEN: u64 = 64 << 20;

fn code_page(cp: u32, bytes: &[u8]) -> String {
    if bytes.is_empty() {
        return String::new();
    }
    // never reached (MAX_OPEN is far below 2 GiB); a negative length would mean "up to a NUL" to the API
    let Ok(len) = i32::try_from(bytes.len()) else { return String::from_utf8_lossy(bytes).into_owned() };
    // SAFETY: the input is `len` bytes long; the first call only measures, the second writes into a buffer
    // of exactly the measured size.
    unsafe {
        let n = MultiByteToWideChar(cp, 0, bytes.as_ptr(), len, null_mut(), 0);
        let mut w = vec![0u16; n.max(0) as usize];
        MultiByteToWideChar(cp, 0, bytes.as_ptr(), len, w.as_mut_ptr(), n);
        String::from_utf16_lossy(&w)
    }
}

/// Text in a code page; true when some characters could not be written in it.
fn to_code_page(cp: u32, s: &str) -> (Vec<u8>, bool) {
    let w: Vec<u16> = s.encode_utf16().collect();
    if w.is_empty() {
        return (Vec::new(), false);
    }
    // a text of 2^31 UTF-16 units and more cannot go through the API in one call: it stays UTF-8 and counts
    // as lossy, so the user is asked before it is written
    let Ok(len) = i32::try_from(w.len()) else { return (s.as_bytes().to_vec(), true) };
    // SAFETY: the input is `len` units long; the first call only measures, the second writes into a buffer
    // of exactly the measured size; `lossy` is a local out value.
    unsafe {
        let mut lossy = 0i32;
        let n = WideCharToMultiByte(cp, WC_NO_BEST_FIT_CHARS, w.as_ptr(), len, null_mut(), 0, std::ptr::null(), &mut lossy);
        if n <= 0 {
            // the API refused (e.g. the code page of Windows is UTF-8, which takes no "lossy" flag): rather
            // the text as UTF-8, marked lossy so the user is asked, than an empty file
            return (s.as_bytes().to_vec(), true);
        }
        let mut out = vec![0u8; n.max(0) as usize];
        WideCharToMultiByte(cp, WC_NO_BEST_FIT_CHARS, w.as_ptr(), len, out.as_mut_ptr(), n, std::ptr::null(), &mut lossy);
        (out, lossy != 0)
    }
}

/// The text of UTF-16 bytes; true when something could not be read (an odd last byte, a lone surrogate).
fn utf16(bytes: &[u8], big_endian: bool) -> (String, bool) {
    let (pairs, rest) = bytes.as_chunks::<2>();
    let w: Vec<u16> = pairs.iter().map(|&c| if big_endian { u16::from_be_bytes(c) } else { u16::from_le_bytes(c) }).collect();
    match String::from_utf16(&w) {
        Ok(s) => (s, !rest.is_empty()),
        Err(_) => (String::from_utf16_lossy(&w), true),
    }
}

/// UTF-16 without a BOM: most of the bytes at one of the positions are zero (text in Latin letters).
fn guess_utf16(bytes: &[u8]) -> Option<bool> {
    if bytes.len() < 4 || !bytes.len().is_multiple_of(2) {
        return None;
    }
    let sample = &bytes[..bytes.len().min(4096)];
    let pairs = sample.len() / 2;
    let even_zero = sample.iter().step_by(2).filter(|&&b| b == 0).count();
    let odd_zero = sample.iter().skip(1).step_by(2).filter(|&&b| b == 0).count();
    if odd_zero * 10 >= pairs * 6 && even_zero * 10 <= pairs {
        Some(false)
    } else if even_zero * 10 >= pairs * 6 && odd_zero * 10 <= pairs {
        Some(true)
    } else {
        None
    }
}

/// The text of the bytes of a file; `forced` = read in this encoding (a reload keeps the encoding of the document).
/// The flag is true when some bytes could not be read in the encoding (saving would change them).
pub fn decode(bytes: &[u8], forced: Option<Encoding>) -> (String, Encoding, bool) {
    let enc = forced.unwrap_or_else(|| {
        if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
            Encoding::Utf8Bom
        } else if bytes.starts_with(&[0xFF, 0xFE]) {
            Encoding::Utf16Le
        } else if bytes.starts_with(&[0xFE, 0xFF]) {
            Encoding::Utf16Be
        } else if std::str::from_utf8(bytes).is_ok() {
            Encoding::Utf8
        } else if let Some(be) = guess_utf16(bytes) {
            if be { Encoding::Utf16Be } else { Encoding::Utf16Le }
        } else {
            Encoding::Ansi
        }
    });
    let (text, lossy) = match enc {
        Encoding::Utf8 | Encoding::Utf8Bom => {
            let b = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF][..]).unwrap_or(bytes);
            match std::str::from_utf8(b) {
                Ok(s) => (s.to_string(), false),
                Err(_) => (String::from_utf8_lossy(b).into_owned(), true),
            }
        }
        Encoding::Utf16Le => utf16(bytes.strip_prefix(&[0xFF, 0xFE][..]).unwrap_or(bytes), false),
        Encoding::Utf16Be => utf16(bytes.strip_prefix(&[0xFE, 0xFF][..]).unwrap_or(bytes), true),
        // every byte means some character in a code page
        Encoding::Ansi => (code_page(CP_ACP, bytes), false),
        Encoding::Oem => (code_page(CP_OEMCP, bytes), false),
    };
    (text, enc, lossy)
}

/// The size of the file in bytes (0 when it cannot be read).
pub fn size(path: &Path) -> u64 {
    std::fs::metadata(path).map_or(0, |m| m.len())
}

pub fn load(path: &Path, forced: Option<Encoding>) -> std::io::Result<Loaded> {
    if size(path) > MAX_OPEN {
        return Err(std::io::Error::other(tr(
            "The file is too big for HeMiPad (more than 1 GB).",
            "Soubor je pro HeMiPad příliš velký (víc než 1 GB).",
        )));
    }
    let bytes = std::fs::read(path)?;
    let (raw, encoding, lossy) = decode(&bytes, forced);
    let eol = detect_eol(&raw);
    Ok(Loaded { text: normalize(&raw), encoding, eol, lossy })
}

/// True when some characters of the text cannot be written in the encoding (ANSI / OEM).
pub fn loses_characters(text: &str, enc: Encoding) -> bool {
    match enc {
        Encoding::Ansi => to_code_page(CP_ACP, text).1,
        Encoding::Oem => to_code_page(CP_OEMCP, text).1,
        _ => false,
    }
}

/// The bytes of the text in the encoding and with the line endings; true when characters were lost (ANSI / OEM).
pub fn encode(text: &str, enc: Encoding, eol: Eol) -> (Vec<u8>, bool) {
    let text = normalize(text);
    let text = if eol == Eol::Lf { text } else { text.replace('\n', eol.as_str()) };
    match enc {
        Encoding::Utf8 => (text.into_bytes(), false),
        Encoding::Utf8Bom => {
            let mut v = vec![0xEF, 0xBB, 0xBF];
            v.extend_from_slice(text.as_bytes());
            (v, false)
        }
        Encoding::Utf16Le | Encoding::Utf16Be => {
            let be = enc == Encoding::Utf16Be;
            let mut v = if be { vec![0xFE, 0xFF] } else { vec![0xFF, 0xFE] };
            for u in text.encode_utf16() {
                v.extend_from_slice(&if be { u.to_be_bytes() } else { u.to_le_bytes() });
            }
            (v, false)
        }
        Encoding::Ansi => to_code_page(CP_ACP, &text),
        Encoding::Oem => to_code_page(CP_OEMCP, &text),
    }
}

/// Writes the file; true when some characters could not be written in the encoding.
pub fn save(path: &Path, text: &str, enc: Encoding, eol: Eol) -> std::io::Result<bool> {
    let (bytes, lossy) = encode(text, enc, eol);
    write_atomic(path, &bytes)?;
    Ok(lossy)
}

/// Writes the bytes so that a crash, a power cut or a full disk never leaves a half-written file:
/// first into a temporary file in the same folder, then it replaces the file (ReplaceFileW keeps
/// the attributes, the permissions and the creation time). A link is written in place as before.
fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::fs::OpenOptions;
    use std::io::{ErrorKind, Write};

    let exists = match std::fs::symlink_metadata(path) {
        Ok(m) if m.file_type().is_symlink() => return std::fs::write(path, bytes),
        Ok(_) => true,
        Err(e) if e.kind() == ErrorKind::NotFound => false,
        Err(_) => return std::fs::write(path, bytes),
    };
    if exists {
        // the same error as before for a read-only, locked or protected file (nothing is changed)
        OpenOptions::new().write(true).open(path)?;
    }

    let dir = match path.parent() {
        Some(d) if !d.as_os_str().is_empty() => d,
        _ => Path::new("."),
    };
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let mut tmp = None;
    for i in 0..100u32 {
        let p = dir.join(format!(".{name}.{}-{i}.hemipad-tmp", std::process::id()));
        match OpenOptions::new().write(true).create_new(true).open(&p) {
            Ok(f) => {
                tmp = Some((p, f));
                break;
            }
            Err(e) if e.kind() == ErrorKind::AlreadyExists => continue,
            Err(_) => break,
        }
    }
    // no temporary file in the folder (no right to create files there): write directly as before
    let Some((tmp, mut f)) = tmp else { return std::fs::write(path, bytes) };

    let written = f.write_all(bytes).and_then(|_| f.sync_all());
    drop(f);
    if let Err(e) = written {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    if !replace_with(path, &tmp, exists) {
        // e.g. a file system without ReplaceFileW: the direct write as before
        let _ = std::fs::remove_file(&tmp);
        return std::fs::write(path, bytes);
    }
    Ok(())
}

/// Puts `tmp` in place of `path`; false when it did not work (then `tmp` is still there or gone).
fn replace_with(path: &Path, tmp: &Path, exists: bool) -> bool {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MOVEFILE_WRITE_THROUGH, MoveFileExW, REPLACEFILE_IGNORE_MERGE_ERRORS, ReplaceFileW,
    };
    let w = |p: &Path| p.as_os_str().encode_wide().chain(Some(0)).collect::<Vec<u16>>();
    let (target, source) = (w(path), w(tmp));
    // SAFETY: both paths are NUL-terminated and live for the call; the optional pointers are null.
    unsafe {
        if exists {
            ReplaceFileW(
                target.as_ptr(),
                source.as_ptr(),
                std::ptr::null(),
                REPLACEFILE_IGNORE_MERGE_ERRORS,
                std::ptr::null(),
                std::ptr::null(),
            ) != 0
        } else {
            // a new file: just rename (without replacing a file that appeared in the meantime)
            MoveFileExW(source.as_ptr(), target.as_ptr(), MOVEFILE_WRITE_THROUGH) != 0
        }
    }
}

/// When the file was last written (to notice changes made by another program).
pub fn modified_time(path: &Path) -> Option<std::time::SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        for enc in Encoding::ALL {
            for eol in Eol::ALL {
                let text = "a\nb\n\nč";
                let (bytes, _) = encode(text, enc, eol);
                let (raw, got, lossy) = decode(&bytes, if matches!(enc, Encoding::Ansi | Encoding::Oem) { Some(enc) } else { None });
                assert_eq!(normalize(&raw), if matches!(enc, Encoding::Oem | Encoding::Ansi) { normalize(&raw) } else { text.to_string() });
                assert!(!lossy);
                if !matches!(enc, Encoding::Ansi | Encoding::Oem) {
                    assert_eq!(got, enc);
                    assert_eq!(detect_eol(&raw), eol);
                }
            }
        }
    }

    #[test]
    fn lossy_decoding() {
        // UTF-8 BOM and a byte that is not UTF-8
        let (_, enc, lossy) = decode(&[0xEF, 0xBB, 0xBF, b'a', 0xFF], None);
        assert_eq!((enc, lossy), (Encoding::Utf8Bom, true));
        // UTF-16 with an odd last byte
        assert!(decode(&[0xFF, 0xFE, b'a', 0, b'b'], None).2);
        // a lone surrogate
        assert!(decode(&[0xFF, 0xFE, 0x00, 0xD8], None).2);
        // good files
        assert!(!decode("čau".as_bytes(), None).2);
        assert!(!decode(&[0xFF, 0xFE, b'a', 0], None).2);
    }

    #[test]
    fn lost_characters() {
        assert!(!loses_characters("abc", Encoding::Ansi));
        assert!(loses_characters("😀", Encoding::Ansi));
        assert!(!loses_characters("😀", Encoding::Utf8));
    }

    fn temp_folder(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("hemipad-test-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn leftovers(dir: &Path) -> usize {
        std::fs::read_dir(dir)
            .unwrap()
            .filter(|e| e.as_ref().unwrap().file_name().to_string_lossy().ends_with(".hemipad-tmp"))
            .count()
    }

    #[test]
    fn save_new_and_overwrite() {
        let dir = temp_folder("save");
        let file = dir.join("a.txt");
        // a new file
        assert!(!save(&file, "one\ntwo", Encoding::Utf8, Eol::CrLf).unwrap());
        assert_eq!(std::fs::read(&file).unwrap(), b"one\r\ntwo");
        // over an existing, longer file
        std::fs::write(&file, "a much longer content than the new one").unwrap();
        save(&file, "x", Encoding::Utf8, Eol::Lf).unwrap();
        assert_eq!(std::fs::read(&file).unwrap(), b"x");
        assert_eq!(leftovers(&dir), 0);
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn save_read_only_fails() {
        let dir = temp_folder("ro");
        let file = dir.join("ro.txt");
        std::fs::write(&file, "keep").unwrap();
        let mut p = std::fs::metadata(&file).unwrap().permissions();
        p.set_readonly(true);
        std::fs::set_permissions(&file, p.clone()).unwrap();
        assert!(save(&file, "new", Encoding::Utf8, Eol::Lf).is_err());
        assert_eq!(std::fs::read(&file).unwrap(), b"keep");
        assert_eq!(leftovers(&dir), 0);
        #[allow(clippy::permissions_set_readonly_false)]
        p.set_readonly(false);
        std::fs::set_permissions(&file, p).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
