# TUI cmdline UX Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Прокачать `:`-cmdline TUI до полноценного line-editor: grapheme-курсор с пословными прыжками и kill-опами, история с персистом и prefix-recall, completion-меню с навигацией, контекстный hint-бар.

**Architecture:** Новый собственный `LineBuffer` (unicode-segmentation) заменяет `App.cmdline: String`; `History` персистится в state-dir; `complete()` возвращает `Completion { common, items }`, по `items` строится `MenuState`; клавишная логика вынесена в тестируемый `App::cmd_key() -> CmdFlow`, main.rs только исполняет gRPC-часть. Меню и hint рендерятся в `ui/menu.rs` / `ui/mod.rs::render_hint`.

**Tech Stack:** Rust, ratatui 0.28 (TestBackend), crossterm 0.28, unicode-segmentation (новая), uefi-common (state_dir).

**Spec:** `docs/superpowers/specs/2026-09-18-tui-cmdline-ux-design.md` — план аргументируется от спеки; исполнители читают обе.

## Global Constraints

- Спека — источник истины: `docs/superpowers/specs/2026-09-18-tui-cmdline-ux-design.md` (цели C1–C6, клавишная карта, Non-goals).
- **Module-first rule:** `pub mod X;` в `lib.rs` добавляется В ТОМ ЖЕ ШАГЕ, что и создание файла модуля, ДО запуска `cargo test`.
- **Без комментариев в коде** (кроме коротких rustdoc `///` на pub-функциях; ссылки `file:line` разрешены).
- TDD: failing test → реализация → тест зелёный → commit. Один коммит на задачу (сообщения из задач).
- После каждой задачи: `cargo test -p uefi-tui` (или `-p uefi-common` в Task 1) и `cargo clippy -p <crate> -- -D warnings`.
- Константы спеки: `MENU_ROWS = 8`, history `CAP = 500`, файл истории `state_dir()/cmdline_history`.
- Word-семантика: Unicode word bounds (`split_word_bounds`), спец-кейсов для путей нет.
- Дефекты плана — отдельный коммит `docs: fix Task N in cmdline-ux plan (...)` ДО реализации шага.
- Out of scope (спека Non-goals): Ctrl+R, live-popup, vi-режим, PageUp/Down в меню, fmt_u32_ids, ассерт пустого forms-списка.

---

## Файлы плана

| Файл | Действие | Ответственность |
|---|---|---|
| `crates/uefi-common/src/state.rs` | Modify | `state_dir()` хелпер |
| `crates/uefi-common/src/lib.rs` | — | state уже pub (не трогаем) |
| `crates/uefi-tui/Cargo.toml` | Modify | + `unicode-segmentation` |
| `crates/uefi-tui/src/line.rs` | Create | LineBuffer (C1) |
| `crates/uefi-tui/src/history.rs` | Create | History (C2) |
| `crates/uefi-tui/src/input.rs` | Modify | новые AppEvent + `map_key` |
| `crates/uefi-tui/src/app.rs` | Modify | MenuItem, MenuState, `cmdline: LineBuffer`, `history`, `cmd_key`, mode-функции |
| `crates/uefi-tui/src/commands.rs` | Modify | `Completion`, C5-мелочи |
| `crates/uefi-tui/src/ui/menu.rs` | Create | рендер меню (C3) |
| `crates/uefi-tui/src/ui/cmdline.rs` | Modify | курсор + hscroll (C4) |
| `crates/uefi-tui/src/ui/mod.rs` | Modify | hint C6 + вызов menu::render |
| `crates/uefi-tui/src/main.rs` | Modify | handle_command → cmd_key + gRPC-флоу |
| `crates/uefi-tui/src/lib.rs` | Modify | `pub mod line; pub mod history;` |
| `TODO.md` | Modify | закрытие пунктов |

---

### Task 1: uefi-common — `state_dir()`

**Files:**
- Modify: `crates/uefi-common/src/state.rs`

**Interfaces:**
- Consumes: `directories::BaseDirs` (уже в зависимостях uefi-common)
- Produces: `pub fn state_dir() -> PathBuf` — XDG state dir движка (`…/uefipatcher`), без файла на конце

- [ ] **Step 1: Failing test**

В конец `crates/uefi-common/src/state.rs` (внутрь существующего `#[cfg(test)] mod tests` или новый):

```rust
#[test]
fn state_dir_ends_with_uefipatcher() {
    let d = state_dir();
    assert!(d.ends_with("uefipatcher"));
    assert!(default_sock().starts_with(&d));
    assert!(default_sock().ends_with("uefipatcher.sock"));
}
```

- [ ] **Step 2: Run — fails**

Run: `cargo test -p uefi-common state_dir`
Expected: FAIL — `cannot find function state_dir`

- [ ] **Step 3: Реализация**

Добавить перед `default_sock()` и зарефакторить его:

```rust
pub fn state_dir() -> PathBuf {
    if let Some(b) = directories::BaseDirs::new()
        && let Some(state) = b.state_dir()
    {
        return state.join("uefipatcher");
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    PathBuf::from(home)
        .join(".local")
        .join("state")
        .join("uefipatcher")
}

pub fn default_sock() -> PathBuf {
    state_dir().join("uefipatcher.sock")
}
```

- [ ] **Step 4: Run — passes**

Run: `cargo test -p uefi-common`
Expected: PASS (все)

- [ ] **Step 5: clippy + commit**

```bash
cargo clippy -p uefi-common -- -D warnings
git add crates/uefi-common/src/state.rs
git commit -m "feat(common): state_dir() helper (XDG state dir, default_sock refactor)"
```

---

### Task 2: `line.rs` — LineBuffer (C1)

**Files:**
- Create: `crates/uefi-tui/src/line.rs`
- Modify: `crates/uefi-tui/src/lib.rs` (+ `pub mod line;`), `crates/uefi-tui/Cargo.toml` (+ dep)

**Interfaces:**
- Produces (для Tasks 8, 10, 11):
  - `pub struct LineBuffer` с методами `new() -> Self`, `from_str(&str) -> Self`, `as_str() -> &str`, `is_empty() -> bool`, `clear()`, `set_str(&str)`, `cursor_grapheme() -> usize`, `insert(char)`, `backspace()`, `delete()`, `left()`, `right()`, `word_left()`, `word_right()`, `home()`, `end()`, `kill_word()`, `kill_to_start()`, `kill_to_end()`

- [ ] **Step 1: Зависимость + модуль (module-first)**

`crates/uefi-tui/Cargo.toml`, секция `[dependencies]`:

```toml
unicode-segmentation = "1"
```

`crates/uefi-tui/src/lib.rs` — добавить:

```rust
pub mod line;
```

- [ ] **Step 2: Failing tests**

`crates/uefi-tui/src/line.rs`:

