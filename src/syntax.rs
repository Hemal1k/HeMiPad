//! Colouring of JSON, YAML, XML, INI / TOML and Markdown. Simple scanners: they only colour,
//! they never change the text, and a broken file just gets fewer colours.

use crate::i18n::tr;
use eframe::egui::{Color32, FontId, TextFormat, text::ByteIndex, text::LayoutJob, text::LayoutSection};
use std::ops::Range;
use std::path::Path;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Lang {
    Plain,
    Json,
    Yaml,
    Xml,
    Ini,
    Markdown,
}

impl Lang {
    pub fn label(self) -> &'static str {
        match self {
            Lang::Plain => tr("Plain text", "Prostý text"),
            Lang::Json => "JSON",
            Lang::Yaml => "YAML",
            Lang::Xml => "XML / HTML",
            Lang::Ini => "INI / TOML",
            Lang::Markdown => "Markdown",
        }
    }

    pub fn from_path(path: &Path) -> Lang {
        let name = path.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
        let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
        match ext.as_str() {
            "json" | "jsonc" | "json5" | "geojson" | "har" | "webmanifest" | "ipynb" | "jsonl" | "ndjson" => Lang::Json,
            "yaml" | "yml" => Lang::Yaml,
            "xml" | "xaml" | "axaml" | "csproj" | "vbproj" | "fsproj" | "vcxproj" | "props" | "targets" | "config" | "svg"
            | "html" | "htm" | "xhtml" | "plist" | "resx" | "manifest" | "nuspec" | "slnx" | "xsd" | "xsl" | "xslt"
            | "wsf" | "rss" | "atom" | "kml" | "gpx" | "appxmanifest" => Lang::Xml,
            "ini" | "cfg" | "conf" | "inf" | "reg" | "toml" | "properties" | "editorconfig" | "gitconfig" | "desktop"
            | "url" | "lnk.ini" => Lang::Ini,
            "md" | "markdown" | "mdown" | "mkd" => Lang::Markdown,
            _ => match name.as_str() {
                ".editorconfig" | ".gitconfig" | ".npmrc" | "desktop.ini" => Lang::Ini,
                ".clang-format" => Lang::Yaml,
                _ => Lang::Plain,
            },
        }
    }

    /// Spelling is checked in prose only.
    pub fn is_prose(self) -> bool {
        matches!(self, Lang::Plain | Lang::Markdown)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Text,
    Key,
    Str,
    Number,
    Keyword,
    Comment,
    Punct,
    Tag,
    Attr,
    Section,
    Heading,
}

/// The colours of the kinds, after VS Code (Dark+ / Light+), which people know.
#[derive(Clone, Copy, PartialEq, Hash)]
pub struct Colors {
    text: Color32,
    key: Color32,
    string: Color32,
    number: Color32,
    keyword: Color32,
    comment: Color32,
    punct: Color32,
    tag: Color32,
    attr: Color32,
    section: Color32,
    heading: Color32,
}

impl Colors {
    pub fn new(light: bool, text: Color32, punct: Color32) -> Colors {
        let c = Color32::from_rgb;
        if light {
            Colors {
                text,
                key: c(0x04, 0x51, 0xA5),
                string: c(0xA3, 0x15, 0x15),
                number: c(0x09, 0x86, 0x58),
                keyword: c(0x00, 0x00, 0xFF),
                comment: c(0x00, 0x80, 0x00),
                punct,
                tag: c(0x80, 0x00, 0x00),
                attr: c(0xE5, 0x00, 0x00),
                section: c(0xAF, 0x00, 0xDB),
                heading: c(0x00, 0x00, 0xFF),
            }
        } else {
            Colors {
                text,
                key: c(0x9C, 0xDC, 0xFE),
                string: c(0xCE, 0x91, 0x78),
                number: c(0xB5, 0xCE, 0xA8),
                keyword: c(0x56, 0x9C, 0xD6),
                comment: c(0x6A, 0x99, 0x55),
                punct,
                tag: c(0x56, 0x9C, 0xD6),
                attr: c(0x9C, 0xDC, 0xFE),
                section: c(0xC5, 0x86, 0xC0),
                heading: c(0x56, 0x9C, 0xD6),
            }
        }
    }

