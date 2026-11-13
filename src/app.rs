//! The window of HeMiPad: the menu bar, the find bar, the editor, the status bar and the dialogs.
//! It does everything the classic Notepad does, plus colours for JSON / YAML / XML / INI / Markdown,
//! line numbers, JSON formatting and the spell checker of Windows.
//!
//! Light on the system: nothing is drawn while nobody does anything (the cursor does not blink,
//! the theme is checked only when something happens), the colouring of the text is cached,
//! and the spelling is checked on a thread of its own a while after typing stops.

use crate::doc::{self, Encoding, Eol};
use crate::i18n::{self, tr};
use crate::settings::Settings;
use crate::spell::{self, Reply, Spell};
use crate::syntax::{self, Colors, Lang};
use crate::sys;
use crate::theme::{self, Palette, glyph};
use crate::widgets::{Btn, CARD_RADIUS, Tone, card, info_bar, row_layout, two_lines};
use eframe::egui::{
    self, Align, Align2, Color32, CornerRadius, Event, Galley, Id, Key, KeyboardShortcut, Layout, Margin, Modifiers, Pos2,
    Rect, RichText, Shape, Stroke, TextBuffer, UiBuilder, pos2, vec2,
};
use eframe::egui::text::{CCursor, CCursorRange};
use eframe::egui::text_edit::TextEditState;
use raw_window_handle::{
    DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle, WindowHandle,
    WindowsDisplayHandle,
};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

/// Problems and ideas: the Issues of the project on GitHub – shown as text only (HeMiPad opens nothing on the internet).
const CONTACT: &str = "github.com/Hemal1k/HeMiPad/issues";
const CHECK: char = '\u{E73E}';
/// The spell checker (with the dictionaries of Windows, ~60 MB) is let go after this long without typing.
const SPELL_IDLE: Duration = Duration::from_secs(180);

fn editor_id() -> Id {
    Id::unique("hemipad-editor")
}

fn find_field_id() -> Id {
    Id::unique("hemipad-find")
}

/// A fast hash of the text (to know whether it changed and to cache its layout).
fn hash(s: &str) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325u64 ^ s.len() as u64;
    for c in s.as_bytes().chunks(8) {
        let mut w = [0u8; 8];
        w[..c.len()].copy_from_slice(c);
        h = (h ^ u64::from_le_bytes(w)).wrapping_mul(0x0100_0000_01b3);
        h ^= h >> 29;
    }
    h
}

/// The byte position of a character position.
fn byte_at(text: &str, ch: usize) -> usize {
    text.char_indices().nth(ch).map_or(text.len(), |(b, _)| b)
}

/// The selection as plain character positions, sorted.
fn chars_of(r: CCursorRange) -> Range<usize> {
    let r = r.as_sorted_char_range();
    r.start.0..r.end.0
}

fn char_range_to_bytes(text: &str, r: &Range<usize>) -> Range<usize> {
    let s = byte_at(text, r.start);
    let e = s + text[s..].char_indices().nth(r.end - r.start).map_or(text.len() - s, |(b, _)| b);
    s..e
}

#[derive(Clone)]
enum Pending {
    New,
    Open,
    OpenPath(PathBuf),
    Close,
}

enum Dialog {
    SaveChanges(Pending),
    Reload,
    GoTo(String),
    NoPrinter,
}

struct Toast {
    text: String,
    tone: Tone,
    until: Instant,
}

struct Find {
    query: String,
    replace: String,
    show_replace: bool,
    focus: bool,
}

#[derive(Clone)]
enum Act {
    New,
    NewWindow,
    Open,
    Save,
    SaveAs,
    PageSetup,
    Print,
    Exit,
    Undo,
    Redo,
    Cut,
    Copy,
    Paste,
    Delete,
    Find(bool),
    FindNext(bool),
    ReplaceOne,
    ReplaceAll,
    CloseFind,
    GoTo,
    SelectAll,
    TimeDate,
    WordWrap,
    Zoom(i32),
    StatusBar,
    LineNumbers,
    Spelling,
    Syntax,
    About(bool),
    Encoding(Encoding),
    Eol(Eol),
    DismissToast,
    Licences,
    SpellReplace(Range<usize>, String),
    SpellAdd(String),
    SpellIgnore(String),
    UiLang(i18n::Choice),
    Theme(u8),
    ShellMenu,
}

/// Line / column of the cursor and the size of the text, computed again only when they change.
#[derive(Default)]
struct Stats {
    key: (u64, usize),
    line: usize,
    col: usize,
    chars: usize,
}

pub struct HeMiPadApp {
    /// Mica of Windows 11 behind the window (transparent background).
    mica: bool,
    /// The item Open in HeMiPad is in the File Explorer menu.
    shell_menu: bool,
    p: Palette,
    source: theme::Source,
    theme_checked: Instant,
    hwnd: isize,
    s: Settings,
    saved_settings: Settings,

    path: Option<PathBuf>,
    text: String,
    enc: Encoding,
    eol: Eol,
    lang: Lang,
    indent: String,
    rev: u64,
    saved_hash: u64,
    modified: bool,
    mtime: Option<SystemTime>,
    title: String,

    galley: Option<(u64, Arc<Galley>)>,
    font_gen: u64,
    stats: Stats,
    selection: Range<usize>,
    lines: usize,

    find: Option<Find>,
    dialog: Option<Dialog>,
    toast: Option<Toast>,
    about: bool,
    highlight: Option<Range<usize>>,
    scroll_to_cursor: bool,
    inject: Vec<Event>,
    allow_close: bool,
    focused: bool,
    refocus: bool,

    spell: Option<Spell>,
    spell_failed: bool,
    spell_sent: u64,
    spell_used: Instant,
    /// Words ignored in this run (told again to a restarted spell checker).
    ignored: Vec<String>,
    spell_due: Option<Instant>,
    wrong: Vec<Range<usize>>,
    wrong_rev: u64,
    menu_word: Option<(Range<usize>, String)>,
    suggestions: Option<(String, Vec<String>)>,
}

impl HeMiPadApp {
    pub fn new(cc: &eframe::CreationContext<'_>, file: Option<PathBuf>, s: Settings) -> Self {
        let source = theme::read_source(s.theme);
        let p = theme::palette(&source);
        theme::install_fonts(&cc.egui_ctx, None);
        theme::apply(&cc.egui_ctx, &p);
        cc.egui_ctx.options_mut(|o| o.zoom_with_keyboard = false);
        let hwnd = match cc.window_handle().map(|h| h.as_raw()) {
            Ok(RawWindowHandle::Win32(w)) => w.hwnd.get(),
            _ => 0,
        };
        let mica = theme::style_window(hwnd, &p, true);
        let mut app = HeMiPadApp {
            mica,
            shell_menu: crate::shell_menu::is_on(),
            p,
            source,
            theme_checked: Instant::now(),
            hwnd,
            saved_settings: s.clone(),
            s,
            path: None,
            text: String::new(),
            enc: Encoding::Utf8,
            eol: Eol::CrLf,
            lang: Lang::Plain,
            indent: "\t".into(),
            rev: 1,
            saved_hash: hash(""),
            modified: false,
            mtime: None,
            title: String::new(),
            galley: None,
            font_gen: 0,
            stats: Stats::default(),
            selection: 0..0,
            lines: 1,
            find: None,
            dialog: None,
            toast: None,
            about: false,
            highlight: None,
            scroll_to_cursor: false,
            inject: Vec::new(),
            allow_close: false,
            focused: true,
            refocus: false,
            spell: None,
            spell_failed: false,
            spell_sent: 0,
            spell_used: Instant::now(),
            ignored: Vec::new(),
            spell_due: None,
            wrong: Vec::new(),
            wrong_rev: 0,
            menu_word: None,
            suggestions: None,
        };
        app.reset_editor(&cc.egui_ctx, 0);
        if let Some(f) = file {
            if f.exists() {
                app.load(&cc.egui_ctx, &f, None);
            } else {
                app.lang = Lang::from_path(&f);
                app.path = Some(f);
                app.toast(tr("The file does not exist yet – it will be created when you save.", "Soubor zatím neexistuje – vytvoří se při uložení."), Tone::Caution);
            }
        }
        app.request_focus(&cc.egui_ctx);
        app
    }

    // ------------------------------------------------------------------ the document