```rust
use unicode_segmentation::UnicodeSegmentation;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_in_middle() {
        let mut b = LineBuffer::from_str("ab");
        b.left();
        b.insert('X');
        assert_eq!(b.as_str(), "aXb");
        assert_eq!(b.cursor_grapheme(), 2);
    }

    #[test]
    fn backspace_deletes_whole_grapheme() {
        let mut b = LineBuffer::from_str("👨‍👩‍👧a");
        b.backspace();
        assert_eq!(b.as_str(), "👨‍👩‍👧");
        b.backspace();
        assert_eq!(b.as_str(), "");
    }

    #[test]
    fn delete_under_cursor() {
        let mut b = LineBuffer::from_str("ab");
        b.home();
        b.delete();
        assert_eq!(b.as_str(), "b");
        assert_eq!(b.cursor_grapheme(), 0);
    }

    #[test]
    fn word_jumps_on_path() {
        let mut b = LineBuffer::from_str("foo /tmp/x");
        b.home();
        b.word_right();
        assert_eq!(b.cursor_grapheme(), 3);
        b.word_right();
        assert_eq!(b.cursor_grapheme(), 8);
        b.word_right();
        assert_eq!(b.cursor_grapheme(), 10);
        b.word_left();
        assert_eq!(b.cursor_grapheme(), 9);
        b.word_left();
        assert_eq!(b.cursor_grapheme(), 5);
    }

    #[test]
    fn home_end() {
        let mut b = LineBuffer::from_str("abc");
        b.home();
        assert_eq!(b.cursor_grapheme(), 0);
        b.end();
        assert_eq!(b.cursor_grapheme(), 3);
    }

    #[test]
    fn kill_word_back_to_previous_word_start() {
        let mut b = LineBuffer::from_str("foo bar baz");
        b.kill_word();
        assert_eq!(b.as_str(), "foo bar ");
        b.kill_word();
        assert_eq!(b.as_str(), "foo ");
    }

    #[test]
    fn kill_to_start_and_end() {
        let mut b = LineBuffer::from_str("hello");
        b.left();
        b.left();
        b.kill_to_start();
        assert_eq!(b.as_str(), "lo");
        b.end();
        b.kill_to_end();
        assert_eq!(b.as_str(), "lo");
    }

    #[test]
    fn set_str_puts_cursor_to_end() {
        let mut b = LineBuffer::new();
        b.set_str("héllo");
        assert_eq!(b.cursor_grapheme(), 5);
        b.set_str("");
        assert!(b.is_empty());
    }
}
```

- [ ] **Step 3: Run — fails**

Run: `cargo test -p uefi-tui line::`
Expected: FAIL — `LineBuffer` not found

- [ ] **Step 4: Реализация**

Выше `mod tests` в `line.rs`:

```rust
pub struct LineBuffer {
    s: String,
    cursor: usize,
}

impl LineBuffer {
    pub fn new() -> Self {
        Self { s: String::new(), cursor: 0 }
    }

    pub fn from_str(s: &str) -> Self {
        let mut b = Self::new();
        b.set_str(s);
        b
    }

    pub fn as_str(&self) -> &str {
        &self.s
    }

    pub fn is_empty(&self) -> bool {
        self.s.is_empty()
    }

    pub fn clear(&mut self) {
        self.s.clear();
        self.cursor = 0;
    }

    pub fn set_str(&mut self, s: &str) {
        self.s = s.to_string();
        self.cursor = self.s.len();
    }

    pub fn cursor_grapheme(&self) -> usize {
        self.s[..self.cursor].graphemes(true).count()
    }

    pub fn insert(&mut self, c: char) {
        self.s.insert(self.cursor, c);
        self.cursor += c.len_utf8();
    }

    pub fn left(&mut self) {
        if let Some((i, _)) = self.s[..self.cursor].grapheme_indices(true).next_back() {
            self.cursor = i;
        }
    }

    pub fn right(&mut self) {
        if let Some((i, g)) = self.s[self.cursor..].grapheme_indices(true).next() {
            self.cursor += i + g.len();
        }
    }

    pub fn home(&mut self) {
        self.cursor = 0;
    }

    pub fn end(&mut self) {
        self.cursor = self.s.len();
    }

    pub fn backspace(&mut self) {
        if let Some((i, _)) = self.s[..self.cursor].grapheme_indices(true).next_back() {
            self.s.replace_range(i..self.cursor, "");
            self.cursor = i;
        }
    }

    pub fn delete(&mut self) {
        if let Some((i, g)) = self.s[self.cursor..].grapheme_indices(true).next() {
            let end = self.cursor + i + g.len();
            self.s.replace_range(self.cursor..end, "");
        }
    }

    pub fn word_left(&mut self) {
        if let Some((i, _)) = self
            .s
            .split_word_bound_indices()
            .filter(|(i, _)| *i < self.cursor)
            .rev()
            .find(|(_, w)| w.chars().next().is_some_and(char::is_alphanumeric))
        {
            self.cursor = i;
        }
    }

    pub fn word_right(&mut self) {
        for (i, w) in self
            .s
            .split_word_bound_indices()
            .skip_while(|(i, _)| *i <= self.cursor)
        {
            if w.chars().next().is_some_and(char::is_alphanumeric) {
                self.cursor = i + w.len();
                return;
            }
        }
        self.cursor = self.s.len();
    }

    pub fn kill_word(&mut self) {
        let start = self
            .s
            .split_word_bound_indices()
            .filter(|(i, _)| *i < self.cursor)
            .rev()
            .find(|(_, w)| w.chars().next().is_some_and(char::is_alphanumeric))
            .map(|(i, _)| i)
            .unwrap_or(0);
        self.s.drain(start..self.cursor);
        self.cursor = start;
    }

    pub fn kill_to_start(&mut self) {
        self.s.drain(..self.cursor);
        self.cursor = 0;
    }

    pub fn kill_to_end(&mut self) {
        self.s.truncate(self.cursor);
    }
}

impl Default for LineBuffer {
    fn default() -> Self {
        Self::new()
    }
}
```

- [ ] **Step 5: Run — passes; commit**

Run: `cargo test -p uefi-tui line::`
Expected: PASS (8 тестов)

```bash
cargo clippy -p uefi-tui -- -D warnings
git add crates/uefi-tui/src/line.rs crates/uefi-tui/src/lib.rs crates/uefi-tui/Cargo.toml
git commit -m "feat(tui): LineBuffer — grapheme-корректный буфер с курсором, слова, kill-опы (C1)"
```

---

### Task 3: `history.rs` — History (C2)

**Files:**
- Create: `crates/uefi-tui/src/history.rs`
- Modify: `crates/uefi-tui/src/lib.rs` (+ `pub mod history;`)

**Interfaces:**
- Consumes: `uefi_common::state::state_dir()` (Task 1)
- Produces (для Task 8/11):
  - `pub const CAP: usize = 500`
  - `pub struct History` с `load() -> Self`, `empty() -> Self`, `load_from(&Path) -> Self`, `save(&self)`, `save_to(&self, &Path)`, `submit(&mut self, &str)`, `reset(&mut self)`, `prev(&mut self, prefix: &str) -> Option<String>`, `next(&mut self, prefix: &str) -> Option<String>`

- [ ] **Step 1: Модуль (module-first)**

`crates/uefi-tui/src/lib.rs`: `pub mod history;`

- [ ] **Step 2: Failing tests**

`crates/uefi-tui/src/history.rs`:

```rust
use std::path::Path;

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
        assert_eq!(h.entry(0).unwrap(), "cmd99");
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
```

Последний ассерт пары: `next`-за-концом возвращает `saved` (строку, набранную до первого `prev`) один раз; после этого `recall = None` и повторный `next` без `prev` — `None`.

- [ ] **Step 3: Run — fails**

Run: `cargo test -p uefi-tui history::`
Expected: FAIL — `History` not found

- [ ] **Step 4: Реализация**

Выше `mod tests`:

```rust
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
        Self { entries: vec![], recall: None, saved: None }
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
```

Тесты используют `h.entries_len()`/`h.entry()` — test-доступ без pub-поля.

- [ ] **Step 5: Run — passes; commit**

Run: `cargo test -p uefi-tui history::`
Expected: PASS (5 тестов)