    fn of(&self, k: Kind) -> Color32 {
        match k {
            Kind::Text => self.text,
            Kind::Key => self.key,
            Kind::Str => self.string,
            Kind::Number => self.number,
            Kind::Keyword => self.keyword,
            Kind::Comment => self.comment,
            Kind::Punct => self.punct,
            Kind::Tag => self.tag,
            Kind::Attr => self.attr,
            Kind::Section => self.section,
            Kind::Heading => self.heading,
        }
    }
}

/// The text laid out with colours. `wrap` = the width to wrap at (infinite = no wrapping).
pub fn layout_job(text: &str, lang: Lang, font: FontId, colors: &Colors, wrap: f32) -> LayoutJob {
    let spans = if lang == Lang::Plain || text.len() > 8_000_000 { Vec::new() } else { spans(lang, text) };
    let mut job = LayoutJob { text: text.to_string(), ..Default::default() };
    job.wrap.max_width = wrap;
    job.keep_trailing_whitespace = true;
    job.break_on_newline = true;
    let format = |k: Kind| TextFormat { font_id: font.clone(), color: colors.of(k), ..Default::default() };
    let mut push = |range: Range<usize>, k: Kind| {
        if range.is_empty() {
            return;
        }
        // neighbours in the same colour become one section
        if let Some(last) = job.sections.last_mut()
            && last.byte_range.end == ByteIndex(range.start)
            && last.format.color == colors.of(k)
        {
            last.byte_range.end = ByteIndex(range.end);
            return;
        }
        job.sections.push(LayoutSection { leading_space: 0.0, byte_range: ByteIndex(range.start)..ByteIndex(range.end), format: format(k) });
    };
    let mut at = 0;
    for (r, k) in spans {
        if r.start < at || r.end > text.len() {
            continue;
        }
        push(at..r.start, Kind::Text);
        push(r.clone(), k);
        at = r.end;
    }
    push(at..text.len(), Kind::Text);
    if job.sections.is_empty() {
        job.sections.push(LayoutSection { leading_space: 0.0, byte_range: ByteIndex(0)..ByteIndex(0), format: format(Kind::Text) });
    }
    job
}

/// Coloured byte ranges, sorted and not overlapping. Ranges only start and end at ASCII bytes,
/// so they are always on character boundaries.
fn spans(lang: Lang, text: &str) -> Vec<(Range<usize>, Kind)> {
    let mut out = Vec::new();
    match lang {
        Lang::Plain => {}
        Lang::Json => json(text.as_bytes(), &mut out),
        Lang::Xml => xml(text.as_bytes(), &mut out),
        Lang::Yaml | Lang::Ini | Lang::Markdown => {
            let mut start = 0;
            let mut fence = false;
            for line in text.split('\n') {
                let b = line.as_bytes();
                match lang {
                    Lang::Yaml => yaml_line(b, start, &mut out),
                    Lang::Ini => ini_line(b, start, &mut out),
                    _ => markdown_line(b, start, &mut fence, &mut out),
                }
                start += line.len() + 1;
            }
        }
    }
    out
}

fn is_word(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b >= 0x80
}

/// The end of a quoted string that starts at `i` (after the closing quote, or at the end of the line).
fn string_end(b: &[u8], i: usize) -> usize {
    let q = b[i];
    let mut j = i + 1;
    while j < b.len() {
        match b[j] {
            b'\\' if q == b'"' => j += 2,
            b'\n' => return j,
            c if c == q => return j + 1,
            _ => j += 1,
        }
    }
    b.len().min(j)
}

fn number_end(b: &[u8], i: usize) -> usize {
    let mut j = i;
    while j < b.len() && (b[j].is_ascii_alphanumeric() || matches!(b[j], b'.' | b'-' | b'+' | b'_')) {
        j += 1;
    }
    j
}