    fn name(&self) -> String {
        self.path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| tr("Untitled", "Bez názvu").to_string())
    }

    /// A new editor state: the cursor at `cursor`, an empty undo history sized to the text
    /// (each undo step keeps a copy of the text, so big files keep fewer steps).
    fn reset_editor(&mut self, ctx: &egui::Context, cursor: usize) {
        let mut st = TextEditState::default();
        let max_undos = (64_000_000 / self.text.len().max(1)).clamp(4, 100);
        st.set_undoer(egui::util::undoer::Undoer::with_settings(egui::util::undoer::Settings { max_undos, ..Default::default() }));
        st.cursor.set_char_range(Some(CCursorRange::one(CCursor::new(cursor))));
        st.store(ctx, editor_id());
        self.selection = cursor..cursor;
    }

    fn changed(&mut self) {
        self.rev += 1;
        self.modified = hash(&self.text) != self.saved_hash;
        self.highlight = None;
        self.lines = self.text.bytes().filter(|&b| b == b'\n').count() + 1;
        if self.spell_on() {
            self.spell_due = Some(Instant::now() + Duration::from_millis(700));
        }
    }

    fn after_load(&mut self) {
        self.saved_hash = hash(&self.text);
        self.changed();
        self.modified = false;
        self.indent = indent_unit(&self.text, self.lang);
        self.wrong.clear();
        self.galley = None;
        self.spell_due = Some(Instant::now());
    }

    fn load(&mut self, ctx: &egui::Context, path: &Path, forced: Option<Encoding>) {
        match doc::load(path, forced) {
            Ok(l) => {
                self.text = l.text;
                self.enc = l.encoding;
                self.eol = l.eol;
                self.lang = Lang::from_path(path);
                self.path = Some(std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf()));
                self.mtime = doc::modified_time(path);
                self.after_load();
                let mut cursor = 0;
                // .LOG: Notepad adds the time and date at the end on every opening
                if self.text.starts_with(".LOG") {
                    if !self.text.ends_with('\n') {
                        self.text.push('\n');
                    }
                    self.text.push_str(&sys::time_date());
                    self.text.push('\n');
                    cursor = self.text.chars().count();
                    self.changed();
                    self.scroll_to_cursor = true;
                }
                self.reset_editor(ctx, cursor);
            }
            Err(e) => self.toast(format!("{}: {e}", tr("The file cannot be opened", "Soubor nejde otevřít")), Tone::Critical),
        }
    }

    fn new_doc(&mut self, ctx: &egui::Context) {
        self.text.clear();
        self.path = None;
        self.enc = Encoding::Utf8;
        self.eol = Eol::CrLf;
        self.lang = Lang::Plain;
        self.mtime = None;
        self.after_load();
        self.reset_editor(ctx, 0);
    }

    /// Saves; asks for a name when there is none or `as_new`. False when not saved.
    fn save(&mut self, as_new: bool) -> bool {
        let path = match (&self.path, as_new) {
            (Some(p), false) => p.clone(),
            _ => {
                let mut d = rfd::FileDialog::new()
                    .set_parent(&Parent(self.hwnd))
                    .set_title(tr("Save as", "Uložit jako"))
                    .add_filter(tr("Text documents (*.txt)", "Textové dokumenty (*.txt)"), &["txt"])
                    .add_filter(tr("All files (*.*)", "Všechny soubory (*.*)"), &["*"]);
                d = match &self.path {
                    Some(p) => {
                        let d = d.set_file_name(p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default());
                        match p.parent() {
                            Some(dir) => d.set_directory(dir),
                            None => d,
                        }
                    }
                    None => d.set_file_name("*.txt"),
                };
                match d.save_file() {
                    Some(p) => p,
                    None => return false,
                }
            }
        };
        match doc::save(&path, &self.text, self.enc, self.eol) {
            Ok(lossy) => {
                if self.path.as_ref() != Some(&path) {
                    self.lang = Lang::from_path(&path);
                    self.indent = indent_unit(&self.text, self.lang);
                }
                self.path = Some(path.clone());
                self.saved_hash = hash(&self.text);
                self.modified = false;
                self.mtime = doc::modified_time(&path);
                if lossy {
                    let t = tr(
                        "Some characters cannot be written in {enc} – they were saved as “?”. Choose UTF-8 in the status bar to keep them.",
                        "Některé znaky nejde zapsat v {enc} – uložily se jako „?“. Aby zůstaly, zvol ve stavovém řádku UTF-8.",
                    )
                    .replace("{enc}", self.enc.label());
                    self.toast(t, Tone::Caution);
                }
                true
            }
            Err(e) => {
                self.toast(format!("{}: {e}", tr("The file cannot be saved", "Soubor nejde uložit")), Tone::Critical);
                false
            }
        }
    }

    fn guard(&mut self, ctx: &egui::Context, p: Pending) {
        if self.modified {
            self.dialog = Some(Dialog::SaveChanges(p));
        } else {
            self.run_pending(ctx, p);
        }
    }

    fn run_pending(&mut self, ctx: &egui::Context, p: Pending) {
        match p {
            Pending::New => self.new_doc(ctx),
            Pending::Open => {
                let mut d = rfd::FileDialog::new()
                    .set_parent(&Parent(self.hwnd))
                    .set_title(tr("Open", "Otevřít"))
                    .add_filter(tr("All files (*.*)", "Všechny soubory (*.*)"), &["*"])
                    .add_filter(tr("Text documents (*.txt)", "Textové dokumenty (*.txt)"), &["txt"]);
                if let Some(dir) = self.path.as_ref().and_then(|p| p.parent()) {
                    d = d.set_directory(dir);
                }
                if let Some(f) = d.pick_file() {
                    self.load(ctx, &f, None);
                }
            }
            Pending::OpenPath(f) => self.load(ctx, &f, None),
            Pending::Close => {
                self.allow_close = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
        self.request_focus(ctx);
    }

    fn toast(&mut self, text: impl Into<String>, tone: Tone) {
        self.toast = Some(Toast { text: text.into(), tone, until: Instant::now() + Duration::from_secs(6) });
    }

    // ------------------------------------------------------------------ editing

    /// The keyboard goes back to the text, now and once more in the next frame
    /// (a click on a menu item takes the focus away at the end of its frame).
    fn request_focus(&mut self, ctx: &egui::Context) {
        ctx.memory_mut(|m| m.request_focus(editor_id()));
        self.refocus = true;
        ctx.request_repaint();
    }

    fn state(&self, ctx: &egui::Context) -> TextEditState {
        TextEditState::load(ctx, editor_id()).unwrap_or_default()
    }

    /// Selects `r` (characters) and scrolls to it.
    fn select(&mut self, ctx: &egui::Context, r: Range<usize>) {
        let mut st = self.state(ctx);
        st.cursor.set_char_range(Some(CCursorRange::two(CCursor::new(r.start), CCursor::new(r.end))));
        st.store(ctx, editor_id());
        self.selection = r;
        self.scroll_to_cursor = true;
    }

    /// Replaces the characters `r` with `with` as one step that Undo takes back; the new text is selected or not.
    fn edit(&mut self, ctx: &egui::Context, r: Range<usize>, with: &str, select: bool) {
        let mut st = self.state(ctx);
        let before = st.cursor.char_range().unwrap_or_default();
        let mut undoer = st.undoer();
        undoer.add_undo(&(before, self.text.clone()));
        let b = char_range_to_bytes(&self.text, &r);
        self.text.replace_range(b, with);
        let end = r.start + with.chars().count();
        let after = if select { CCursorRange::two(CCursor::new(r.start), CCursor::new(end)) } else { CCursorRange::one(CCursor::new(end)) };
        undoer.add_undo(&(after, self.text.clone()));
        st.set_undoer(undoer);
        st.cursor.set_char_range(Some(after));
        st.store(ctx, editor_id());
        self.selection = chars_of(after);
        self.changed();
        self.scroll_to_cursor = true;
    }

    fn selected_text(&self) -> &str {
        if self.selection.is_empty() {
            return "";
        }
        let b = char_range_to_bytes(&self.text, &self.selection);
        &self.text[b]
    }

    /// Keys typed into the editor that HeMiPad handles itself: Enter keeps the indentation,
    /// Tab indents the selected lines (or with spaces when the file uses them), Shift+Tab takes it back.
    fn editor_keys(&mut self, ctx: &egui::Context) {
        if !ctx.memory(|m| m.has_focus(editor_id())) {
            return;
        }
        let sel = self.state(ctx).cursor.char_range().map(chars_of).unwrap_or(0..0);
        let b0 = byte_at(&self.text, sel.start);
        let b1 = byte_at(&self.text, sel.end);
        let multi = self.text[b0..b1].contains('\n');
        let line_start = self.text[..b0].rfind('\n').map_or(0, |p| p + 1);
        let lead: String = self.text[line_start..b0].chars().take_while(|c| *c == ' ' || *c == '\t').collect();
        let unit = self.indent.clone();
        let mut block_tab = None;
        // replaced where they are, so keys typed in the same frame keep their order
        ctx.input_mut(|i| {
            i.events.retain_mut(|e| match e {
                Event::Key { key: Key::Enter, pressed: true, modifiers, .. } if modifiers.is_none() || *modifiers == Modifiers::SHIFT => {
                    *e = Event::Paste(format!("\n{lead}"));
                    true
                }
                Event::Key { key: Key::Tab, pressed: true, modifiers, .. } if modifiers.is_none() || *modifiers == Modifiers::SHIFT => {
                    if multi || modifiers.shift {
                        block_tab = Some(modifiers.shift);
                        false
                    } else {
                        *e = Event::Paste(unit.clone());
                        true
                    }
                }
                _ => true,
            });
        });
        if let Some(shift) = block_tab {
            {
                // the whole lines of the selection
                let ls = line_start;
                let le = if b1 > b0 && self.text[..b1].ends_with('\n') { b1 - 1 } else { self.text[b1..].find('\n').map_or(self.text.len(), |p| b1 + p) };
                let block = &self.text[ls..le];
                let new: Vec<String> = block
                    .split('\n')
                    .map(|l| {
                        if shift {
                            if let Some(r) = l.strip_prefix('\t') {
                                r.to_string()
                            } else {
                                let n = l.chars().take_while(|c| *c == ' ').count().min(unit.len().max(1));
                                l[n..].to_string()
                            }
                        } else if l.is_empty() {
                            String::new()
                        } else {
                            format!("{unit}{l}")
                        }
                    })
                    .collect();
                let new = new.join("\n");
                if new != block {
                    let cs = self.text[..ls].chars().count();
                    let ce = cs + block.chars().count();
                    self.edit(ctx, cs..ce, &new, multi);
                }
            }
        }
    }

    // ------------------------------------------------------------------ find

    /// The next match of the query from the selection (characters), or None.
    fn find_match(&self, query: &str, forward: bool) -> Option<Range<usize>> {
        if query.is_empty() {
            return None;
        }
        let fold = |c: char| if self.s.match_case { c } else { c.to_lowercase().next().unwrap_or(c) };
        let q: Vec<char> = query.chars().map(fold).collect();
        let hay: Vec<char> = self.text.chars().map(fold).collect();
        if q.len() > hay.len() {
            return None;
        }
        let last = hay.len() - q.len();
        let at = |i: usize| hay[i..i + q.len()] == q[..];
        let wrap = self.s.wrap_around;
        if forward {
            let from = self.selection.end.min(last + 1);
            (from..=last).find(|&i| at(i)).or_else(|| if wrap { (0..from.min(last + 1)).find(|&i| at(i)) } else { None })
        } else {
            let to = self.selection.start.min(last + 1);
            (0..to).rev().find(|&i| at(i)).or_else(|| if wrap { (to..=last).rev().find(|&i| at(i)) } else { None })
        }
        .map(|i| i..i + q.len())
    }

    fn find_next(&mut self, ctx: &egui::Context, forward: bool) {
        let Some(query) = self.find.as_ref().map(|f| f.query.clone()).filter(|q| !q.is_empty()) else {
            self.find = Some(Find { query: self.selected_text().to_string(), replace: String::new(), show_replace: false, focus: true });
            return;
        };
        match self.find_match(&query, forward) {
            Some(r) => {
                self.select(ctx, r.clone());
                self.highlight = Some(r);
            }
            None => self.toast(tr("Cannot find “{q}”.", "„{q}“ nelze najít.").replace("{q}", &query), Tone::Caution),
        }
    }

    fn matches_selection(&self, query: &str) -> bool {
        let sel = self.selected_text();
        if self.s.match_case { sel == query } else { sel.to_lowercase() == query.to_lowercase() }
    }

    fn replace_one(&mut self, ctx: &egui::Context) {
        let Some(f) = &self.find else { return };
        let (q, r) = (f.query.clone(), f.replace.clone());
        if q.is_empty() {
            return;
        }
        if self.matches_selection(&q) {
            let sel = self.selection.clone();
            self.edit(ctx, sel, &r, false);
        }
        self.find_next(ctx, true);
    }

    fn replace_all(&mut self, ctx: &egui::Context) {
        let Some(f) = &self.find else { return };
        let (q, r) = (f.query.clone(), f.replace.clone());
        if q.is_empty() {
            return;
        }
        let fold = |c: char| if self.s.match_case { c } else { c.to_lowercase().next().unwrap_or(c) };
        let qf: Vec<char> = q.chars().map(fold).collect();
        let chars: Vec<char> = self.text.chars().collect();
        let mut out = String::with_capacity(self.text.len());
        let (mut i, mut n) = (0, 0);
        while i < chars.len() {
            if i + qf.len() <= chars.len() && chars[i..i + qf.len()].iter().map(|&c| fold(c)).eq(qf.iter().copied()) {
                out.push_str(&r);
                i += qf.len();
                n += 1;
            } else {
                out.push(chars[i]);
                i += 1;
            }
        }
        if n == 0 {
            self.toast(tr("Cannot find “{q}”.", "„{q}“ nelze najít.").replace("{q}", &q), Tone::Caution);
            return;
        }
        let len = chars.len();
        self.edit(ctx, 0..len, &out, false);
        self.select(ctx, 0..0);
        self.toast(tr("Replaced: {n}", "Nahrazeno: {n}").replace("{n}", &n.to_string()), Tone::Success);
    }

    // ------------------------------------------------------------------ spelling

    fn spell_on(&self) -> bool {
        self.s.spelling && self.lang.is_prose() && !self.spell_failed && self.text.len() <= spell::MAX_LEN
    }

    fn spell_tick(&mut self, ctx: &egui::Context) {
        if let Some(sp) = &self.spell {
            while let Ok(r) = sp.rx.try_recv() {
                match r {
                    Reply::Checked(rev, wrong) => {
                        if rev == self.rev {
                            self.wrong = wrong;
                            self.wrong_rev = rev;
                        }
                    }
                    Reply::Suggestions(w, s) => self.suggestions = Some((w, s)),
                }
            }
        }
        if !self.spell_on() {
            // not needed (off, or JSON / YAML…): the dictionaries of Windows leave the memory
            self.spell = None;
            return;
        }
        let now = Instant::now();
        let Some(due) = self.spell_due else {
            // nothing typed for a while: give the memory back, it starts again with the next change
            if self.spell.is_some() {
                let idle = now.duration_since(self.spell_used);
                if idle >= SPELL_IDLE {
                    self.spell = None;
                } else {
                    ctx.request_repaint_after(SPELL_IDLE - idle);
                }
            }
            return;
        };
        if now < due {
            ctx.request_repaint_after(due - now);
            return;
        }
        self.spell_due = None;
        if self.spell_sent == self.rev && self.wrong_rev == self.rev {
            return;
        }
        // no words, no dictionary
        if !self.text.chars().any(char::is_alphabetic) {
            self.wrong.clear();
            self.wrong_rev = self.rev;
            return;
        }
        if !self.ensure_spell(ctx) {
            return;
        }
        if let Some(sp) = &self.spell {
            sp.check(self.rev, self.text.clone());
            self.spell_sent = self.rev;
            self.spell_used = now;
        }
    }

    /// Starts the spell checker if it is not running (the words ignored before are ignored again).
    fn ensure_spell(&mut self, ctx: &egui::Context) -> bool {
        if self.spell.is_some() {
            return true;
        }
        let c = ctx.clone();
        self.spell = Spell::start(move || c.request_repaint());
        match &self.spell {
            Some(sp) => {
                for w in &self.ignored {
                    sp.ignore(w.clone());
                }
                self.spell_used = Instant::now();
                true
            }
            None => {
                self.spell_failed = true;
                self.toast(
                    tr("Windows has no spell checker for Czech or English.", "Windows nemá kontrolu pravopisu pro češtinu ani angličtinu."),
                    Tone::Caution,
                );
                false
            }
        }
    }

    // ------------------------------------------------------------------ the window

    fn follow_theme(&mut self, ctx: &egui::Context, now: bool) {
        if !now && self.theme_checked.elapsed() < Duration::from_secs(2) {
            return;
        }
        self.theme_checked = Instant::now();
        let source = theme::read_source(self.s.theme);
        if source != self.source {
            self.source = source;
            self.p = theme::palette(&source);
            theme::apply(ctx, &self.p);
            self.mica = theme::style_window(self.hwnd, &self.p, true);
            self.galley = None;
        }
    }

    /// The background of the window: transparent where Windows 11 shows Mica, else the colour of the theme.
    fn backdrop(&self) -> Color32 {
        if self.mica { Color32::TRANSPARENT } else { self.p.bg }
    }

    fn font_px(&self) -> f32 {
        (self.s.font.size * 96.0 / 72.0 * self.s.zoom).max(4.0)
    }

    fn shortcuts(&mut self, ctx: &egui::Context, acts: &mut Vec<Act>) {
        let ctrl = Modifiers::COMMAND;
        let cs = Modifiers::COMMAND | Modifiers::SHIFT;
        let list: [(Modifiers, Key, Act); 22] = [
            (cs, Key::N, Act::NewWindow),
            (cs, Key::S, Act::SaveAs),
            (Modifiers::SHIFT, Key::F3, Act::FindNext(false)),
            (ctrl, Key::N, Act::New),
            (ctrl, Key::O, Act::Open),
            (ctrl, Key::S, Act::Save),
            (ctrl, Key::P, Act::Print),
            (ctrl, Key::F, Act::Find(false)),
            (ctrl, Key::H, Act::Find(true)),
            (ctrl, Key::G, Act::GoTo),
            (Modifiers::NONE, Key::F3, Act::FindNext(true)),
            (Modifiers::NONE, Key::F5, Act::TimeDate),
            (ctrl, Key::Plus, Act::Zoom(1)),
            (ctrl, Key::Equals, Act::Zoom(1)),
            (ctrl, Key::Minus, Act::Zoom(-1)),
            (ctrl, Key::Num0, Act::Zoom(0)),
            (ctrl, Key::W, Act::Exit),
            (ctrl, Key::Num1, Act::Zoom(0)),
            (Modifiers::NONE, Key::Escape, Act::CloseFind),
            (Modifiers::ALT, Key::Z, Act::WordWrap),
            (ctrl, Key::L, Act::LineNumbers),
            (Modifiers::NONE, Key::F7, Act::Spelling),
        ];
        let modal = self.dialog.is_some();
        ctx.input_mut(|i| {
            for (m, k, a) in list {
                if modal {
                    continue;
                }
                if matches!(a, Act::CloseFind) && self.find.is_none() {
                    continue;
                }
                if i.consume_shortcut(&KeyboardShortcut::new(m, k)) {
                    acts.push(a);
                }
            }
            // Ctrl + mouse wheel zooms
            let z = i.zoom_delta();
            if z != 1.0 {
                acts.push(Act::Zoom(if z > 1.0 { 1 } else { -1 }));
            }
        });
    }

    fn menu_bar(&mut self, ui: &mut egui::Ui, acts: &mut Vec<Act>) {
        let p = self.p;
        let has_sel = !self.selection.is_empty();
        egui::MenuBar::new().ui(ui, |ui| {
            ui.spacing_mut().button_padding = vec2(10.0, 4.0);
            ui.menu_button(tr("File", "Soubor"), |ui| {
                ui.set_min_width(260.0);
                item(ui, tr("New", "Nový"), "Ctrl+N", Act::New, acts);
                item(ui, tr("New window", "Nové okno"), "Ctrl+Shift+N", Act::NewWindow, acts);
                item(ui, tr("Open…", "Otevřít…"), "Ctrl+O", Act::Open, acts);
                item(ui, tr("Save", "Uložit"), "Ctrl+S", Act::Save, acts);
                item(ui, tr("Save as…", "Uložit jako…"), "Ctrl+Shift+S", Act::SaveAs, acts);
                ui.separator();
                ui.menu_button((blank(), tr("Print", "Tisk")), |ui| {
                    ui.set_min_width(220.0);
                    item(ui, tr("Print…", "Tisk…"), "Ctrl+P", Act::Print, acts);
                    item(ui, tr("Page setup…", "Vzhled stránky…"), "", Act::PageSetup, acts);
                });
                ui.separator();
                item(ui, tr("Exit", "Konec"), "", Act::Exit, acts);
            });
            ui.menu_button(tr("Edit", "Úpravy"), |ui| {
                ui.set_min_width(260.0);
                item(ui, tr("Undo", "Zpět"), "Ctrl+Z", Act::Undo, acts);
                item(ui, tr("Redo", "Znovu"), "Ctrl+Y", Act::Redo, acts);
                ui.separator();
                item_if(ui, has_sel, tr("Cut", "Vyjmout"), "Ctrl+X", Act::Cut, acts);
                item_if(ui, has_sel, tr("Copy", "Kopírovat"), "Ctrl+C", Act::Copy, acts);
                item(ui, tr("Paste", "Vložit"), "Ctrl+V", Act::Paste, acts);
                item(ui, tr("Delete", "Odstranit"), "Del", Act::Delete, acts);
                ui.separator();
                item(ui, tr("Find…", "Najít…"), "Ctrl+F", Act::Find(false), acts);
                item(ui, tr("Find next", "Najít další"), "F3", Act::FindNext(true), acts);
                item(ui, tr("Find previous", "Najít předchozí"), "Shift+F3", Act::FindNext(false), acts);
                item(ui, tr("Replace…", "Nahradit…"), "Ctrl+H", Act::Find(true), acts);
                item(ui, tr("Go to…", "Přejít na…"), "Ctrl+G", Act::GoTo, acts);
                ui.separator();
                item(ui, tr("Select all", "Vybrat vše"), "Ctrl+A", Act::SelectAll, acts);
                item(ui, tr("Time/Date", "Čas a datum"), "F5", Act::TimeDate, acts);
            });
            // what is switched often
            ui.menu_button(tr("View", "Zobrazení"), |ui| {
                ui.set_min_width(280.0);
                ui.menu_button((blank(), tr("Zoom", "Lupa")), |ui| {
                    ui.set_min_width(240.0);
                    item(ui, tr("Zoom in", "Přiblížit"), "Ctrl+Plus", Act::Zoom(1), acts);
                    item(ui, tr("Zoom out", "Oddálit"), "Ctrl+Minus", Act::Zoom(-1), acts);
                    item(ui, tr("Restore default", "Obnovit výchozí"), "Ctrl+0", Act::Zoom(0), acts);
                });
                if check_item(ui, &p, self.s.word_wrap, tr("Word wrap", "Zalamování řádků"), "Alt+Z") {
                    acts.push(Act::WordWrap);
                }
                if check_item(ui, &p, self.s.status_bar, tr("Status bar", "Stavový řádek"), "") {
                    acts.push(Act::StatusBar);
                }
                if check_item(ui, &p, self.s.line_numbers, tr("Line numbers", "Čísla řádků"), "Ctrl+L") {
                    acts.push(Act::LineNumbers);
                }
                if check_item(ui, &p, self.s.syntax, tr("Colours (JSON, YAML, XML…)", "Barvy (JSON, YAML, XML…)"), "") {
                    acts.push(Act::Syntax);
                }
                // the languages are known only while the checker runs (it is started only when needed)
                let tip = match &self.spell {
                    Some(sp) => format!("{}: {}", tr("Dictionaries of Windows", "Slovníky Windows"), sp.langs.join(", ")),
                    None if self.spell_failed => tr("Windows has no spell checker for Czech or English.", "Windows nemá kontrolu pravopisu pro češtinu ani angličtinu.").to_string(),
                    None => tr("Czech and English, with the dictionaries of Windows.", "Čeština a angličtina, se slovníky Windows.").to_string(),
                };
                let resp = ui.add(egui::Button::new((
                    RichText::new(CHECK.to_string()).font(theme::icon(12.0)).color(if self.s.spelling { p.text } else { Color32::TRANSPARENT }),
                    tr("Spell check", "Kontrola pravopisu"),
                ))
                .shortcut_text(RichText::new("F7").size(12.0)));
                if resp.on_hover_text(tip).clicked() {
                    acts.push(Act::Spelling);
                }
            });
            // what is set once – appearance, language, File Explorer, About – under the gear on the right,
            // as the settings of Windows 11 apps
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let gear = RichText::new(glyph::SETTINGS.to_string()).font(theme::icon(16.0));
            ui.menu_button(gear, |ui| {
                ui.set_min_width(300.0);
                ui.menu_button((blank(), tr("Theme", "Motiv")), |ui| {
                    ui.set_min_width(200.0);
                    for (m, label) in [(0u8, tr("Automatic", "Automaticky")), (1, tr("Light", "Světlý")), (2, tr("Dark", "Tmavý"))] {
                        if check_item(ui, &p, self.s.theme == m, label, "") {
                            acts.push(Act::Theme(m));
                        }
                    }
                });
                ui.menu_button((blank(), tr("Language", "Jazyk")), |ui| {
                    ui.set_min_width(200.0);
                    for (c, label) in [
                        (i18n::Choice::Windows, tr("Automatic", "Automaticky")),
                        (i18n::Choice::English, "English"),
                        (i18n::Choice::Czech, "Čeština"),
                    ] {
                        if check_item(ui, &p, self.s.language == c, label, "") {
                            acts.push(Act::UiLang(c));
                        }
                    }
                });
                let explorer = tr("“Open in HeMiPad” in File Explorer", "„Otevřít v HeMiPadu“ v Průzkumníku");
                if checkbox_item(ui, &p, self.shell_menu, explorer) {
                    acts.push(Act::ShellMenu);
                }
                ui.separator();
                item(ui, tr("About HeMiPad", "O aplikaci HeMiPad"), "", Act::About(true), acts);
            })
            .response
            .on_hover_text(tr("Settings", "Nastavení"));
            });
        });
    }

    fn find_bar(&mut self, ui: &mut egui::Ui, acts: &mut Vec<Act>) {
        let p = self.p;
        let Some(f) = &mut self.find else { return };
        let field = |ui: &mut egui::Ui, text: &mut String, hint: &str, id: Id| {
            ui.add(
                egui::TextEdit::singleline(text)
                    .id(id)
                    .hint_text(hint)
                    .desired_width(260.0)
                    .margin(Margin::symmetric(8, 6)),
            )
        };
        ui.horizontal(|ui| {
            ui.set_min_height(34.0);
            let tip = if f.show_replace { tr("Hide Replace", "Skrýt nahrazení") } else { tr("Replace", "Nahradit") };
            if Btn::new("").icon(if f.show_replace { glyph::CHEVRON_DOWN } else { glyph::CHEVRON_RIGHT }).subtle().tip(tip).show(ui, &p).clicked() {
                f.show_replace = !f.show_replace;
            }
            let r = field(ui, &mut f.query, tr("Find", "Najít"), find_field_id());
            if f.focus {
                r.request_focus();
                f.focus = false;
            }
            if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                acts.push(Act::FindNext(!ui.input(|i| i.modifiers.shift)));
                r.request_focus();
            }
            if Btn::new("").icon(glyph::DOWN).subtle().tip(tr("Find next (F3)", "Najít další (F3)")).show(ui, &p).clicked() {
                acts.push(Act::FindNext(true));
            }
            if Btn::new("").icon(glyph::UP).subtle().tip(tr("Find previous (Shift+F3)", "Najít předchozí (Shift+F3)")).show(ui, &p).clicked() {
                acts.push(Act::FindNext(false));
            }
            ui.add_space(6.0);
            ui.checkbox(&mut self.s.match_case, tr("Match case", "Rozlišovat velká a malá"));
            ui.checkbox(&mut self.s.wrap_around, tr("Wrap around", "Hledat dokola"));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if Btn::new("").icon(glyph::CLOSE).subtle().tip(tr("Close (Esc)", "Zavřít (Esc)")).show(ui, &p).clicked() {
                    acts.push(Act::CloseFind);
                }
            });
        });
        if f.show_replace {
            ui.horizontal(|ui| {
                ui.set_min_height(34.0);
                ui.add_space(40.0);
                let r = field(ui, &mut f.replace, tr("Replace with", "Nahradit čím"), Id::unique("hemipad-replace"));
                if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                    acts.push(Act::ReplaceOne);
                    r.request_focus();
                }
                if Btn::new(tr("Replace", "Nahradit")).show(ui, &p).clicked() {
                    acts.push(Act::ReplaceOne);
                }
                if Btn::new(tr("Replace all", "Nahradit vše")).show(ui, &p).clicked() {
                    acts.push(Act::ReplaceAll);
                }
            });
        }
    }

    fn status_bar(&mut self, ui: &mut egui::Ui, acts: &mut Vec<Act>) {
        let p = self.p;
        // line and column of the cursor, computed again only when they change
        let cur = self.selection.end;
        if self.stats.key != (self.rev, cur) {
            let b = byte_at(&self.text, cur);
            let before = &self.text[..b];
            let line_start = before.rfind('\n').map_or(0, |i| i + 1);
            self.stats = Stats {
                key: (self.rev, cur),
                line: before.bytes().filter(|&c| c == b'\n').count() + 1,
                col: before[line_start..].chars().count() + 1,
                chars: if self.stats.key.0 == self.rev { self.stats.chars } else { self.text.chars().count() },
            };
        }
        let small = |t: String| RichText::new(t).size(12.0).color(p.text2);
        ui.horizontal(|ui| {
            ui.set_min_height(26.0);
            ui.spacing_mut().button_padding = vec2(8.0, 2.0);
            ui.spacing_mut().interact_size.y = 24.0;
            // the menu buttons of the status bar are flat, a fill only under the mouse
            let w = &mut ui.visuals_mut().widgets;
            w.inactive.weak_bg_fill = Color32::TRANSPARENT;
            w.inactive.bg_stroke = Stroke::NONE;
            ui.label(small(tr("Ln {l}, Col {c}", "Ř {l}, Sl {c}").replace("{l}", &self.stats.line.to_string()).replace("{c}", &self.stats.col.to_string())));
            sep(ui, &p);
            let n = group_digits(self.stats.chars);
            let chars = if self.selection.is_empty() {
                tr("{n} characters", "{n} znaků").replace("{n}", &n)
            } else {
                tr("{s} of {n} characters", "{s} z {n} znaků").replace("{s}", &group_digits(self.selection.len())).replace("{n}", &n)
            };
            ui.label(small(chars));
            if self.spell_on() && self.wrong_rev == self.rev && !self.wrong.is_empty() {
                sep(ui, &p);
                ui.label(RichText::new(tr("{n} misspelled", "{n} chyb").replace("{n}", &self.wrong.len().to_string())).size(12.0).color(p.critical));
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.add_space(6.0);
                ui.menu_button(small(self.enc.label().to_string()), |ui| {
                    ui.set_min_width(180.0);
                    ui.label(RichText::new(tr("Save with encoding", "Uložit v kódování")).size(12.0).color(p.text2));
                    for e in Encoding::ALL {
                        if check_item(ui, &p, self.enc == e, e.label(), "") {
                            acts.push(Act::Encoding(e));
                        }
                    }
                })
                .response
                .on_hover_text(tr("Encoding", "Kódování"));
                sep(ui, &p);
                ui.menu_button(small(self.eol.label().to_string()), |ui| {
                    ui.set_min_width(180.0);
                    for e in Eol::ALL {
                        if check_item(ui, &p, self.eol == e, e.label(), "") {
                            acts.push(Act::Eol(e));
                        }
                    }
                })
                .response
                .on_hover_text(tr("Line endings", "Konce řádků"));
                sep(ui, &p);
                ui.menu_button(small(format!("{} %", (self.s.zoom * 100.0).round())), |ui| {
                    ui.set_min_width(200.0);
                    item(ui, tr("Zoom in", "Přiblížit"), "Ctrl+Plus", Act::Zoom(1), acts);
                    item(ui, tr("Zoom out", "Oddálit"), "Ctrl+Minus", Act::Zoom(-1), acts);
                    item(ui, tr("Restore default", "Obnovit výchozí"), "Ctrl+0", Act::Zoom(0), acts);
                })
                .response
                .on_hover_text(tr("Zoom", "Lupa"));
                sep(ui, &p);
                // the type of the file comes from its name (.json, .yaml…); shown, not chosen
                ui.add_space(8.0);
                ui.label(small(self.lang.label().to_string()));
            });
        });
    }

    fn editor(&mut self, ui: &mut egui::Ui, acts: &mut Vec<Act>) {
        let p = self.p;
        let ctx = ui.ctx().clone();
        let px = self.font_px();
        let font = theme::editor(px);
        let wrap = self.s.word_wrap;
        let lang = if self.s.syntax { self.lang } else { Lang::Plain };
        let colors = Colors::new(p.light, p.text, p.text2);

        let full = ui.available_rect_before_wrap();
        let gutter_w = if self.s.line_numbers {
            let digits = self.lines.to_string().len().max(2) as f32;
            let w = ctx.fonts_mut(|f| f.glyph_width(&font, '0'));
            (digits * w + 22.0).round()
        } else {
            0.0
        };
        let gutter = Rect::from_min_max(full.min, pos2(full.min.x + gutter_w, full.max.y));
        let area = Rect::from_min_max(pos2(full.min.x + gutter_w, full.min.y), full.max);
        // the text sits on a card, as the settings of HeMiDock
        ui.painter().rect_filled(full, CARD_RADIUS, p.card);
        if gutter_w > 0.0 {
            let r = CARD_RADIUS;
            ui.painter().rect_filled(gutter, CornerRadius { nw: r, sw: r, ne: 0, se: 0 }, if p.light { p.bg } else { p.dialog_foot.gamma_multiply(0.5) });
        }
        ui.painter().rect_stroke(full, CARD_RADIUS, Stroke::new(1.0, p.card_stroke), egui::StrokeKind::Inside);

        let cache = &mut self.galley;
        let key_extra = {
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            (wrap, px.to_bits(), lang, p.light, self.font_gen).hash(&mut h);
            h.finish()
        };
        let mut layouter = |ui: &egui::Ui, buf: &dyn TextBuffer, wrap_width: f32| -> Arc<Galley> {
            let text = buf.as_str();
            let width = if wrap { wrap_width } else { f32::INFINITY };
            let key = hash(text) ^ key_extra.rotate_left(17) ^ (width.to_bits() as u64).rotate_left(41);
            if let Some((k, g)) = cache.as_ref()
                && *k == key
            {
                return g.clone();
            }
            let job = syntax::layout_job(text, lang, font.clone(), &colors, width);
            let g = ui.fonts_mut(|f| f.layout_job(job));
            *cache = Some((key, g.clone()));
            g
        };

        let mut child = ui.new_child(UiBuilder::new().max_rect(area));
        let scroll_to = std::mem::take(&mut self.scroll_to_cursor);
        let out = egui::ScrollArea::new([!wrap, true])
            .id_salt("hemipad-scroll")
            .auto_shrink([false, false])
            .show(&mut child, |ui| {
                // a bit lower than the visible area, so an empty or short text has no scroll bar
                let size = ui.available_size() - vec2(0.0, 2.0);
                let out = egui::TextEdit::multiline(&mut self.text)
                    .id(editor_id())
                    .font(font.clone())
                    .frame(egui::Frame::new().inner_margin(Margin { left: 10, right: 10, top: 8, bottom: 8 }))
                    .desired_width(if wrap { size.x } else { f32::INFINITY })
                    .min_size(size)
                    .lock_focus(true)
                    .layouter(&mut layouter)
                    .show(ui);
                if scroll_to && let Some(c) = out.state.cursor.char_range() {
                    let r = out.galley.pos_from_cursor(c.primary).translate(out.galley_pos.to_vec2());
                    ui.scroll_to_rect(r.expand2(vec2(40.0, 0.0)), Some(Align::Center));
                }
                out
            })
            .inner;

        let resp = &out.response.response;
        if resp.changed() {
            self.changed();
        }
        if let Some(c) = out.cursor_range.or_else(|| out.state.cursor.char_range()) {
            self.selection = chars_of(c);
        }
        let focused = resp.has_focus();
        if focused {
            self.highlight = None;
        }
        let galley = out.galley.clone();
        let origin = out.galley_pos;

        // line numbers, squiggles under wrong words, the found text while the find bar has the focus
        let painter = ui.painter_at(area);
        let gpainter = ui.painter_at(gutter);
        let cur_line = self.stats.line;
        let show_wrong = self.spell_on() && self.wrong_rev == self.rev && !self.wrong.is_empty();
        let mut wi = 0;
        let (mut line, mut new_line, mut ch) = (1usize, true, 0usize);
        for row in &galley.rows {
            let r = row.rect().translate(origin.to_vec2());
            let n_incl = row.char_count_including_newline().0;
            let n_excl = row.char_count_excluding_newline().0;
            if r.top() > area.bottom() {
                break;
            }
            if r.bottom() >= area.top() {
                if gutter_w > 0.0 && new_line {
                    let color = if line == cur_line { p.text2 } else { p.text3 };
                    gpainter.text(pos2(gutter.right() - 12.0, r.top()), Align2::RIGHT_TOP, line.to_string(), font.clone(), color);
                }
                if show_wrong {
                    while wi < self.wrong.len() && self.wrong[wi].end <= ch {
                        wi += 1;
                    }
                    let mut k = wi;
                    while k < self.wrong.len() && self.wrong[k].start < ch + n_excl {
                        let w = &self.wrong[k];
                        let s = w.start.max(ch) - ch;
                        let e = w.end.min(ch + n_excl) - ch;
                        let x0 = r.left() + row.x_offset(egui::epaint::text::CharIndex(s));
                        let x1 = r.left() + row.x_offset(egui::epaint::text::CharIndex(e));
                        squiggle(&painter, x0, x1, r.bottom() - 1.5, p.critical);
                        k += 1;
                    }
                }
            }
            ch += n_incl;
            if row.ends_with_newline {
                line += 1;
                new_line = true;
            } else {
                new_line = false;
            }
        }
        if !focused && let Some(h) = &self.highlight {
            let a = galley.pos_from_cursor(CCursor::new(h.start)).translate(origin.to_vec2());
            let b = galley.pos_from_cursor(CCursor::new(h.end)).translate(origin.to_vec2());
            let rect = if (a.top() - b.top()).abs() < 1.0 { Rect::from_min_max(a.min, pos2(b.min.x, a.max.y)) } else { Rect::from_min_max(a.min, pos2(area.right(), a.max.y)) };
            painter.rect_filled(rect, 2.0, p.accent.gamma_multiply(0.4));
        }

        // right click: the spelling suggestions and the edit commands
        if resp.secondary_clicked() {
            self.menu_word = None;
            self.suggestions = None;
            if let Some(pos) = resp.interact_pointer_pos() {
                let c = galley.cursor_from_pos(pos - origin).index.0;
                if show_wrong && let Some(w) = self.wrong.iter().find(|w| w.start <= c && c <= w.end).cloned() {
                    let b = char_range_to_bytes(&self.text, &w);
                    let word = self.text[b].to_string();
                    if self.ensure_spell(&ctx)
                        && let Some(sp) = &self.spell
                    {
                        sp.suggest(word.clone());
                        self.spell_used = Instant::now();
                    }
                    self.menu_word = Some((w.clone(), word));
                }
            }
        }
        let has_sel = !self.selection.is_empty();
        resp.context_menu(|ui| {
            ui.set_min_width(220.0);
            if let Some((range, word)) = &self.menu_word {
                match &self.suggestions {
                    Some((w, list)) if w == word => {
                        if list.is_empty() {
                            ui.label(RichText::new(tr("No suggestions", "Žádné návrhy")).color(p.text2));
                        }
                        for s in list {
                            if ui.add(egui::Button::new((blank(), RichText::new(s).font(theme::semibold(14.0))))).clicked() {
                                acts.push(Act::SpellReplace(range.clone(), s.clone()));
                            }
                        }
                    }
                    _ => {
                        ui.label(RichText::new("…").color(p.text2));
                    }
                }
                item(ui, tr("Add to dictionary", "Přidat do slovníku"), "", Act::SpellAdd(word.clone()), acts);
                item(ui, tr("Ignore", "Ignorovat"), "", Act::SpellIgnore(word.clone()), acts);
                ui.separator();
            }
            item(ui, tr("Undo", "Zpět"), "Ctrl+Z", Act::Undo, acts);
            ui.separator();
            item_if(ui, has_sel, tr("Cut", "Vyjmout"), "Ctrl+X", Act::Cut, acts);
            item_if(ui, has_sel, tr("Copy", "Kopírovat"), "Ctrl+C", Act::Copy, acts);
            item(ui, tr("Paste", "Vložit"), "Ctrl+V", Act::Paste, acts);
            item_if(ui, has_sel, tr("Delete", "Odstranit"), "Del", Act::Delete, acts);
            ui.separator();
            item(ui, tr("Select all", "Vybrat vše"), "Ctrl+A", Act::SelectAll, acts);
        });
        let _ = Pos2::ZERO;
    }

    /// "About" in a small window of its own (with Mica and the title bar of the theme, as the main window).
    fn about_window(&mut self, ctx: &egui::Context, acts: &mut Vec<Act>) {
        let title = tr("About HeMiPad", "O aplikaci HeMiPad");
        let icon = egui::IconData { rgba: include_bytes!("../assets/HeMiPad-64.rgba").to_vec(), width: 64, height: 64 };
        let builder = egui::ViewportBuilder::default()
            .with_title(title)
            .with_inner_size([480.0, 470.0])
            .with_resizable(false)
            .with_minimize_button(false)
            .with_maximize_button(false)
            .with_transparent(true)
            .with_icon(icon);
        let p = self.p;
        let mut close = false;
        let mut mica = self.mica;
        ctx.show_viewport_immediate(egui::ViewportId::from_hash_of("hemipad-about"), builder, |ui, _class| {
            let actx = ui.ctx().clone();
            // the new window: its title bar in the theme (found by its title, egui does not hand out its handle)
            let styled = actx.data_mut(|d| d.get_temp::<(bool, bool)>(Id::unique("about-styled")));
            if styled.map(|s| s.0) != Some(p.light) {
                let t = sys::wide(title);
                let hwnd = unsafe { windows_sys::Win32::UI::WindowsAndMessaging::FindWindowW(std::ptr::null(), t.as_ptr()) };
                if !hwnd.is_null() {
                    mica = theme::style_window(hwnd as isize, &p, true);
                    actx.data_mut(|d| d.insert_temp(Id::unique("about-styled"), (p.light, mica)));
                }
            } else if let Some((_, m)) = styled {
                mica = m;
            }
            let bg = if mica { Color32::TRANSPARENT } else { p.bg };
            egui::CentralPanel::default()
                .frame(egui::Frame::new().fill(bg).inner_margin(Margin { left: 20, right: 20, top: 16, bottom: 16 }))
                .show(ui, |ui| self.about_page(ui, acts));
            if actx.input(|i| i.viewport().close_requested() || i.key_pressed(Key::Escape)) {
                close = true;
            }
        });
        if close {
            self.about = false;
            self.request_focus(ctx);
        }
    }

    /// The content of "About", as in HeMiDock: author, version, build date, contact, licences.
    fn about_page(&self, ui: &mut egui::Ui, acts: &mut Vec<Act>) {
        let p = self.p;
        ui.horizontal(|ui| {
            let (r, _) = ui.allocate_exact_size(vec2(40.0, 40.0), egui::Sense::hover());
            let tex = ui.ctx().data_mut(|d| d.get_temp::<egui::TextureHandle>(Id::unique("about-icon")));
            let tex = tex.unwrap_or_else(|| {
                let img = egui::ColorImage::from_rgba_unmultiplied([64, 64], include_bytes!("../assets/HeMiPad-64.rgba"));
                let t = ui.ctx().load_texture("hemipad-icon", img, egui::TextureOptions::LINEAR);
                ui.ctx().data_mut(|d| d.insert_temp(Id::unique("about-icon"), t.clone()));
                t
            });
            ui.painter().image(tex.id(), r, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
            ui.add_space(6.0);
            ui.label(RichText::new("HeMiPad").font(theme::display(28.0)).color(p.text));
        });
        ui.add_space(14.0);
        ui.spacing_mut().item_spacing.y = 4.0;
        let value = |ui: &mut egui::Ui, label: &str, value: &str| {
            card(ui, &p, |ui| {
                row_layout(ui, 44.0, |ui| {
                    ui.label(RichText::new(label).size(14.0).color(p.text));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(RichText::new(value).size(14.0).color(p.text2));
                    });
                });
            });
        };
        value(ui, tr("Author", "Autor"), "Richard Hemala");
        value(ui, tr("Version", "Verze"), &format!("Beta {}", env!("CARGO_PKG_VERSION")));
        value(ui, tr("Build date", "Datum sestavení"), &i18n::build_date());
        card(ui, &p, |ui| {
            row_layout(ui, 44.0, |ui| {
                ui.label(RichText::new(tr("Contact", "Kontakt")).size(14.0).color(p.text));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(RichText::new(CONTACT).size(14.0).color(p.text2));
                });
            });
        });
        card(ui, &p, |ui| {
            row_layout(ui, 44.0, |ui| {
                two_lines(ui, &p, tr("Licences", "Licence"), tr("Licences of the components used", "Licence použitých součástí"));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if Btn::new(tr("Open", "Otevřít")).show(ui, &p).clicked() {
                        acts.push(Act::Licences);
                    }
                });
            });
        });
    }

    fn dialogs(&mut self, ctx: &egui::Context) {
        let name = self.name();
        let Some(d) = &mut self.dialog else { return };
        let p = self.p;
        let (title, text): (String, String) = match d {
            Dialog::SaveChanges(_) => (
                "HeMiPad".into(),
                tr("Do you want to save changes to {name}?", "Chcete uložit změny do souboru {name}?").replace("{name}", &name),
            ),
            Dialog::Reload => (
                tr("The file was changed", "Soubor byl změněn").into(),
                tr(
                    "Another program changed {name}. Load it again and lose your changes here?",
                    "Jiný program změnil soubor {name}. Načíst ho znovu a zahodit změny tady?",
                )
                .replace("{name}", &name),
            ),
            Dialog::GoTo(_) => (tr("Go to line", "Přejít na řádek").into(), String::new()),
            Dialog::NoPrinter => (
                tr("No printer", "Žádná tiskárna").into(),
                tr(
                    "Windows has no printer, so there is nothing to print to or set up. Add a printer in Windows Settings → Bluetooth & devices → Printers & scanners.",
                    "Ve Windows není žádná tiskárna, takže není kam tisknout ani co nastavit. Tiskárnu přidáte v Nastavení Windows → Bluetooth a zařízení → Tiskárny a skenery.",
                )
                .into(),
            ),
        };
        let buttons: Vec<&str> = match d {
            Dialog::NoPrinter => vec![tr("OK", "OK")],
            Dialog::SaveChanges(_) => vec![tr("Save", "Uložit"), tr("Don't save", "Neukládat"), tr("Cancel", "Zrušit")],
            Dialog::Reload => vec![tr("Load again", "Načíst znovu"), tr("Keep mine", "Ponechat moje")],
            Dialog::GoTo(_) => vec![tr("Go to", "Přejít"), tr("Cancel", "Zrušit")],
        };
        let mut choice: Option<usize> = None;
        let frame = egui::Frame::new()
            .fill(p.dialog)
            .stroke(Stroke::new(1.0, p.card_stroke))
            .corner_radius(8)
            .shadow(ctx.global_style().visuals.window_shadow);
        let lines = self.lines;
        let modal = egui::Modal::new(Id::unique("hemipad-dialog")).backdrop_color(p.smoke).frame(frame).show(ctx, |ui| {
            let width = 440.0;
            ui.set_width(width);
            ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
            egui::Frame::new().inner_margin(Margin { left: 24, right: 24, top: 22, bottom: 24 }).show(ui, |ui| {
                ui.set_width(width - 48.0);
                ui.label(RichText::new(&title).font(theme::semibold(20.0)).color(p.text));
                ui.add_space(12.0);
                if let Dialog::GoTo(n) = d {
                    ui.label(RichText::new(tr("Line number (1 – {n}):", "Číslo řádku (1 – {n}):").replace("{n}", &lines.to_string())).color(p.text));
                    ui.add_space(6.0);
                    let r = ui.add(egui::TextEdit::singleline(n).desired_width(width - 48.0).margin(Margin::symmetric(8, 6)));
                    r.request_focus();
                    if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                        choice = Some(0);
                    }
                } else {
                    ui.add(egui::Label::new(RichText::new(&text).size(14.0).color(p.text)).wrap());
                }
            });
            egui::Frame::new()
                .fill(p.dialog_foot)
                .corner_radius(CornerRadius { nw: 0, ne: 0, sw: 8, se: 8 })
                .inner_margin(Margin::same(24))
                .show(ui, |ui| {
                    ui.set_width(width - 48.0);
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 8.0;
                        let w = (width - 48.0 - 8.0 * (buttons.len() - 1) as f32) / buttons.len() as f32;
                        for (i, b) in buttons.iter().enumerate() {
                            let btn = Btn::new(b).width(w);
                            let btn = if i == 0 { btn.accent() } else { btn };
                            if btn.show(ui, &p).clicked() {
                                choice = Some(i);
                            }
                        }
                    });
                });
        });
        if modal.should_close() && choice.is_none() {
            choice = Some(usize::MAX);
        }
        let Some(c) = choice else { return };
        let d = self.dialog.take().unwrap();
        match d {
            Dialog::NoPrinter => self.request_focus(ctx),
            Dialog::SaveChanges(pending) => match c {
                0 => {
                    if self.save(false) {
                        self.run_pending(ctx, pending);
                    }
                }
                1 => self.run_pending(ctx, pending),
                _ => self.request_focus(ctx),
            },
            Dialog::Reload => {
                if c == 0
                    && let Some(f) = self.path.clone()
                {
                    let keep = self.selection.end;
                    self.load(ctx, &f, Some(self.enc));
                    let n = keep.min(self.text.chars().count());
                    self.select(ctx, n..n);
                } else {
                    // ask again only after the next change on disk
                    self.mtime = self.path.as_deref().and_then(doc::modified_time);
                }
                self.request_focus(ctx);
            }
            Dialog::GoTo(n) => {
                if c == 0 {
                    match n.trim().parse::<usize>() {
                        Ok(l) if l >= 1 && l <= self.lines => {
                            let b = if l == 1 { 0 } else { self.text.match_indices('\n').nth(l - 2).map_or(0, |(i, _)| i + 1) };
                            let ch = self.text[..b].chars().count();
                            self.select(ctx, ch..ch);
                        }
                        _ => {
                            self.toast(tr("The line number is beyond the number of lines.", "Číslo řádku je mimo rozsah řádků."), Tone::Caution);
                        }
                    }
                }
                self.request_focus(ctx);
            }
        }
    }

    /// Another program changed the file (checked when the window gets the focus back).
    fn check_disk(&mut self, ctx: &egui::Context) {
        let Some(path) = self.path.clone() else { return };
        let now = doc::modified_time(&path);
        if now.is_none() || now == self.mtime || self.dialog.is_some() {
            return;
        }
        if self.modified {
            self.dialog = Some(Dialog::Reload);
        } else {
            let keep = self.selection.end;
            self.load(ctx, &path, Some(self.enc));
            let n = keep.min(self.text.chars().count());
            self.select(ctx, n..n);
            self.toast(tr("Another program changed the file – loaded again.", "Jiný program soubor změnil – načten znovu."), Tone::Success);
        }
    }

    fn act(&mut self, ctx: &egui::Context, acts: Vec<Act>) {
        let key = |key: Key, modifiers: Modifiers| Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers };
        for a in acts {
            // commands for the text give the keyboard back to it
            if matches!(
                a,
                Act::Undo | Act::Redo | Act::Cut | Act::Copy | Act::Delete | Act::TimeDate | Act::WordWrap | Act::Zoom(_)
                    | Act::Encoding(_) | Act::Eol(_) | Act::LineNumbers | Act::Syntax | Act::Spelling | Act::StatusBar | Act::UiLang(_) | Act::Theme(_)
            ) && self.find.as_ref().is_none_or(|f| !f.focus)
            {
                self.refocus = true;
                ctx.request_repaint();
            }
            match a {
                Act::New => self.guard(ctx, Pending::New),
                Act::NewWindow => sys::new_window(None),
                Act::Open => self.guard(ctx, Pending::Open),
                Act::Save => {
                    self.save(false);
                }
                Act::SaveAs => {
                    self.save(true);
                }
                Act::PageSetup | Act::Print if !crate::print::has_printer() => self.dialog = Some(Dialog::NoPrinter),
                Act::PageSetup => {
                    let mut m = self.s.margins;
                    if crate::print::page_setup(self.hwnd, &mut m) {
                        self.s.margins = m;
                    }
                }
                Act::Print => {
                    let sel = self.selected_text().to_string();
                    let sel = (!sel.is_empty()).then_some(sel.as_str());
                    match crate::print::print(self.hwnd, &self.name(), &self.text, sel, &self.s.font, self.s.margins) {
                        Ok(_) => {}
                        Err(e) => self.toast(e, Tone::Critical),
                    }
                }
                Act::Exit => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
                Act::Undo => self.inject.push(key(Key::Z, Modifiers::COMMAND)),
                Act::Redo => self.inject.push(key(Key::Y, Modifiers::COMMAND)),
                Act::Cut => self.inject.push(Event::Cut),
                Act::Copy => self.inject.push(Event::Copy),
                Act::Paste => {
                    self.request_focus(ctx);
                    ctx.send_viewport_cmd(egui::ViewportCommand::RequestPaste);
                }
                Act::Delete => self.inject.push(key(Key::Delete, Modifiers::NONE)),
                Act::SelectAll => {
                    let n = self.text.chars().count();
                    self.select(ctx, 0..n);
                    self.scroll_to_cursor = false;
                    self.request_focus(ctx);
                }
                Act::Find(replace) => {
                    let sel = self.selected_text();
                    let pre = (!sel.is_empty() && !sel.contains('\n')).then(|| sel.to_string());
                    match &mut self.find {
                        Some(f) => {
                            if let Some(q) = pre {
                                f.query = q;
                            }
                            f.show_replace |= replace;
                            f.focus = true;
                        }
                        None => {
                            self.find = Some(Find { query: pre.unwrap_or_default(), replace: String::new(), show_replace: replace, focus: true })
                        }
                    }
                }
                Act::FindNext(fwd) => self.find_next(ctx, fwd),
                Act::ReplaceOne => self.replace_one(ctx),
                Act::ReplaceAll => self.replace_all(ctx),
                Act::CloseFind => {
                    self.find = None;
                    self.highlight = None;
                    self.request_focus(ctx);
                }
                Act::GoTo => self.dialog = Some(Dialog::GoTo(self.stats.line.to_string())),
                Act::TimeDate => self.inject.push(Event::Paste(sys::time_date())),
                Act::WordWrap => self.s.word_wrap = !self.s.word_wrap,
                Act::Zoom(d) => {
                    self.s.zoom = match d {
                        0 => 1.0,
                        d if d > 0 => (self.s.zoom + 0.1).min(5.0),
                        _ => (self.s.zoom - 0.1).max(0.1),
                    };
                    self.s.zoom = (self.s.zoom * 10.0).round() / 10.0;
                }
                Act::StatusBar => self.s.status_bar = !self.s.status_bar,
                Act::LineNumbers => self.s.line_numbers = !self.s.line_numbers,
                Act::Syntax => self.s.syntax = !self.s.syntax,
                Act::Spelling => {
                    self.s.spelling = !self.s.spelling;
                    self.spell_failed = false;
                    self.spell_due = Some(Instant::now());
                    if self.s.spelling && !self.lang.is_prose() {
                        self.toast(
                            tr("Spelling is checked in plain text and Markdown only.", "Pravopis se kontroluje jen v prostém textu a Markdownu."),
                            Tone::Caution,
                        );
                    }
                }
                Act::About(on) => self.about = on,
                Act::ShellMenu => match crate::shell_menu::set(!self.shell_menu) {
                    Ok(()) => {
                        self.shell_menu = !self.shell_menu;
                        let text = if self.shell_menu {
                            tr(
                                "“Open in HeMiPad” is in the menu of files (in Windows 11 under “Show more options”).",
                                "„Otevřít v HeMiPadu“ je v nabídce souborů (ve Windows 11 pod „Zobrazit další možnosti“).",
                            )
                        } else {
                            tr("The item was removed from the File Explorer menu.", "Položka byla z nabídky Průzkumníka odebrána.")
                        };
                        self.toast(text, Tone::Success);
                    }
                    Err(e) => self.toast(e, Tone::Critical),
                },
                Act::Theme(m) => {
                    self.s.theme = m;
                    self.follow_theme(ctx, true);
                }
                Act::UiLang(c) => {
                    self.s.language = c;
                    i18n::set(c);
                }
                Act::Encoding(e) => {
                    if e != self.enc {
                        self.enc = e;
                        self.saved_hash = 0;
                        self.modified = true;
                    }
                }
                Act::Eol(e) => {
                    if e != self.eol {
                        self.eol = e;
                        self.saved_hash = 0;
                        self.modified = true;
                    }
                }
                Act::DismissToast => self.toast = None,
                Act::Licences => {
                    let file = std::env::temp_dir().join("HeMiPad-ThirdPartyNotices.txt");
                    match std::fs::write(&file, include_str!("../assets/ThirdPartyNotices.txt")) {
                        Ok(()) => sys::new_window(Some(&file)),
                        Err(e) => self.toast(format!("{}: {e}", tr("The licences cannot be opened", "Licence nejde otevřít")), Tone::Critical),
                    }
                }
                Act::SpellReplace(r, w) => {
                    self.edit(ctx, r, &w, false);
                    self.request_focus(ctx);
                }
                Act::SpellAdd(w) => {
                    if self.ensure_spell(ctx)
                        && let Some(sp) = &self.spell
                    {
                        sp.add(w);
                    }
                    self.spell_sent = 0;
                    self.spell_due = Some(Instant::now() + Duration::from_millis(150));
                }
                Act::SpellIgnore(w) => {
                    if self.ensure_spell(ctx)
                        && let Some(sp) = &self.spell
                    {
                        sp.ignore(w.clone());
                    }
                    self.ignored.push(w);
                    self.spell_sent = 0;
                    self.spell_due = Some(Instant::now() + Duration::from_millis(150));
                }
            }
        }
    }

    fn update_title(&mut self, ctx: &egui::Context) {
        let t = format!("{}{} - HeMiPad", if self.modified { "*" } else { "" }, self.name());
        if t != self.title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(t.clone()));
            self.title = t;
        }
    }
}