```bash
cargo clippy -p uefi-tui -- -D warnings
git add crates/uefi-tui/src/history.rs crates/uefi-tui/src/lib.rs
git commit -m "feat(tui): History — персист 500 + дедуп + prefix-recall со saved-восстановлением (C2)"
```

---

### Task 4: `input.rs` — новые события + `map_key`

**Files:**
- Modify: `crates/uefi-tui/src/input.rs`

**Interfaces:**
- Produces: `AppEvent` += `Left, Right, Home, End, Delete, WordLeft, WordRight`; `pub fn map_key(k: crossterm::event::KeyEvent) -> Option<AppEvent>`

- [ ] **Step 1: Failing tests**

В `input.rs` заменить `mod tests` на:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn key(code: KeyCode, m: KeyModifiers) -> Option<AppEvent> {
        map_key(KeyEvent::new(code, m))
    }

    #[test]
    fn tick_on_timeout() {
        let e = poll_event(Duration::from_millis(0));
        assert_eq!(e, Some(AppEvent::Tick));
    }

    #[test]
    fn arrows_and_words() {
        assert_eq!(key(KeyCode::Left, KeyModifiers::NONE), Some(AppEvent::Left));
        assert_eq!(key(KeyCode::Right, KeyModifiers::NONE), Some(AppEvent::Right));
        assert_eq!(key(KeyCode::Left, KeyModifiers::CONTROL), Some(AppEvent::WordLeft));
        assert_eq!(key(KeyCode::Right, KeyModifiers::CONTROL), Some(AppEvent::WordRight));
    }

    #[test]
    fn home_end_delete() {
        assert_eq!(key(KeyCode::Home, KeyModifiers::NONE), Some(AppEvent::Home));
        assert_eq!(key(KeyCode::End, KeyModifiers::NONE), Some(AppEvent::End));
        assert_eq!(key(KeyCode::Delete, KeyModifiers::NONE), Some(AppEvent::Delete));
    }

    #[test]
    fn ctrl_char_still_mapped() {
        assert_eq!(key(KeyCode::Char('u'), KeyModifiers::CONTROL), Some(AppEvent::Ctrl('u')));
        assert_eq!(key(KeyCode::Char('x'), KeyModifiers::NONE), Some(AppEvent::Key('x')));
    }
}
```

- [ ] **Step 2: Run — fails**

Run: `cargo test -p uefi-tui input::`
Expected: FAIL — `map_key` not found

- [ ] **Step 3: Реализация**

Заменить тело `poll_event` (ветку `match k.code`) на вызов нового `map_key`; enum и функция:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppEvent {
    Key(char),
    Ctrl(char),
    Enter,
    Esc,
    Backspace,
    Tab,
    BackTab,
    Up,
    Down,
    PageUp,
    PageDown,
    Left,
    Right,
    Home,
    End,
    Delete,
    WordLeft,
    WordRight,
    Quit,
    Tick,
}

pub fn map_key(k: crossterm::event::KeyEvent) -> Option<AppEvent> {
    use crossterm::event::{KeyCode, KeyEventKind, KeyModifiers};
    if k.kind != KeyEventKind::Press {
        return None;
    }
    let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
    Some(match k.code {
        KeyCode::Char(c) if ctrl => AppEvent::Ctrl(c),
        KeyCode::Char(c) => AppEvent::Key(c),
        KeyCode::Enter => AppEvent::Enter,
        KeyCode::Esc => AppEvent::Esc,
        KeyCode::Backspace => AppEvent::Backspace,
        KeyCode::Tab => AppEvent::Tab,
        KeyCode::BackTab => AppEvent::BackTab,
        KeyCode::Up => AppEvent::Up,
        KeyCode::Down => AppEvent::Down,
        KeyCode::PageUp => AppEvent::PageUp,
        KeyCode::PageDown => AppEvent::PageDown,
        KeyCode::Left if ctrl => AppEvent::WordLeft,
        KeyCode::Right if ctrl => AppEvent::WordRight,
        KeyCode::Left => AppEvent::Left,
        KeyCode::Right => AppEvent::Right,
        KeyCode::Home => AppEvent::Home,
        KeyCode::End => AppEvent::End,
        KeyCode::Delete => AppEvent::Delete,
        _ => return None,
    })
}

pub fn poll_event(timeout: Duration) -> Option<AppEvent> {
    if !event::poll(timeout).unwrap_or_default() {
        return Some(AppEvent::Tick);
    }
    if let Event::Key(k) = event::read().ok()? {
        return map_key(k);
    }
    None
}
```

(импорт `KeyModifiers` из верхней строки `use crossterm::event::{…}` убрать — теперь локальный в `map_key`).

- [ ] **Step 4: Run — passes; commit**

Run: `cargo test -p uefi-tui input::`
Expected: PASS

```bash
cargo clippy -p uefi-tui -- -D warnings
git add crates/uefi-tui/src/input.rs
git commit -m "feat(tui): AppEvent += Left/Right/Home/End/Delete/WordLeft/WordRight + тестируемый map_key"
```

---

### Task 5: `Completion`-структура в `complete()`

**Files:**
- Modify: `crates/uefi-tui/src/commands.rs` (`complete`, ~`:894-927`), `crates/uefi-tui/src/app.rs` (MenuItem), `crates/uefi-tui/src/main.rs:159-168` (вызов)

**Interfaces:**
- Consumes: —
- Produces (для Tasks 7, 11):
  - `app::MenuItem { pub display: String, pub apply: String }`
  - `commands::Completion { pub common: Option<String>, pub items: Vec<crate::app::MenuItem> }`
  - `pub fn complete(app: &App, cmdline: &str) -> Completion` (семантика `common` = прежний `rep`)

- [ ] **Step 1: MenuItem в app.rs**

В `app.rs` рядом с `RegistryRow`:

```rust
#[derive(Debug, Clone)]
pub struct MenuItem {
    pub display: String,
    pub apply: String,
}
```

- [ ] **Step 2: Реализация complete()**

В `commands.rs` заменить функцию `complete` (тело до `common_prefix`) на:

```rust
pub struct Completion {
    pub common: Option<String>,
    pub items: Vec<crate::app::MenuItem>,
}

pub fn complete(app: &App, cmdline: &str) -> Completion {
    let ends_space = cmdline.ends_with(' ');
    let mut parts: Vec<&str> = cmdline.split_whitespace().collect();
    let token = if ends_space || parts.is_empty() {
        String::new()
    } else {
        parts.pop().unwrap().to_string()
    };
    let head: Vec<&str> = parts.clone();
    let candidates: Vec<String> = if head.is_empty() {
        COMMANDS
            .iter()
            .filter(|c| c.starts_with(token.as_str()))
            .map(|s| s.to_string())
            .collect()
    } else {
        context_candidates(app, head[0], &head, &token)
    };
    if candidates.is_empty() {
        return Completion { common: None, items: vec![] };
    }
    let mut base = head.join(" ");
    if !base.is_empty() {
        base.push(' ');
    }
    let items: Vec<crate::app::MenuItem> = candidates
        .iter()
        .map(|c| {
            let mut apply = base.clone();
            apply.push_str(c);
            crate::app::MenuItem { display: c.clone(), apply }
        })
        .collect();
    if candidates.len() == 1 {
        return Completion { common: Some(items[0].apply.clone()), items: vec![] };
    }
    let prefix = common_prefix(&candidates);
    base.push_str(&prefix);
    Completion { common: Some(base), items }
}
```

- [ ] **Step 3: main.rs — миграция вызова**

`main.rs` `handle_command`, ветка `AppEvent::Tab` (сейчас `:159-168`) — временная механическая миграция (Task 11 перепишет целиком):