fn json(b: &[u8], out: &mut Vec<(Range<usize>, Kind)>) {
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        match c {
            b'"' => {
                let end = string_end(b, i);
                let mut k = end;
                while k < b.len() && matches!(b[k], b' ' | b'\t' | b'\r' | b'\n') {
                    k += 1;
                }
                out.push((i..end, if b.get(k) == Some(&b':') { Kind::Key } else { Kind::Str }));
                i = end;
            }
            b'/' if b.get(i + 1) == Some(&b'/') => {
                let end = b[i..].iter().position(|&x| x == b'\n').map_or(b.len(), |p| i + p);
                out.push((i..end, Kind::Comment));
                i = end;
            }
            b'/' if b.get(i + 1) == Some(&b'*') => {
                let end = find(b, i + 2, b"*/").map_or(b.len(), |p| p + 2);
                out.push((i..end, Kind::Comment));
                i = end;
            }
            b'-' | b'0'..=b'9' => {
                let end = number_end(b, i + 1);
                out.push((i..end, Kind::Number));
                i = end;
            }
            b'{' | b'}' | b'[' | b']' | b',' | b':' => {
                out.push((i..i + 1, Kind::Punct));
                i += 1;
            }
            c if c.is_ascii_alphabetic() => {
                let mut end = i;
                while end < b.len() && is_word(b[end]) {
                    end += 1;
                }
                if matches!(&b[i..end], b"true" | b"false" | b"null") {
                    out.push((i..end, Kind::Keyword));
                }
                i = end;
            }
            _ => i += 1,
        }
    }
}

fn find(b: &[u8], from: usize, what: &[u8]) -> Option<usize> {
    if from >= b.len() {
        return None;
    }
    b[from..].windows(what.len()).position(|w| w == what).map(|p| from + p)
}

fn xml(b: &[u8], out: &mut Vec<(Range<usize>, Kind)>) {
    let mut i = 0;
    while i < b.len() {
        if b[i..].starts_with(b"<!--") {
            let end = find(b, i + 4, b"-->").map_or(b.len(), |p| p + 3);
            out.push((i..end, Kind::Comment));
            i = end;
        } else if b[i..].starts_with(b"<![CDATA[") {
            let end = find(b, i + 9, b"]]>").map_or(b.len(), |p| p + 3);
            out.push((i..end, Kind::Str));
            i = end;
        } else if b[i] == b'<' {
            let mut j = i + 1;
            while j < b.len() && matches!(b[j], b'/' | b'?' | b'!') {
                j += 1;
            }
            out.push((i..j, Kind::Punct));
            let name_start = j;
            while j < b.len() && (is_word(b[j]) || matches!(b[j], b':' | b'-' | b'.')) {
                j += 1;
            }
            out.push((name_start..j, Kind::Tag));
            // attributes up to '>'
            while j < b.len() && b[j] != b'>' {
                match b[j] {
                    b'"' | b'\'' => {
                        let end = string_end_any_line(b, j);
                        out.push((j..end, Kind::Str));
                        j = end;
                    }
                    b'=' | b'/' | b'?' => {
                        out.push((j..j + 1, Kind::Punct));
                        j += 1;
                    }
                    c if is_word(c) => {
                        let s = j;
                        while j < b.len() && (is_word(b[j]) || matches!(b[j], b':' | b'-' | b'.')) {
                            j += 1;
                        }
                        out.push((s..j, Kind::Attr));
                    }
                    b'<' => break,
                    _ => j += 1,
                }
            }
            if j < b.len() && b[j] == b'>' {
                out.push((j..j + 1, Kind::Punct));
                j += 1;
            }
            i = j;
        } else if b[i] == b'&' {
            let mut j = i + 1;
            while j < b.len() && j - i < 12 && (b[j].is_ascii_alphanumeric() || b[j] == b'#') {
                j += 1;
            }
            if b.get(j) == Some(&b';') {
                out.push((i..j + 1, Kind::Keyword));
                i = j + 1;
            } else {
                i += 1;
            }
        } else {
            i += 1;
        }
    }
}

/// Attribute values may span lines.
fn string_end_any_line(b: &[u8], i: usize) -> usize {
    let q = b[i];
    b[i + 1..].iter().position(|&c| c == q).map_or(b.len(), |p| i + 1 + p + 1)
}