impl eframe::App for HeMiPadApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.follow_theme(&ctx, false);

        // closing the window: ask about unsaved changes, remember the window
        if ctx.input(|i| i.viewport().close_requested()) {
            if self.modified && !self.allow_close {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                if self.dialog.is_none() {
                    self.dialog = Some(Dialog::SaveChanges(Pending::Close));
                }
            } else {
                ctx.input(|i| {
                    let v = i.viewport();
                    self.s.maximized = v.maximized.unwrap_or(false);
                    if !self.s.maximized && v.minimized != Some(true) {
                        if let Some(r) = v.inner_rect {
                            self.s.width = r.width();
                            self.s.height = r.height();
                        }
                        if let Some(r) = v.outer_rect {
                            self.s.position = Some((r.min.x, r.min.y));
                            self.s.scale = v.native_pixels_per_point.unwrap_or(1.0);
                        }
                    }
                });
                self.s.save();
            }
        }

        // files dropped from File Explorer: the first here, each other one in a new window (as Notepad)
        let dropped: Vec<PathBuf> = ctx.input(|i| {
            i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).filter(|p| !p.as_os_str().is_empty() && p.is_file()).collect()
        });
        if let Some((first, rest)) = dropped.split_first() {
            for f in rest {
                sys::new_window(Some(f));
            }
            self.guard(&ctx, Pending::OpenPath(first.clone()));
        }
        let focused = ctx.input(|i| i.viewport().focused).unwrap_or(true);
        if focused && !self.focused {
            self.check_disk(&ctx);
        }
        self.focused = focused;
        if self.toast.as_ref().is_some_and(|t| Instant::now() > t.until) {
            self.toast = None;
        }
        if let Some(t) = &self.toast {
            ctx.request_repaint_after(t.until.saturating_duration_since(Instant::now()));
        }

        let mut acts = Vec::new();
        self.shortcuts(&ctx, &mut acts);
        // pasted text may bring "\r\n": the editor keeps only "\n"
        ctx.input_mut(|i| {
            for e in i.events.iter_mut() {
                if let Event::Paste(s) = e
                    && s.contains('\r')
                {
                    *s = doc::normalize(s);
                }
            }
        });
        if std::mem::take(&mut self.refocus) && self.dialog.is_none() {
            ctx.memory_mut(|m| m.request_focus(editor_id()));
        }
        // commands from the menus for the editor (Undo, Cut, Copy, Delete, F5…)
        if !self.inject.is_empty() && self.dialog.is_none() {
            ctx.memory_mut(|m| m.request_focus(editor_id()));
            let events = std::mem::take(&mut self.inject);
            ctx.input_mut(|i| i.events.extend(events));
        }
        if self.dialog.is_none() {
            self.editor_keys(&ctx);
        }

        let p = self.p;
        let bg = self.backdrop();
        let bar = egui::Frame::new().fill(bg).inner_margin(Margin { left: 6, right: 6, top: 2, bottom: 2 });
        egui::Panel::top("menu").frame(bar).show_separator_line(false).show(ui, |ui| self.menu_bar(ui, &mut acts));
        if self.find.is_some() {
            egui::Panel::top("find")
                .frame(egui::Frame::new().fill(bg).inner_margin(Margin { left: 8, right: 8, top: 2, bottom: 6 }))
                .show_separator_line(false)
                .show(ui, |ui| self.find_bar(ui, &mut acts));
        }
        if self.s.status_bar {
            egui::Panel::bottom("status")
                .frame(egui::Frame::new().fill(bg).inner_margin(Margin { left: 12, right: 6, top: 1, bottom: 1 }))
                .show_separator_line(false)
                .show(ui, |ui| self.status_bar(ui, &mut acts));
        }
        if let Some(t) = &self.toast {
            let (text, tone) = (t.text.clone(), t.tone);
            egui::Panel::bottom("toast")
                .frame(egui::Frame::new().fill(bg).inner_margin(Margin { left: 10, right: 10, top: 6, bottom: 2 }))
                .show_separator_line(false)
                .show(ui, |ui| {
                    if info_bar(ui, &p, tone, &text, Some(())) {
                        acts.push(Act::DismissToast);
                    }
                });
        }
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(bg).inner_margin(Margin { left: 10, right: 10, top: 2, bottom: 2 }))
            .show(ui, |ui| self.editor(ui, &mut acts));
        if self.about {
            self.about_window(&ctx, &mut acts);
        }
        self.dialogs(&ctx);
        self.act(&ctx, acts);
        self.spell_tick(&ctx);
        self.update_title(&ctx);
        if self.s != self.saved_settings {
            self.saved_settings = self.s.clone();
            self.s.save();
        }
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        self.backdrop().to_normalized_gamma_f32()
    }
}