```rust
AppEvent::Tab => {
    let comp = commands::complete(app, &app.cmdline);
    if let Some(r) = comp.common {
        app.cmdline = r;
    }
    if !comp.items.is_empty() {
        app.status_msg = format!("{} candidates", comp.items.len());
    }
}
```

(`&app.cmdline` — `&String` → `&str` deref-coercion; после Task 8 паттерн меняется на `app.cmdline.as_str()` / `set_str(&r)` — см. таблицу в Task 8.)

- [ ] **Step 4: Миграция тестов commands.rs**

Тесты, деструктурирующие кортеж (grep: `let (rep, opts) = complete(`): `complete_first_token_to_unique_command`, `complete_artifact_ids_after_flag`, `complete_flags_of_insert`, `complete_hii_verbs_and_item_ids`, `complete_hii_nouns_add_targets_items_and_paths`, `complete_path_lists_dir_entries_with_slash_for_dirs`, `complete_hii_add_file_position_is_path_completion`, `complete_ffs_flag_and_formset_guid_value`. Паттерн миграции каждого:

```rust
// было:
// let (rep, opts) = complete(&app, "inse");
// assert_eq!(rep.as_deref(), Some("insert"));
// assert_eq!(opts, vec!["insert".to_string()]);
// стало:
let c = complete(&app, "inse");
assert_eq!(c.common.as_deref(), Some("insert"));
assert!(c.items.iter().all(|i| i.apply.ends_with(&i.display)));
```

Ассерты на содержимое `opts` заменяются на `c.items.iter().map(|i| i.display.clone()).collect::<Vec<_>>()` при точном сравнении списков.

- [ ] **Step 5: Run — passes; commit**

Run: `cargo test -p uefi-tui`
Expected: PASS (все, включая мигрированные)

```bash
cargo clippy -p uefi-tui -- -D warnings
git add crates/uefi-tui/src/commands.rs crates/uefi-tui/src/app.rs crates/uefi-tui/src/main.rs
git commit -m "refactor(tui): complete() -> Completion { common, items } с apply-строками для меню"
```

---

### Task 6: C5 — мелочи complete()

**Files:**
- Modify: `crates/uefi-tui/src/commands.rs` (`fmt_item` + 4 вызова, `--ffs` guard `:1012`, `complete_path` `:960-983`)

**Interfaces:**
- Produces: `fn fmt_item(target: impl Display, form_id_ifr: u32) -> String` (private)

- [ ] **Step 1: Failing tests** (добавить в `mod tests` commands.rs)

```rust
#[test]
fn ffs_flag_only_in_grammar_position() {
    let app = crate::app::App::new();
    let c = complete(&app, "hii formset add --f");
    assert!(c.items.iter().any(|i| i.display == "--ffs"));
    let c = complete(&app, "hii form add x --f");
    assert!(!c.items.iter().any(|i| i.display == "--ffs"));
}

#[cfg(unix)]
#[test]
fn complete_path_descends_into_dir_symlinks() {
    let td = tempfile::tempdir().unwrap();
    std::fs::create_dir(td.path().join("real")).unwrap();
    std::os::unix::fs::symlink(td.path().join("real"), td.path().join("link")).unwrap();
    let token = td.path().join("li").to_string_lossy().to_string();
    let c = complete(&crate::app::App::new(), &token);
    assert!(c.items.iter().any(|i| i.display.ends_with("link/")));
}
```

- [ ] **Step 2: Run — fails**

Run: `cargo test -p uefi-tui ffs_flag complete_path_descends`
Expected: FAIL

- [ ] **Step 3: Реализация**

3a. `--ffs` guard (`:1012`):

```rust
// было: "hii" if head.contains(&"formset") && head.contains(&"add") => &["--ffs"],
// стало:
"hii" if head.len() == 3 && head[1] == "formset" && head[2] == "add" => &["--ffs"],
```

3b. Симлинки в `complete_path` — заменить маппинг кортежа:

```rust
// было: e.file_type().map(|t| t.is_dir()).unwrap_or(false),
// стало:
std::fs::metadata(e.path()).map(|m| m.is_dir()).unwrap_or(false),
```

3c. Хелпер + дедуп (вызовы на `:1060`, `:1090`, `:1193`, `:1480`):

```rust
fn fmt_item(target: impl std::fmt::Display, form_id_ifr: u32) -> String {
    format!("{}#{}", target, form_id_ifr)
}
```

```rust
// было: .map(|f| format!("{}#{}", f.form_id, f.form_id_ifr))
// стало:
.map(|f| fmt_item(&f.form_id, f.form_id_ifr))

// было: Some(format!("{}#{}", key.target, key.form_id_ifr))
// стало:
Some(fmt_item(&key.target, key.form_id_ifr))

// было: let item_id = format!("{}#{}", key.target, key.form_id_ifr);
// стало:
let item_id = fmt_item(&key.target, key.form_id_ifr);
```

(если `key.target` — уже `&str`, передавать без `&`).

- [ ] **Step 4: Run — passes; commit**

Run: `cargo test -p uefi-tui`
Expected: PASS

```bash
cargo clippy -p uefi-tui -- -D warnings
git add crates/uefi-tui/src/commands.rs
git commit -m "fix(tui): --ffs completion по грамматической позиции, симлинки-директории в complete_path, fmt_item дедуп (C5)"
```

---

### Task 7: `MenuState` в app.rs

**Files:**
- Modify: `crates/uefi-tui/src/app.rs`

**Interfaces:**
- Consumes: `MenuItem` (Task 5)
- Produces (для Tasks 9, 11):
  - `pub const MENU_ROWS: usize = 8`
  - `pub struct MenuState { pub open: bool, pub items: Vec<MenuItem>, pub selected: usize, pub offset: usize }`
  - методы: `close()`, `open_with(Vec<MenuItem>)`, `refresh(Vec<MenuItem>)`, `up()`, `down()`, `selected_apply() -> Option<&str>`

- [ ] **Step 1: Failing tests** (в `app.rs` `mod tests`)

```rust
#[test]
fn menu_navigation_wraps_and_scrolls() {
    let mut m = MenuState::default();
    let items: Vec<MenuItem> = (0..10)
        .map(|i| MenuItem { display: format!("i{i}"), apply: format!("a{i}") })
        .collect();
    m.open_with(items);
    assert_eq!(m.selected, 0);
    m.up();
    assert_eq!(m.selected, 9);
    assert_eq!(m.offset, 2);
    m.down();
    assert_eq!(m.selected, 0);
    assert_eq!(m.offset, 0);
    for _ in 0..5 {
        m.down();
    }
    assert_eq!(m.selected, 5);
    assert_eq!(m.offset, 0);
    m.down();
    m.down();
    m.down();
    assert_eq!(m.selected, 8);
    assert_eq!(m.offset, 1);
}

#[test]
fn menu_refresh_resets_and_closes_on_empty() {
    let mut m = MenuState::default();
    m.open_with(vec![MenuItem { display: "a".into(), apply: "x a".into() }]);
    m.down();
    m.refresh(vec![]);
    assert!(!m.open);
    let items = vec![
        MenuItem { display: "b".into(), apply: "x b".into() },
        MenuItem { display: "c".into(), apply: "x c".into() },
    ];
    m.refresh(items);
    assert!(m.open);
    assert_eq!(m.selected, 0);
    assert_eq!(m.selected_apply(), Some("x b"));
}
```

- [ ] **Step 2: Run — fails**

Run: `cargo test -p uefi-tui menu_`
Expected: FAIL

