//! What HeMiPad remembers between runs, in %AppData%\HeMiPad\settings.ini (lines "key=value").

use crate::i18n::Choice;
use crate::sys::FontChoice;
use std::path::PathBuf;

#[derive(Clone, PartialEq)]
pub struct Settings {
    /// The language of the app: as Windows (English, or Czech on a Czech Windows), or picked by hand.
    pub language: Choice,
    /// 0 = automatic (as HeMiDock / Windows), 1 = light, 2 = dark.
    pub theme: u8,
    pub word_wrap: bool,
    pub status_bar: bool,
    pub line_numbers: bool,
    pub spelling: bool,
    pub syntax: bool,
    pub zoom: f32,
    pub font: FontChoice,
    pub match_case: bool,
    pub wrap_around: bool,
    /// Page margins in hundredths of a millimetre: left, top, right, bottom.
    pub margins: [i32; 4],
    /// The size of the window in points, and whether it is maximised.
    pub width: f32,
    pub height: f32,
    pub maximized: bool,
    /// Where the window was (top left corner, points) and the scale of that monitor (pixels per point).
    pub position: Option<(f32, f32)>,
    pub scale: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            language: Choice::Windows,
            theme: 0,
            word_wrap: true,
            status_bar: true,
            line_numbers: false,
            spelling: true,
            syntax: true,
            zoom: 1.0,
            font: FontChoice::default(),
            match_case: false,
            wrap_around: true,
            margins: [2000, 2500, 2000, 2500],
            width: 960.0,
            height: 680.0,
            maximized: false,
            position: None,
            scale: 1.0,
        }
    }
}

fn file() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(|d| PathBuf::from(d).join("HeMiPad").join("settings.ini"))
}

impl Settings {
    pub fn load() -> Settings {
        let mut s = Settings::default();
        let Some(text) = file().and_then(|f| std::fs::read_to_string(f).ok()) else { return s };
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else { continue };
            let (k, v) = (k.trim(), v.trim());
            let b = v == "1" || v.eq_ignore_ascii_case("true");
            let f = v.parse::<f32>().ok();
            match k {
                "Language" => s.language = Choice::from_key(v),
                "Theme" => s.theme = v.parse::<u8>().unwrap_or(0).min(2),
                "WordWrap" => s.word_wrap = b,
                "StatusBar" => s.status_bar = b,
                "LineNumbers" => s.line_numbers = b,
                "Spelling" => s.spelling = b,
                "Syntax" => s.syntax = b,
                "Zoom" => s.zoom = f.unwrap_or(1.0).clamp(0.1, 5.0),
                "MatchCase" => s.match_case = b,
                "WrapAround" => s.wrap_around = b,
                "Margins" => {
                    let m: Vec<i32> = v.split(',').filter_map(|x| x.trim().parse().ok()).collect();
                    if let [l, t, r, bt] = m[..] {
                        s.margins = [l, t, r, bt];
                    }
                }
                "Width" => s.width = f.unwrap_or(s.width).clamp(320.0, 10_000.0),
                "Height" => s.height = f.unwrap_or(s.height).clamp(240.0, 10_000.0),
                "Maximized" => s.maximized = b,
                "Position" => {
                    let m: Vec<f32> = v.split(',').filter_map(|x| x.trim().parse().ok()).collect();
                    if let [x, y] = m[..] {
                        s.position = Some((x, y));
                    }
                }
                "Scale" => s.scale = f.unwrap_or(1.0).clamp(0.5, 5.0),
                _ => {}
            }
        }
        s
    }

    pub fn save(&self) {
        let Some(f) = file() else { return };
        let b = |v: bool| if v { "1" } else { "0" };
        let m = self.margins;
        let text = format!(
            "Language={}\nTheme={}\nWordWrap={}\nStatusBar={}\nLineNumbers={}\nSpelling={}\nSyntax={}\nZoom={}\n\
             MatchCase={}\nWrapAround={}\nMargins={},{},{},{}\nWidth={}\nHeight={}\nMaximized={}\n{}Scale={}\n",
            self.language.key(),
            self.theme,
            b(self.word_wrap),
            b(self.status_bar),
            b(self.line_numbers),
            b(self.spelling),
            b(self.syntax),
            self.zoom,
            b(self.match_case),
            b(self.wrap_around),
            m[0],
            m[1],
            m[2],
            m[3],
            self.width.round(),
            self.height.round(),
            b(self.maximized),
            self.position.map(|(x, y)| format!("Position={},{}\n", x.round(), y.round())).unwrap_or_default(),
            self.scale,
        );
        if let Some(dir) = f.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(f, text);
    }
}