// ---------------------------------------------------------------------- parts

/// How the file indents: a tab, or the spaces it uses most (YAML and JSON: 2 spaces when nothing is indented yet).
fn indent_unit(text: &str, lang: Lang) -> String {
    let (mut tabs, mut spaces, mut min) = (0, 0, usize::MAX);
    for l in text.lines().take(5000) {
        if l.starts_with('\t') {
            tabs += 1;
        } else if l.starts_with(' ') {
            let n = l.chars().take_while(|c| *c == ' ').count();
            if l.trim().is_empty() {
                continue;
            }
            spaces += 1;
            if n >= 2 {
                min = min.min(n);
            }
        }
    }
    if spaces > tabs && min != usize::MAX {
        " ".repeat(min.clamp(2, 8))
    } else if tabs > 0 {
        "\t".into()
    } else if matches!(lang, Lang::Yaml | Lang::Json) {
        "  ".into()
    } else {
        "\t".into()
    }
}

fn group_digits(n: usize) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push('\u{A0}');
        }
        out.push(c);
    }
    out
}

fn sep(ui: &mut egui::Ui, p: &Palette) {
    let (r, _) = ui.allocate_exact_size(vec2(9.0, 14.0), egui::Sense::hover());
    ui.painter().vline(r.center().x, r.y_range(), Stroke::new(1.0, p.text3.gamma_multiply(0.5)));
}