- [ ] **Step 3: Реализация** (рядом с `MenuItem`)

```rust
pub const MENU_ROWS: usize = 8;

#[derive(Debug, Clone, Default)]
pub struct MenuState {
    pub open: bool,
    pub items: Vec<MenuItem>,
    pub selected: usize,
    pub offset: usize,
}

impl MenuState {
    pub fn close(&mut self) {
        self.open = false;
        self.items.clear();
        self.selected = 0;
        self.offset = 0;
    }

    pub fn open_with(&mut self, items: Vec<MenuItem>) {
        self.items = items;
        self.open = true;
        self.selected = 0;
        self.offset = 0;
    }

    pub fn refresh(&mut self, items: Vec<MenuItem>) {
        if items.is_empty() {
            self.close();
        } else {
            self.items = items;
            self.selected = 0;
            self.offset = 0;
        }
    }

    pub fn down(&mut self) {
        if self.items.is_empty() {
            return;
        }
        self.selected = (self.selected + 1) % self.items.len();
        self.scroll();
    }

    pub fn up(&mut self) {
        if self.items.is_empty() {
            return;
        }
        self.selected = if self.selected == 0 { self.items.len() - 1 } else { self.selected - 1 };
        self.scroll();
    }

    fn scroll(&mut self) {
        if self.selected < self.offset {
            self.offset = self.selected;
        } else if self.selected >= self.offset + MENU_ROWS {
            self.offset = self.selected + 1 - MENU_ROWS;
        }
    }

    pub fn selected_apply(&self) -> Option<&str> {
        self.items.get(self.selected).map(|i| i.apply.as_str())
    }
}
```

- [ ] **Step 4: Run — passes; commit**

Run: `cargo test -p uefi-tui`
Expected: PASS

```bash
cargo clippy -p uefi-tui -- -D warnings
git add crates/uefi-tui/src/app.rs
git commit -m "feat(tui): MenuState — навигация с wrap, скролл-окно 8, refresh/reset (C3 state)"
```

---

### Task 8: App — `cmdline: LineBuffer` + `history` + `menu` (механическая миграция)

**Files:**
- Modify: `crates/uefi-tui/src/app.rs` (поля, `new()`, mode-функции `:474-490`), `crates/uefi-tui/src/main.rs`, `crates/uefi-tui/src/ui/cmdline.rs`, `crates/uefi-tui/src/commands.rs` (точечные `app.cmdline`)

**Interfaces:**
- Consumes: `LineBuffer` (Task 2), `History` (Task 3), `MenuState` (Task 7)
- Produces: `App.cmdline: LineBuffer`, `App.history: History`, `App.menu: MenuState`; поведение клавиш в этой задаче НЕ меняется (старый набор: символы/Backspace/Tab/Enter/Esc)

- [ ] **Step 1: Поля и new()**

```rust
// app.rs, поля структуры:
pub cmdline: crate::line::LineBuffer,
pub menu: MenuState,
pub history: crate::history::History,

// new():
cmdline: crate::line::LineBuffer::new(),
menu: MenuState::default(),
history: crate::history::History::load(),
```

- [ ] **Step 2: Mode-функции**

```rust
pub fn enter_command_mode(&mut self) {
    self.mode = Mode::Command;
    self.cmdline.clear();
    self.insert_cmd = "";
    self.menu.close();
    self.history.reset();
}

pub fn enter_insert_mode(&mut self, cmd: &'static str, prefill: String) {
    self.mode = Mode::Insert;
    self.insert_cmd = cmd;
    self.cmdline.set_str(&prefill);
    self.menu.close();
    self.history.reset();
}

pub fn exit_to_normal(&mut self) {
    self.mode = Mode::Normal;
    self.cmdline.clear();
    self.insert_cmd = "";
    self.menu.close();
}
```

- [ ] **Step 3: Миграция всех использованием `.cmdline`**

Run: `grep -rn "\.cmdline" crates/uefi-tui/src`
Ожидаемые места и паттерны:

| Место | Было | Стало |
|---|---|---|
| `main.rs` handle_command | `app.cmdline.push(*c)` | `app.cmdline.insert(*c)` |
| `main.rs` handle_command | `app.cmdline.pop()` | `app.cmdline.backspace()` |
| `main.rs` handle_command | `let cmdline = app.cmdline.clone()` | `let cmdline = app.cmdline.as_str().to_string()` |
| `main.rs` Tab-ветка | `complete(app, &app.cmdline.clone())` | `complete(app, app.cmdline.as_str())` |
| `main.rs` Tab-ветка | `app.cmdline = r` | `app.cmdline.set_str(&r)` |
| `ui/cmdline.rs` | `format!(":{}▌", app.cmdline)` | `format!(":{}▌", app.cmdline.as_str())` |
| `ui/cmdline.rs` Insert | `app.cmdline.is_empty()` | `app.cmdline.is_empty()` (без изменений) |
| тесты app.rs/main | `app.cmdline = "…".into()` | `app.cmdline.set_str("…")` |
| тесты | `assert_eq!(app.cmdline, "…")` | `assert_eq!(app.cmdline.as_str(), "…")` |

- [ ] **Step 4: Run — passes; commit**

Run: `cargo test -p uefi-tui`
Expected: PASS (все существующие; поведение не изменилось)

```bash
cargo clippy -p uefi-tui -- -D warnings
git add crates/uefi-tui/src/
git commit -m "refactor(tui): App.cmdline String -> LineBuffer + поля history/menu (механическая миграция)"
```

---

### Task 9: `ui/menu.rs` — рендер меню (C3)

**Files:**
- Create: `crates/uefi-tui/src/ui/menu.rs`
- Modify: `crates/uefi-tui/src/ui/mod.rs` (+ `pub mod menu;` + вызов в `render`)

**Interfaces:**
- Consumes: `MenuState`, `MENU_ROWS` (Task 7)
- Produces: `pub fn render(f: &mut Frame, anchor: Rect, menu: &MenuState)` — popup вверх от `anchor.y`, ширина `anchor.width`, title `n/N`

- [ ] **Step 1: Модуль (module-first)**

`ui/mod.rs`: `pub mod menu;` рядом с остальными `pub mod`.

- [ ] **Step 2: Failing test**

`ui/menu.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{MenuItem, MenuState};
    use ratatui::backend::TestBackend;
    use ratatui::layout::Rect;
    use ratatui::Terminal;

    fn row(terminal: &Terminal<TestBackend>, y: u16) -> String {
        (0..40)
            .map(|x| terminal.backend().buffer().get(x, y).symbol().to_string())
            .collect()
    }

    #[test]
    fn popup_renders_items_and_counter() {
        let mut terminal = Terminal::new(TestBackend::new(40, 12)).unwrap();
        let mut menu = MenuState::default();
        menu.open_with(vec![
            MenuItem { display: "alpha".into(), apply: "x alpha".into() },
            MenuItem { display: "beta".into(), apply: "x beta".into() },
        ]);
        terminal
            .draw(|f| render(f, Rect::new(0, 10, 40, 3), &menu))
            .unwrap();
        assert!(row(&terminal, 6).contains("1/2"));
        assert!(row(&terminal, 7).contains("alpha"));
        assert!(row(&terminal, 8).contains("beta"));
    }

    #[test]
    fn closed_menu_renders_nothing() {
        let mut terminal = Terminal::new(TestBackend::new(40, 12)).unwrap();
        terminal
            .draw(|f| render(f, Rect::new(0, 10, 40, 3), &MenuState::default()))
            .unwrap();
        assert!(row(&terminal, 6).trim().is_empty());
    }

    #[test]
    fn height_clamped_to_menu_rows_and_scrolls() {
        let mut terminal = Terminal::new(TestBackend::new(40, 24)).unwrap();
        let mut menu = MenuState::default();
        let items: Vec<MenuItem> = (0..10)
            .map(|i| MenuItem { display: format!("i{i}"), apply: format!("a{i}") })
            .collect();
        menu.open_with(items);
        for _ in 0..9 {
            menu.down();
        }
        terminal
            .draw(|f| render(f, Rect::new(0, 20, 40, 3), &menu))
            .unwrap();
        assert!(row(&terminal, 10).contains("10/10"));
        assert!(row(&terminal, 11).contains("i2"));
        assert!(row(&terminal, 18).contains("i9"));
    }
}
```