/// A scalar of YAML / INI: quoted string, number, true/false/null…, or plain text.
fn scalar(b: &[u8], off: usize, from: usize, to: usize, out: &mut Vec<(Range<usize>, Kind)>, yaml: bool) {
    let mut s = from;
    while s < to && b[s] == b' ' {
        s += 1;
    }
    let mut e = to;
    while e > s && matches!(b[e - 1], b' ' | b'\t' | b'\r') {
        e -= 1;
    }
    if s >= e {
        return;
    }
    let v = &b[s..e];
    let kind = if matches!(v[0], b'"' | b'\'') {
        Kind::Str
    } else if v[0].is_ascii_digit() || (matches!(v[0], b'-' | b'+' | b'.') && v.len() > 1 && v[1].is_ascii_digit()) {
        if v.iter().all(|c| c.is_ascii_alphanumeric() || matches!(c, b'.' | b'-' | b'+' | b'_' | b':')) { Kind::Number } else { Kind::Text }
    } else {
        let lower = v.to_ascii_lowercase();
        let words: &[&[u8]] = if yaml {
            &[b"true", b"false", b"null", b"~", b"yes", b"no", b"on", b"off"]
        } else {
            &[b"true", b"false", b"yes", b"no", b"on", b"off"]
        };
        if words.contains(&lower.as_slice()) {
            Kind::Keyword
        } else if yaml && matches!(v[0], b'&' | b'*' | b'!') {
            Kind::Keyword
        } else if yaml && matches!(v[0], b'|' | b'>') && v.len() <= 3 {
            Kind::Punct
        } else {
            Kind::Text
        }
    };
    out.push((off + s..off + e, kind));
}

/// The position of " #" (a comment) outside quotes, or None.
fn comment_start(b: &[u8], from: usize, marks: &[u8]) -> Option<usize> {
    let mut q = 0u8;
    for i in from..b.len() {
        let c = b[i];
        if q != 0 {
            if c == q {
                q = 0;
            }
        } else if c == b'"' || c == b'\'' {
            q = c;
        } else if marks.contains(&c) && (i == 0 || b[i - 1] == b' ' || b[i - 1] == b'\t') {
            return Some(i);
        }
    }
    None
}

fn yaml_line(b: &[u8], off: usize, out: &mut Vec<(Range<usize>, Kind)>) {
    let mut i = 0;
    while i < b.len() && matches!(b[i], b' ' | b'\t') {
        i += 1;
    }
    if i >= b.len() {
        return;
    }
    if b[i] == b'#' {
        out.push((off + i..off + b.len(), Kind::Comment));
        return;
    }
    if i == 0 && (b.starts_with(b"---") || b.starts_with(b"...")) {
        out.push((off..off + 3, Kind::Keyword));
        if let Some(c) = comment_start(b, 3, b"#") {
            out.push((off + c..off + b.len(), Kind::Comment));
        }
        return;
    }
    // list items "- "
    while i < b.len() && b[i] == b'-' && (i + 1 == b.len() || b[i + 1] == b' ') {
        out.push((off + i..off + i + 1, Kind::Punct));
        i += 1;
        while i < b.len() && b[i] == b' ' {
            i += 1;
        }
    }
    let end = comment_start(b, i, b"#").unwrap_or(b.len());
    // "key:" (a colon followed by a space or the end, outside quotes)
    let mut q = 0u8;
    let mut colon = None;
    for j in i..end {
        let c = b[j];
        if q != 0 {
            if c == q {
                q = 0;
            }
        } else if (c == b'"' || c == b'\'') && j == i {
            q = c;
        } else if c == b':' && (j + 1 == end || b[j + 1] == b' ' || b[j + 1] == b'\t' || b[j + 1] == b'\r') {
            colon = Some(j);
            break;
        } else if matches!(c, b'{' | b'[') && j == i {
            break;
        }
    }
    let value_from = if let Some(c) = colon {
        out.push((off + i..off + c, Kind::Key));
        out.push((off + c..off + c + 1, Kind::Punct));
        c + 1
    } else {
        i
    };
    scalar(b, off, value_from, end, out, true);
    if end < b.len() {
        out.push((off + end..off + b.len(), Kind::Comment));
    }
}