/// A wavy red line under a misspelled word.
fn squiggle(painter: &egui::Painter, x0: f32, x1: f32, y: f32, color: Color32) {
    if x1 - x0 < 2.0 {
        return;
    }
    let mut pts = Vec::new();
    let mut x = x0;
    let mut up = true;
    while x < x1 {
        pts.push(pos2(x, if up { y - 1.2 } else { y + 1.2 }));
        x += 2.0;
        up = !up;
    }
    pts.push(pos2(x1, y));
    painter.add(Shape::line(pts, Stroke::new(1.0, color)));
}

fn item(ui: &mut egui::Ui, text: &str, shortcut: &str, act: Act, acts: &mut Vec<Act>) {
    item_if(ui, true, text, shortcut, act, acts);
}

fn item_if(ui: &mut egui::Ui, enabled: bool, text: &str, shortcut: &str, act: Act, acts: &mut Vec<Act>) {
    let b = egui::Button::new((blank(), text)).shortcut_text(RichText::new(shortcut).size(12.0));
    if ui.add_enabled(enabled, b).clicked() {
        acts.push(act);
    }
}

/// An invisible check mark: keeps the texts of all menu items in one column.
fn blank() -> RichText {
    RichText::new(CHECK.to_string()).font(theme::icon(12.0)).color(Color32::TRANSPARENT)
}