- [ ] **Step 3: Run — fails**

Run: `cargo test -p uefi-tui ui::menu`
Expected: FAIL — `render` not found

- [ ] **Step 4: Реализация**

Выше `mod tests`:

```rust
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem};
use ratatui::Frame;

use crate::app::{MenuState, MENU_ROWS};

pub fn render(f: &mut Frame, anchor: Rect, menu: &MenuState) {
    if !menu.open || menu.items.is_empty() {
        return;
    }
    let h = menu.items.len().min(MENU_ROWS) as u16;
    let rows = h + 2;
    let y = anchor.y.saturating_sub(rows);
    let area = Rect { x: anchor.x, y, width: anchor.width, height: rows };
    f.render_widget(Clear, area);
    let visible: Vec<ListItem> = menu.items[menu.offset..menu.offset + h as usize]
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let idx = menu.offset + i;
            let style = if idx == menu.selected {
                Style::default().bg(Color::Blue).add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            ListItem::new(item.display.clone()).style(style)
        })
        .collect();
    let title = format!("{}/{}", menu.selected + 1, menu.items.len());
    let list = List::new(visible).block(Block::default().borders(Borders::ALL).title(title));
    f.render_widget(list, area);
}
```

- [ ] **Step 5: Подключить в layout**

`ui/mod.rs::render`, сразу после `cmdline::render(f, vertical[2], app);`:

```rust
menu::render(f, vertical[2], &app.menu);
```

- [ ] **Step 6: Run — passes; commit**

Run: `cargo test -p uefi-tui`
Expected: PASS

```bash
cargo clippy -p uefi-tui -- -D warnings
git add crates/uefi-tui/src/ui/
git commit -m "feat(tui): completion-меню — popup над cmdline, n/N, подсветка, скролл (C3 render)"
```

---

### Task 10: `ui/cmdline.rs` — курсор + hscroll (C4)

**Files:**
- Modify: `crates/uefi-tui/src/ui/cmdline.rs` (полная замена render)

**Interfaces:**
- Consumes: `LineBuffer::cursor_grapheme()` (Task 2)
- Produces: рендер `:{cmdline}` с курсором-блоком на позиции курсора; горизонтальный скролл окном

- [ ] **Step 1: Failing tests** (новый `mod tests` в cmdline.rs)

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::App;
    use ratatui::backend::TestBackend;
    use ratatui::layout::Rect;
    use ratatui::style::Modifier;
    use ratatui::Terminal;

    fn draw(app: &App) -> Terminal<TestBackend> {
        let mut t = Terminal::new(TestBackend::new(20, 5)).unwrap();
        t.draw(|f| render(f, Rect::new(0, 0, 20, 3), app)).unwrap();
        t
    }

    #[test]
    fn cursor_reversed_mid_string() {
        let mut app = App::new();
        app.mode = crate::app::Mode::Command;
        app.cmdline.set_str("abc");
        app.cmdline.left();
        let t = draw(&app);
        assert_eq!(t.backend().buffer().get(4, 1).symbol(), "c");
        assert!(t.backend().buffer().get(4, 1).modifier().contains(Modifier::REVERSED));
    }

    #[test]
    fn cursor_at_end_is_blank_reversed() {
        let mut app = App::new();
        app.mode = crate::app::Mode::Command;
        app.cmdline.set_str("ab");
        let t = draw(&app);
        assert!(t.backend().buffer().get(4, 1).modifier().contains(Modifier::REVERSED));
    }

    #[test]
    fn long_line_scrolls_to_keep_cursor_visible() {
        let mut app = App::new();
        app.mode = crate::app::Mode::Command;
        app.cmdline.set_str("0123456789abcdefghij");
        let t = draw(&app);
        assert_ne!(t.backend().buffer().get(1, 1).symbol(), "0");
        assert_eq!(t.backend().buffer().get(17, 1).symbol(), "j");
        assert!(t.backend().buffer().get(17, 1).modifier().contains(Modifier::REVERSED));
    }
}
```

- [ ] **Step 2: Run — fails**

Run: `cargo test -p uefi-tui ui::cmdline`
Expected: FAIL

- [ ] **Step 3: Реализация** (заменить `render`)

```rust
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::app::{App, Mode};

pub fn render(f: &mut Frame, area: Rect, app: &App) {
    let inner = (area.width as usize).saturating_sub(2);
    let content = match app.mode {
        Mode::Command => Line::from(cmdline_spans(":", &app.cmdline, inner)),
        Mode::Insert if app.insert_cmd.is_empty() => Line::from(cmdline_spans("> ", &app.cmdline, inner)),
        Mode::Insert => {
            let p = format!("{}> ", app.insert_cmd);
            Line::from(cmdline_spans(&p, &app.cmdline, inner))
        }
        Mode::Normal => Line::from("Press : for commands, i/r/d for insert/replace/remove"),
    };
    let title = match app.mode {
        Mode::Insert => "Insert",
        _ => "Command",
    };
    let p = Paragraph::new(content).block(Block::default().borders(Borders::ALL).title(title));
    f.render_widget(p, area);
}

