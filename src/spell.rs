//! Spelling from Windows (the Spell Checking API, the same dictionaries as Edge and Word use):
//! Czech and English together – a word is wrong only when it is wrong in every language.
//! The text is checked on a thread of its own, a while after the last change.

use std::ops::Range;
use std::sync::mpsc::{Receiver, Sender, channel};
use windows::Win32::Globalization::{ISpellChecker, ISpellCheckerFactory, SpellCheckerFactory};
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree,
};
use windows::core::{PCWSTR, PWSTR};

/// Longer texts are not checked (it would take too long).
pub const MAX_LEN: usize = 2_000_000;

enum Job {
    Check(u64, String),
    Suggest(String),
    Add(String),
    Ignore(String),
}

pub enum Reply {
    /// The revision of the text and the wrong words as character ranges.
    Checked(u64, Vec<Range<usize>>),
    Suggestions(String, Vec<String>),
}

pub struct Spell {
    tx: Sender<Job>,
    pub rx: Receiver<Reply>,
    /// The languages Windows has a spell checker for (cs-CZ, en-US…).
    pub langs: Vec<String>,
}

impl Spell {
    /// Starts the thread; None when Windows has no spell checker for Czech or English.
    pub fn start(wake: impl Fn() + Send + 'static) -> Option<Spell> {
        let (tx, jobs) = channel::<Job>();
        let (reply_tx, rx) = channel();
        let (ok_tx, ok_rx) = channel();
        std::thread::spawn(move || {
            let Some(checkers) = Checkers::new() else {
                let _ = ok_tx.send(None);
                return;
            };
            let _ = ok_tx.send(Some(checkers.1.clone()));
            while let Ok(mut job) = jobs.recv() {
                // only the newest text matters
                while let Ok(next) = jobs.try_recv() {
                    match (&job, next) {
                        (Job::Check(..), n @ Job::Check(..)) => job = n,
                        (_, n) => {
                            checkers.run(job, &reply_tx);
                            job = n;
                        }
                    }
                }
                checkers.run(job, &reply_tx);
                wake();
            }
            // HeMiPad let the checker go (Spell dropped): release the dictionaries and COM
            drop(checkers);
            unsafe { windows::Win32::System::Com::CoUninitialize() };
        });
        ok_rx.recv().ok().flatten().map(|langs| Spell { tx, rx, langs })
    }

