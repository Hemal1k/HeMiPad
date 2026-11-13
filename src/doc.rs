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

fn code_page(cp: u32, bytes: &[u8]) -> String {
    if bytes.is_empty() {
        return String::new();
    }
    unsafe {
        let n = MultiByteToWideChar(cp, 0, bytes.as_ptr(), bytes.len() as i32, null_mut(), 0);
        let mut w = vec![0u16; n.max(0) as usize];
        MultiByteToWideChar(cp, 0, bytes.as_ptr(), bytes.len() as i32, w.as_mut_ptr(), n);
        String::from_utf16_lossy(&w)
    }
}

/// Text in a code page; true when some characters could not be written in it.
fn to_code_page(cp: u32, s: &str) -> (Vec<u8>, bool) {
    let w: Vec<u16> = s.encode_utf16().collect();
    if w.is_empty() {
        return (Vec::new(), false);
    }
    unsafe {
        let mut lossy = 0i32;
        let n = WideCharToMultiByte(cp, WC_NO_BEST_FIT_CHARS, w.as_ptr(), w.len() as i32, null_mut(), 0, std::ptr::null(), &mut lossy);
        let mut out = vec![0u8; n.max(0) as usize];
        WideCharToMultiByte(cp, WC_NO_BEST_FIT_CHARS, w.as_ptr(), w.len() as i32, out.as_mut_ptr(), n, std::ptr::null(), &mut lossy);
        (out, lossy != 0)
    }
}

fn utf16(bytes: &[u8], big_endian: bool) -> String {
    let w: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|c| if big_endian { u16::from_be_bytes([c[0], c[1]]) } else { u16::from_le_bytes([c[0], c[1]]) })
        .collect();
    String::from_utf16_lossy(&w)
}

/// UTF-16 without a BOM: most of the bytes at one of the positions are zero (text in Latin letters).
fn guess_utf16(bytes: &[u8]) -> Option<bool> {
    if bytes.len() < 4 || bytes.len() % 2 != 0 {
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

/// The text of the bytes of a file; `forced` = read in this encoding (File → Reopen with encoding).
pub fn decode(bytes: &[u8], forced: Option<Encoding>) -> (String, Encoding) {
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
    let text = match enc {
        Encoding::Utf8 | Encoding::Utf8Bom => {
            String::from_utf8_lossy(bytes.strip_prefix(&[0xEF, 0xBB, 0xBF][..]).unwrap_or(bytes)).into_owned()
        }
        Encoding::Utf16Le => utf16(bytes.strip_prefix(&[0xFF, 0xFE][..]).unwrap_or(bytes), false),
        Encoding::Utf16Be => utf16(bytes.strip_prefix(&[0xFE, 0xFF][..]).unwrap_or(bytes), true),
        Encoding::Ansi => code_page(CP_ACP, bytes),
        Encoding::Oem => code_page(CP_OEMCP, bytes),
    };
    (text, enc)
}

pub fn load(path: &Path, forced: Option<Encoding>) -> std::io::Result<Loaded> {
    let bytes = std::fs::read(path)?;
    let (raw, encoding) = decode(&bytes, forced);
    let eol = detect_eol(&raw);
    Ok(Loaded { text: normalize(&raw), encoding, eol })
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
    std::fs::write(path, bytes)?;
    Ok(lossy)
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
                let (raw, got) = decode(&bytes, if matches!(enc, Encoding::Ansi | Encoding::Oem) { Some(enc) } else { None });
                assert_eq!(normalize(&raw), if matches!(enc, Encoding::Oem | Encoding::Ansi) { normalize(&raw) } else { text.to_string() });
                if !matches!(enc, Encoding::Ansi | Encoding::Oem) {
                    assert_eq!(got, enc);
                    assert_eq!(detect_eol(&raw), eol);
                }
            }
        }
    }
}