fn cmdline_spans(prefix: &str, buf: &crate::line::LineBuffer, inner: usize) -> Vec<Span<'static>> {
    use unicode_segmentation::UnicodeSegmentation;
    let text = format!("{}{}", prefix, buf.as_str());
    let graphemes: Vec<&str> = text.graphemes(true).collect();
    let cur = prefix.graphemes(true).count() + buf.cursor_grapheme();
    let start = if cur >= inner { cur + 1 - inner } else { 0 };
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut run = String::new();
    for (gi, g) in graphemes.iter().enumerate().skip(start).take(inner) {
        if *gi == cur {
            if !run.is_empty() {
                spans.push(Span::raw(std::mem::take(&mut run)));
            }
            spans.push(Span::styled(
                g.to_string(),
                Style::default().add_modifier(Modifier::REVERSED),
            ));
        } else {
            run.push_str(g);
        }
    }
    if !run.is_empty() {
        spans.push(Span::raw(run));
    }
    if cur == graphemes.len() {
        spans.push(Span::styled(
            " ".to_string(),
            Style::default().add_modifier(Modifier::REVERSED),
        ));
    }
    spans
}
```

Геометрия тестов: ширина 20 ⇒ inner 18, левая граница x=0, контент с x=1. «abc»+left: cur=3, x=1+3=4 ✓. «ab»: cur=3=len ⇒ хвостовой blank на x=4 ✓. Длинная: cur=22=len ⇒ start=5, «j» (idx 21) на позиции 21−5=16 ⇒ x=17, под курсором reversed ✓.

- [ ] **Step 4: Run — passes; commit**

Run: `cargo test -p uefi-tui`
Expected: PASS

```bash
cargo clippy -p uefi-tui -- -D warnings
git add crates/uefi-tui/src/ui/cmdline.rs
git commit -m "feat(tui): курсор-блок внутри cmdline + горизонтальный скролл (C4)"
```

---

### Task 11: `App::cmd_key` — клавишная карта + интеграция (история, меню)

**Files:**
- Modify: `crates/uefi-tui/src/app.rs` (CmdFlow + cmd_key + accept_menu_selection), `crates/uefi-tui/src/main.rs` (handle_command)

**Interfaces:**
- Consumes: всё из Tasks 2–7
- Produces:
  - `pub enum CmdFlow { Execute, Exit, None }`
  - `impl App { pub fn cmd_key(&mut self, ev: &AppEvent) -> CmdFlow }`
  - приватный `fn accept_menu_selection(&mut self)`

- [ ] **Step 1: Failing tests** (в app.rs `mod tests`; в начале блока — `use crate::input::AppEvent;`, если ещё не импортирован)

```rust
#[test]
fn cmd_key_editing_and_history() {
    let mut app = App::new();
    app.history = crate::history::History::empty();
    app.mode = Mode::Command;
    for c in "save x".chars() {
        app.cmd_key(&AppEvent::Key(c));
    }
    assert_eq!(app.cmdline.as_str(), "save x");
    app.cmd_key(&AppEvent::Left);
    app.cmd_key(&AppEvent::WordLeft);
    app.cmd_key(&AppEvent::Ctrl('w'));
    assert_eq!(app.cmdline.as_str(), "save ");
    assert_eq!(app.cmd_key(&AppEvent::Enter), CmdFlow::Execute);
    app.history.submit("save x");

    app.cmdline.set_str("");
    app.cmd_key(&AppEvent::Up);
    assert_eq!(app.cmdline.as_str(), "save x");
    app.cmd_key(&AppEvent::Backspace);
    assert_eq!(app.cmdline.as_str(), "save ");
    app.cmd_key(&AppEvent::Up);
    assert_eq!(app.cmdline.as_str(), "save x");
}

#[test]
fn cmd_key_tab_opens_menu_and_right_accepts() {
    let mut app = App::new();
    app.history = crate::history::History::empty();
    app.mode = Mode::Command;
    app.cmdline.set_str("s");
    assert_eq!(app.cmd_key(&AppEvent::Tab), CmdFlow::None);
    assert!(app.menu.open);
    assert!(app.menu.selected_apply().unwrap().starts_with("s"));
    app.cmd_key(&AppEvent::Down);
    let expected = app.menu.items[1].apply.clone();
    app.cmd_key(&AppEvent::Right);
    assert_eq!(app.cmdline.as_str(), expected);
    assert!(!app.menu.open);
}

#[test]
fn cmd_key_esc_two_stage_and_enter_flow() {
    let mut app = App::new();
    app.history = crate::history::History::empty();
    app.mode = Mode::Command;
    app.cmdline.set_str("s");
    app.cmd_key(&AppEvent::Tab);
    assert!(app.menu.open);
    assert_eq!(app.cmd_key(&AppEvent::Esc), CmdFlow::None);
    assert!(!app.menu.open);
    assert_eq!(app.cmd_key(&AppEvent::Esc), CmdFlow::Exit);
}
```

- [ ] **Step 2: Run — fails**

Run: `cargo test -p uefi-tui cmd_key`
Expected: FAIL

- [ ] **Step 3: Реализация в app.rs**

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CmdFlow {
    Execute,
    Exit,
    None,
}

impl App {
    pub fn cmd_key(&mut self, ev: &AppEvent) -> CmdFlow {
        use crate::input::AppEvent as E;
        let mut mutated = false;
        let flow = match ev {
            E::Key(c) => {
                self.cmdline.insert(*c);
                mutated = true;
                CmdFlow::None
            }
            E::Ctrl(c) => match c {
                'a' => {
                    self.cmdline.home();
                    CmdFlow::None
                }
                'e' => {
                    self.cmdline.end();
                    CmdFlow::None
                }
                'w' => {
                    self.cmdline.kill_word();
                    mutated = true;
                    CmdFlow::None
                }
                'u' => {
                    self.cmdline.kill_to_start();
                    mutated = true;
                    CmdFlow::None
                }
                'k' => {
                    self.cmdline.kill_to_end();
                    mutated = true;
                    CmdFlow::None
                }
                _ => CmdFlow::None,
            },
            E::Backspace => {
                self.cmdline.backspace();
                mutated = true;
                CmdFlow::None
            }
            E::Delete => {
                self.cmdline.delete();
                mutated = true;
                CmdFlow::None
            }
            E::Left => {
                self.cmdline.left();
                CmdFlow::None
            }
            E::Right => {
                if self.menu.open {
                    self.accept_menu_selection();
                } else {
                    self.cmdline.right();
                }
                CmdFlow::None
            }
            E::WordLeft => {
                self.cmdline.word_left();
                CmdFlow::None
            }
            E::WordRight => {
                self.cmdline.word_right();
                CmdFlow::None
            }
            E::Home => {
                self.cmdline.home();
                CmdFlow::None
            }
            E::End => {
                self.cmdline.end();
                CmdFlow::None
            }
            E::Up => {
                if self.menu.open {
                    self.menu.up();
                } else if let Some(s) = self.history.prev(self.cmdline.as_str()) {
                    self.cmdline.set_str(&s);
                }
                CmdFlow::None
            }
            E::Down => {
                if self.menu.open {
                    self.menu.down();
                } else if let Some(s) = self.history.next(self.cmdline.as_str()) {
                    self.cmdline.set_str(&s);
                }
                CmdFlow::None
            }
            E::Tab => {
                if self.menu.open {
                    self.accept_menu_selection();
                } else {
                    let comp = crate::commands::complete(self, self.cmdline.as_str());
                    if let Some(c) = comp.common {
                        self.cmdline.set_str(&c);
                    }
                    if comp.items.is_empty() {
                        self.menu.close();
                    } else {
                        self.menu.open_with(comp.items);
                    }
                    self.history.reset();
                }
                CmdFlow::None
            }
            E::BackTab => {
                if self.menu.open {
                    self.menu.up();
                }
                CmdFlow::None
            }
            E::Enter => CmdFlow::Execute,
            E::Esc => {
                if self.menu.open {
                    self.menu.close();
                    CmdFlow::None
                } else {
                    CmdFlow::Exit
                }
            }
            _ => CmdFlow::None,
        };
        if mutated {
            self.history.reset();
            if self.menu.open {
                let comp = crate::commands::complete(self, self.cmdline.as_str());
                self.menu.refresh(comp.items);
            }
        }
        flow
    }

    fn accept_menu_selection(&mut self) {
        if let Some(apply) = self.menu.selected_apply().map(str::to_string) {
            self.cmdline.set_str(&apply);
            self.history.reset();
            let comp = crate::commands::complete(self, self.cmdline.as_str());
            self.menu.refresh(comp.items);
        }
    }
}
```

- [ ] **Step 4: main.rs — handle_command**

Заменить целиком:

```rust
async fn handle_command(app: &mut App, ev: &AppEvent, client: &mut Option<commands::Client>) {
    match app.cmd_key(ev) {
        app::CmdFlow::Execute => {
            let cmd = app.cmdline.as_str().to_string();
            if let Some(c) = client {
                match commands::execute_command(app, &cmd, c).await {
                    Ok(_) => {}
                    Err(e) => app.status_msg = format!("error: {e}"),
                }
            } else {
                app.status_msg = "no engine connection".into();
            }
            app.history.submit(&cmd);
            app.history.save();
            app.exit_to_normal();
        }
        app::CmdFlow::Exit => app.exit_to_normal(),
        app::CmdFlow::None => {}
    }
}
```