/// A menu item with a check box (empty / ticked in the accent colour), for a setting that is switched on or off.
fn checkbox_item(ui: &mut egui::Ui, p: &Palette, on: bool, text: &str) -> bool {
    // Segoe Fluent Icons: Checkbox (empty square) and CheckboxComposite (ticked square)
    let (g, color) = if on { ('\u{E73A}', p.accent_text) } else { ('\u{E739}', p.text2) };
    let mark = RichText::new(g.to_string()).font(theme::icon(14.0)).color(color);
    ui.add(egui::Button::new((mark, text))).clicked()
}

/// A menu item with a check mark when `on`.
fn check_item(ui: &mut egui::Ui, p: &Palette, on: bool, text: &str, shortcut: &str) -> bool {
    let mark = RichText::new(CHECK.to_string()).font(theme::icon(12.0)).color(if on { p.text } else { Color32::TRANSPARENT });
    ui.add(egui::Button::new((mark, text)).shortcut_text(RichText::new(shortcut).size(12.0))).clicked()
}

/// The window as the owner of the file dialogs.
struct Parent(isize);

impl HasWindowHandle for Parent {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        let hwnd = std::num::NonZeroIsize::new(self.0).ok_or(HandleError::Unavailable)?;
        let raw = RawWindowHandle::Win32(raw_window_handle::Win32WindowHandle::new(hwnd));
        Ok(unsafe { WindowHandle::borrow_raw(raw) })
    }
}

impl HasDisplayHandle for Parent {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
        Ok(unsafe { DisplayHandle::borrow_raw(RawDisplayHandle::Windows(WindowsDisplayHandle::new())) })
    }
}
