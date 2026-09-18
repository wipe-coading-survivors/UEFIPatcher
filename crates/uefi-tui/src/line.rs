use unicode_segmentation::UnicodeSegmentation;

pub struct LineBuffer {
    s: String,
    cursor: usize,
}

impl LineBuffer {
    pub fn new() -> Self {
        Self {
            s: String::new(),
            cursor: 0,
        }
    }

    #[allow(clippy::should_implement_trait)]
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
        if let Some((i, w)) = self.s.split_word_bound_indices().find(|(i, w)| {
            i + w.len() > self.cursor && w.chars().next().is_some_and(char::is_alphanumeric)
        }) {
            self.cursor = i + w.len();
        } else {
            self.cursor = self.s.len();
        }
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