(`exit_to_normal` из Task 8 уже закрывает меню; `app::CmdFlow` импортировать или писать через путь.)

- [ ] **Step 5: Run — passes; commit**

Run: `cargo test -p uefi-tui`
Expected: PASS

```bash
cargo clippy -p uefi-tui -- -D warnings
git add crates/uefi-tui/src/app.rs crates/uefi-tui/src/main.rs
git commit -m "feat(tui): cmd_key — полная клавишная карта cmdline (история prefix-recall, меню, kill-опы)"
```

---

### Task 12: C6 — контекстный hint-бар

**Files:**
- Modify: `crates/uefi-tui/src/ui/mod.rs` (`render_hint`)

**Interfaces:**
- Consumes: `app.menu.open`
- Produces: hint-строки Command/Insert ± меню

- [ ] **Step 1: Failing tests** (новый `mod tests` в ui/mod.rs)

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{App, MenuItem, Mode};
    use ratatui::backend::TestBackend;
    use ratatui::layout::Rect;
    use ratatui::Terminal;

    fn hint_of(app: &App) -> String {
        let mut t = Terminal::new(TestBackend::new(100, 5)).unwrap();
        t.draw(|f| render_hint(f, Rect::new(0, 0, 100, 1), app)).unwrap();
        (0..100)
            .map(|x| t.backend().buffer().get(x, 0).symbol().to_string())
            .collect()
    }

    #[test]
    fn command_hint_variants() {
        let mut app = App::new();
        app.mode = Mode::Command;
        let closed = hint_of(&app);
        assert!(closed.contains("TAB compl"));
        assert!(closed.contains("↑↓ hist"));
        assert!(closed.contains("Ctrl+W/U/K"));
        app.menu.open_with(vec![MenuItem { display: "x".into(), apply: "x".into() }]);
        let open = hint_of(&app);
        assert!(open.contains("[menu]"));
        assert!(open.contains("↑↓ select"));
        assert!(open.contains("Esc close"));
    }

    #[test]
    fn insert_hint_prefix() {
        let mut app = App::new();
        app.mode = Mode::Insert;
        app.insert_cmd = "insert";
        assert!(hint_of(&app).contains("INSERT"));
    }
}
```

- [ ] **Step 2: Run — fails**

Run: `cargo test -p uefi-tui hint_`
Expected: FAIL

- [ ] **Step 3: Реализация**

В `render_hint` заменить ветки `Mode::Command` и `Mode::Insert` на:

```rust
crate::app::Mode::Command | crate::app::Mode::Insert => {
    let mode = if matches!(app.mode, crate::app::Mode::Command) { "COMMAND" } else { "INSERT" };
            if app.menu.open {
                format!("{mode}[menu]: ↑↓ select · TAB/→ accept · BackTab back · Esc close · Enter run")
            } else {
                format!("{mode}: TAB compl · ↑↓ hist · Ctrl+←→ word · Ctrl+W/U/K del · Home/End · Enter run · Esc cancel")
            }
        }
```

- [ ] **Step 4: Run — passes; commit**

Run: `cargo test -p uefi-tui`
Expected: PASS

```bash
cargo clippy -p uefi-tui -- -D warnings
git add crates/uefi-tui/src/ui/mod.rs
git commit -m "feat(tui): контекстный hint-бар Command/Insert с вариацией по состоянию меню (C6)"
```

---

### Task 13: Финал — проверки, TODO.md, живой гейт

**Files:**
- Modify: `TODO.md`

- [ ] **Step 1: Полные проверки**

```bash
cargo test --all
cargo clippy --all -- -D warnings
cargo fmt --all -- --check
```

Expected: PASS / без warnings / без diff (при diff — `cargo fmt --all`, включить в коммит)

- [ ] **Step 2: TODO.md — закрыть пункты**

В `TODO.md` пункт «uefi-tui: completion-меню не рендерится» (`:3350`) — добавить закрывающую строку и сменить `[ ]`→`[x]`:

```markdown
* [x] **uefi-tui: completion-меню не рендерится** — …
  Закрыто циклом cmdline UX (2026-09-18, спека
  `2026-09-18-tui-cmdline-ux-design.md`): popup-меню над cmdline
  (ui/menu.rs), навигация ↑↓, приём TAB/→, live-фильтрация.
```

В пункте «uefi-tui V3, мелочи» (`:3325`) — отметить закрытыми подпункты (2) item-format, (4) `--ffs` guard, (5) симлинки; (1) fmt_u32_ids и (3) ассерт остаются `[ ]` (вне scope, спека Non-goals).

- [ ] **Step 3: Коммит**

```bash
git add TODO.md
git commit -m "docs(todo): cmdline UX цикл закрыт — completion-меню (3350) + V3-мелочи (2)(4)(5); fmt_u32_ids и пустой-forms-ассерт остаются"
```

- [ ] **Step 4: Живой гейт владельца**

Запуск на живом образе (риг/локально): `uefi-tui` → `:open refs/fw/HNX99TF_200525_original_E5C88C6F.bin --mode write` → прогон чеклиста:
- перемещения по строке (символы/слова/края/kill-опы)
- `:hii question add <TAB>` — меню выпадает, ↑↓ листает, счётчик n/N, TAB принимает
- Up/Down recall истории после нескольких команд; повторный запуск TUI — история на месте (`~/.local/state/uefipatcher/cmdline_history`)
- hint-бар меняется при входе в `:` и при открытии меню
- вердикт владельца зафиксировать аддендумом в спеку (§ «Вердикт», паттерн Revival A)

---

## Само-проверка плана (после написания)

**Спека-покрытие:**
- C1 LineBuffer → Task 2 ✓
- C2 History + state_dir → Tasks 1, 3 ✓
- C3 MenuState + рендер + поведение → Tasks 5, 7, 9, 11 ✓
- C4 курсор + hscroll → Task 10 ✓
- C5 мелочи → Task 6 ✓
- C6 hint-бар → Task 12 ✓
- Клавишная карта спеки ↔ Task 11 (полная таблица) ✓; Right-при-меню-принимает ✓; Up/Down дуальность ✓; двухступенчатый Esc ✓; Enter всегда исполняет ✓
- prefix-recall + saved-восстановление → Task 3 (`prev`/`next`) ✓
- Тесты спеки (unit/render/integration/живой гейт) → Tasks 2,3,7,11 (unit), 9,10,12 (TestBackend), 13 Step 4 (гейт) ✓; integration `tui_integration` не мигрирует явно — `complete()` зовётся только из main.rs/app.rs, тесты используют `execute_command` ✓
- Non-goals не реализованы ✓

**Placeholder scan:** TBD/TODO в шагах нет; все шаги содержат код или точные команды. ✓

**Type consistency:**
- `MenuItem` — app.rs (Task 5), используется commands.rs (Task 5), MenuState (Task 7), меню-рендер и hint-тесты (Tasks 9, 12) ✓
- `Completion { common, items }` — Task 5 → Task 11 ✓
- `LineBuffer::as_str/set_str/cursor_grapheme` — Task 2 → Tasks 8, 10, 11 ✓
- `History::{empty,load,submit,save,reset,prev,next}` — Task 3 → Tasks 8, 11 ✓
- `AppEvent` новые варианты — Task 4 → Task 11 ✓
- `CmdFlow` — Task 11 (app.rs) ↔ main.rs ✓