fn ini_line(b: &[u8], off: usize, out: &mut Vec<(Range<usize>, Kind)>) {
    let mut i = 0;
    while i < b.len() && matches!(b[i], b' ' | b'\t') {
        i += 1;
    }
    if i >= b.len() {
        return;
    }
    match b[i] {
        b';' | b'#' => out.push((off + i..off + b.len(), Kind::Comment)),
        b'[' => {
            let end = b.iter().rposition(|&c| c == b']').map_or(b.len(), |p| p + 1).max(i + 1);
            out.push((off + i..off + end, Kind::Section));
            if let Some(c) = comment_start(b, end, b";#") {
                out.push((off + c..off + b.len(), Kind::Comment));
            }
        }
        _ => {
            let eq = b[i..].iter().position(|&c| c == b'=').map(|p| i + p);
            let Some(eq) = eq else { return };
            let mut k = eq;
            while k > i && matches!(b[k - 1], b' ' | b'\t') {
                k -= 1;
            }
            out.push((off + i..off + k, Kind::Key));
            out.push((off + eq..off + eq + 1, Kind::Punct));
            let end = comment_start(b, eq + 1, b"#;").unwrap_or(b.len());
            scalar(b, off, eq + 1, end, out, false);
            if end < b.len() {
                out.push((off + end..off + b.len(), Kind::Comment));
            }
        }
    }
}

fn markdown_line(b: &[u8], off: usize, fence: &mut bool, out: &mut Vec<(Range<usize>, Kind)>) {
    let trimmed = b.iter().position(|&c| c != b' ').unwrap_or(b.len());
    let t = &b[trimmed..];
    if t.starts_with(b"```") || t.starts_with(b"~~~") {
        *fence = !*fence;
        out.push((off..off + b.len(), Kind::Str));
        return;
    }
    if *fence {
        out.push((off..off + b.len(), Kind::Str));
        return;
    }
    if t.starts_with(b"#") {
        out.push((off..off + b.len(), Kind::Heading));
        return;
    }
    if t.starts_with(b">") {
        out.push((off..off + b.len(), Kind::Comment));
        return;
    }
    let mut i = trimmed;
    if (t.starts_with(b"- ") || t.starts_with(b"* ") || t.starts_with(b"+ ")) && t.len() > 1 {
        out.push((off + i..off + i + 1, Kind::Punct));
        i += 2;
    } else {
        let digits = t.iter().take_while(|c| c.is_ascii_digit()).count();
        if digits > 0 && t.get(digits) == Some(&b'.') && t.get(digits + 1) == Some(&b' ') {
            out.push((off + i..off + i + digits + 1, Kind::Punct));
            i += digits + 2;
        }
    }
    // `code` and [link](target)
    while i < b.len() {
        match b[i] {
            b'`' => {
                let end = b[i + 1..].iter().position(|&c| c == b'`').map_or(b.len(), |p| i + 1 + p + 1);
                out.push((off + i..off + end, Kind::Str));
                i = end;
            }
            b']' if b.get(i + 1) == Some(&b'(') => {
                let end = b[i + 2..].iter().position(|&c| c == b')').map_or(b.len(), |p| i + 2 + p + 1);
                out.push((off + i + 1..off + end, Kind::Attr));
                i = end;
            }
            _ => i += 1,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_spans() {
        let s = r#"{"a": [1, true, "x"]}"#;
        let sp = spans(Lang::Json, s);
        assert!(sp.iter().any(|(r, k)| &s[r.clone()] == "\"a\"" && *k == Kind::Key));
        assert!(sp.iter().any(|(r, k)| &s[r.clone()] == "\"x\"" && *k == Kind::Str));
        assert!(sp.iter().any(|(r, k)| &s[r.clone()] == "true" && *k == Kind::Keyword));
    }

    #[test]
    fn yaml_spans() {
        let s = "name: Petr # comment\n- item: 5\nčeský: ano";
        let sp = spans(Lang::Yaml, s);
        assert!(sp.iter().any(|(r, k)| &s[r.clone()] == "name" && *k == Kind::Key));
        assert!(sp.iter().any(|(r, k)| &s[r.clone()] == "# comment" && *k == Kind::Comment));
        assert!(sp.iter().any(|(r, k)| &s[r.clone()] == "5" && *k == Kind::Number));
        assert!(sp.iter().any(|(r, k)| &s[r.clone()] == "český" && *k == Kind::Key));
    }
}
