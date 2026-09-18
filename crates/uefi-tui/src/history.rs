use std::path::Path;
use std::path::PathBuf;

use uefi_common::state::state_dir;

pub const CAP: usize = 500;

pub struct History {
    entries: Vec<String>,
    recall: Option<usize>,
    saved: Option<String>,
}

impl History {
    pub fn empty() -> Self {
        Self {
            entries: vec![],
            recall: None,
            saved: None,
        }
    }

    pub fn load() -> Self {
        Self::load_from(&history_path())
    }

    pub fn load_from(path: &Path) -> Self {
        let mut h = Self::empty();
        let Ok(content) = std::fs::read_to_string(path) else {
            return h;
        };
        h.entries = content
            .lines()
            .filter(|l| !l.is_empty())
            .map(str::to_string)
            .collect();
        h.entries.dedup();
        if h.entries.len() > CAP {
            h.entries.drain(..h.entries.len() - CAP);
        }
        h
    }

    pub fn submit(&mut self, line: &str) {
        self.recall = None;
        self.saved = None;
        if line.is_empty() || self.entries.last().is_some_and(|s| s == line) {
            return;
        }
        self.entries.push(line.to_string());
        if self.entries.len() > CAP {
            self.entries.remove(0);
        }
    }

    pub fn reset(&mut self) {
        self.recall = None;
        self.saved = None;
    }

    pub fn save(&self) {
        self.save_to(&history_path());
    }

    pub fn save_to(&self, path: &Path) {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let mut out = self.entries.join("\n");
        if !out.is_empty() {
            out.push('\n');
        }
        let _ = std::fs::write(path, out);
    }

    pub fn prev(&mut self, prefix: &str) -> Option<String> {
        if self.recall.is_none() {
            self.saved = Some(prefix.to_string());
        }
        let mut i = self.recall.unwrap_or(self.entries.len());
        while i > 0 {
            i -= 1;
            if self.entries[i].starts_with(prefix) {
                self.recall = Some(i);
                return Some(self.entries[i].clone());
            }
        }
        None
    }

    pub fn next(&mut self, prefix: &str) -> Option<String> {
        let mut i = self.recall? + 1;
        while i < self.entries.len() {
            if self.entries[i].starts_with(prefix) {
                self.recall = Some(i);
                return Some(self.entries[i].clone());
            }
            i += 1;
        }
        let saved = self.saved.take();
        self.recall = None;
        saved
    }

    pub fn entries_len(&self) -> usize {
        self.entries.len()
    }

    pub fn entry(&self, i: usize) -> Option<&str> {
        self.entries.get(i).map(String::as_str)
    }
}

fn history_path() -> PathBuf {
    state_dir().join("cmdline_history")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> std::path::PathBuf {
        let d = tempfile::tempdir().unwrap().keep();
        d.join(format!("hist-{tag}.txt"))
    }

    #[test]
    fn submit_dedups_consecutive_and_empty() {
        let mut h = History::empty();
        h.submit("");
        h.submit("open a");
        h.submit("open a");
        h.submit("save b");
        assert_eq!(h.entries_len(), 2);
    }

    #[test]
    fn load_dedups_and_caps() {
        let p = tmp("cap");
        let mut lines = vec!["x1".to_string()];
        for i in 0..600 {
            lines.push(format!("cmd{i}"));
            lines.push(format!("cmd{i}"));
        }
        std::fs::write(&p, lines.join("\n")).unwrap();
        let h = History::load_from(&p);
        assert_eq!(h.entries_len(), CAP);
        assert_eq!(h.entry(0).unwrap(), "cmd100");
    }

    #[test]
    fn save_load_roundtrip() {
        let p = tmp("rt");
        let mut h = History::empty();
        h.submit("alpha");
        h.submit("beta");
        h.save_to(&p);
        let h2 = History::load_from(&p);
        assert_eq!(h2.entry(0).unwrap(), "alpha");
        assert_eq!(h2.entry(1).unwrap(), "beta");
    }

    #[test]
    fn prefix_recall_and_saved_restore() {
        let mut h = History::empty();
        h.submit("open one");
        h.submit("save two");
        h.submit("open three");
        assert_eq!(h.prev("open").unwrap(), "open three");
        assert_eq!(h.prev("open").unwrap(), "open one");
        assert_eq!(h.next("open").unwrap(), "open three");
        assert_eq!(h.prev("zzz"), None);
        assert_eq!(h.next("open").unwrap(), "open");
    }

    #[test]
    fn reset_clears_recall() {
        let mut h = History::empty();
        h.submit("aa");
        h.submit("ab");
        h.prev("a").unwrap();
        h.reset();
        assert_eq!(h.prev("a").unwrap(), "ab");
    }
}