    pub fn check(&self, rev: u64, text: String) {
        let _ = self.tx.send(Job::Check(rev, text));
    }
    pub fn suggest(&self, word: String) {
        let _ = self.tx.send(Job::Suggest(word));
    }
    /// Adds the word to the user dictionary of Windows (for good).
    pub fn add(&self, word: String) {
        let _ = self.tx.send(Job::Add(word));
    }
    /// Ignores the word until HeMiPad is closed.
    pub fn ignore(&self, word: String) {
        let _ = self.tx.send(Job::Ignore(word));
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn languages() {
        use windows::Win32::System::Com::IEnumString;
        unsafe {
            let _ = super::CoInitializeEx(None, super::COINIT_MULTITHREADED);
            let f: super::ISpellCheckerFactory = super::CoCreateInstance(&super::SpellCheckerFactory, None, super::CLSCTX_INPROC_SERVER).unwrap();
            let list: IEnumString = f.SupportedLanguages().unwrap();
            loop {
                let mut item = [super::PWSTR::null()];
                let mut got = 0u32;
                let _ = list.Next(&mut item, Some(&mut got));
                if got == 0 {
                    break;
                }
                println!("LANG {}", item[0].to_string().unwrap());
            }
        }
    }
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

struct Checkers(Vec<ISpellChecker>, Vec<String>);

impl Checkers {
    fn new() -> Option<Checkers> {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            let factory: ISpellCheckerFactory = CoCreateInstance(&SpellCheckerFactory, None, CLSCTX_INPROC_SERVER).ok()?;
            let mut list = Vec::new();
            let mut names = Vec::new();
            for lang in ["cs-CZ", "en-US", "en-GB"] {
                if list.len() == 2 && lang == "en-GB" {
                    break;
                }
                let w = wide(lang);
                if factory.IsSupported(PCWSTR(w.as_ptr())).map(|b| b.as_bool()).unwrap_or(false)
                    && let Ok(c) = factory.CreateSpellChecker(PCWSTR(w.as_ptr()))
                {
                    list.push(c);
                    names.push(lang.to_string());
                }
            }
            (!list.is_empty()).then_some(Checkers(list, names))
        }
    }

    fn run(&self, job: Job, tx: &Sender<Reply>) {
        match job {
            Job::Check(rev, text) => {
                let _ = tx.send(Reply::Checked(rev, self.check(&text)));
            }
            Job::Suggest(word) => {
                let s = self.suggest(&word);
                let _ = tx.send(Reply::Suggestions(word, s));
            }
            Job::Add(word) => unsafe {
                let w = wide(&word);
                for c in &self.0 {
                    let _ = c.Add(PCWSTR(w.as_ptr()));
                }
            },
            Job::Ignore(word) => unsafe {
                let w = wide(&word);
                for c in &self.0 {
                    let _ = c.Ignore(PCWSTR(w.as_ptr()));
                }
            },
        }
    }

    /// Wrong in this checker: UTF-16 ranges of the text.
    fn errors(c: &ISpellChecker, text: &[u16]) -> Vec<Range<usize>> {
        let mut out = Vec::new();
        unsafe {
            let Ok(list) = c.Check(PCWSTR(text.as_ptr())) else { return out };
            loop {
                let mut e: Option<windows::Win32::Globalization::ISpellingError> = None;
                if list.Next(&mut e).is_err() {
                    break;
                }
                let Some(e) = e else { break };
                let (Ok(s), Ok(l)) = (e.StartIndex(), e.Length()) else { break };
                out.push(s as usize..(s + l) as usize);
            }
        }
        out
    }

    fn word_ok(c: &ISpellChecker, word: &[u16]) -> bool {
        let mut w = word.to_vec();
        w.push(0);
        Self::errors(c, &w).is_empty()
    }

    /// The wrong words as character ranges, sorted.
    fn check(&self, text: &str) -> Vec<Range<usize>> {
        if text.len() > MAX_LEN {
            return Vec::new();
        }
        let w: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
        let first = &self.0[0];
        let mut wrong: Vec<Range<usize>> = Self::errors(first, &w)
            .into_iter()
            .filter(|r| r.end <= w.len() - 1 && self.0[1..].iter().all(|c| !Self::word_ok(c, &w[r.clone()])))
            .collect();
        wrong.sort_by_key(|r| r.start);
        // UTF-16 positions → character positions (one pass, the ranges are sorted)
        let mut out = Vec::with_capacity(wrong.len());
        let (mut u, mut ch) = (0usize, 0usize);
        let mut chars = text.chars();
        let mut to_char = |target: usize, u: &mut usize, ch: &mut usize| {
            while *u < target {
                match chars.next() {
                    Some(c) => {
                        *u += c.len_utf16();
                        *ch += 1;
                    }
                    None => break,
                }
            }
            *ch
        };
        for r in wrong {
            let s = to_char(r.start, &mut u, &mut ch);
            let e = to_char(r.end, &mut u, &mut ch);
            if e > s {
                out.push(s..e);
            }
        }
        out
    }

    fn suggest(&self, word: &str) -> Vec<String> {
        let w = wide(word);
        let mut out = Vec::new();
        for c in &self.0 {
            unsafe {
                let Ok(list) = c.Suggest(PCWSTR(w.as_ptr())) else { continue };
                loop {
                    let mut item = [PWSTR::null()];
                    let mut got = 0u32;
                    if list.Next(&mut item, Some(&mut got)).is_err() || got == 0 {
                        break;
                    }
                    if let Ok(s) = item[0].to_string()
                        && !out.contains(&s)
                    {
                        out.push(s);
                    }
                    CoTaskMemFree(Some(item[0].0 as _));
                }
            }
        }
        out.truncate(8);
        out
    }
}
