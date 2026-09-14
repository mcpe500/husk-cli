//! Modal line editor (opencode-style Normal/Insert).
//!
//! Insert mode: typing writes; readline/emacs shortcuts (Ctrl+A/E/K/U).
//! Normal mode: keys become commands — `i`/`a` back to insert, `k`/`j` or
//! arrows scroll handled by the app, not here.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Insert,
    Normal,
}

#[derive(Debug, Clone)]
pub struct InputState {
    pub text: String,
    /// Byte index into `text` (always on a char boundary).
    pub cursor: usize,
    pub mode: Mode,
}

impl Default for InputState {
    fn default() -> Self {
        Self { text: String::new(), cursor: 0, mode: Mode::Insert }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum KeyEffect {
    None,
    /// Text/cursor changed.
    Edited,
    /// Enter pressed in insert mode.
    Submit,
    /// Esc pressed (drop to Normal).
    ToNormal,
    /// `i`/`a` pressed in normal mode.
    ToInsert,
    /// Ctrl+C in normal mode / empty — quit signal.
    Quit,
}

impl InputState {
    pub fn handle_char(&mut self, c: char) -> KeyEffect {
        match self.mode {
            Mode::Insert => {
                self.insert_char(c);
                KeyEffect::Edited
            }
            Mode::Normal => match c {
                'i' => {
                    self.mode = Mode::Insert;
                    KeyEffect::ToInsert
                }
                'a' => {
                    self.mode = Mode::Insert;
                    self.cursor = self.text.len();
                    KeyEffect::ToInsert
                }
                _ => KeyEffect::None,
            },
        }
    }

    pub fn insert_char(&mut self, c: char) {
        self.text.insert(self.cursor, c);
        self.cursor += c.len_utf8();
    }

    pub fn backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let prev = self.text[..self.cursor]
            .char_indices()
            .next_back()
            .map(|(i, _)| i)
            .unwrap_or(0);
        self.text.replace_range(prev..self.cursor, "");
        self.cursor = prev;
    }

    pub fn delete(&mut self) {
        if self.cursor >= self.text.len() {
            return;
        }
        let next = self.text[self.cursor..]
            .char_indices()
            .nth(1)
            .map(|(i, _)| self.cursor + i)
            .unwrap_or(self.text.len());
        self.text.replace_range(self.cursor..next, "");
    }

    pub fn home(&mut self) {
        self.cursor = 0;
    }

    pub fn end(&mut self) {
        self.cursor = self.text.len();
    }

    /// Delete to end of line (Ctrl+K).
    pub fn kill_to_end(&mut self) {
        self.text.truncate(self.cursor);
    }

    /// Delete to start of line (Ctrl+U).
    pub fn kill_to_start(&mut self) {
        self.text.replace_range(..self.cursor, "");
        self.cursor = 0;
    }

    pub fn left(&mut self) {
        self.cursor = self.text[..self.cursor]
            .char_indices()
            .next_back()
            .map(|(i, _)| i)
            .unwrap_or(0);
    }

    pub fn right(&mut self) {
        self.cursor = self.text[self.cursor..]
            .char_indices()
            .nth(1)
            .map(|(i, _)| self.cursor + i)
            .unwrap_or(self.text.len());
    }

    pub fn submit(&mut self) -> String {
        self.mode = Mode::Normal;
        std::mem::take(&mut self.text)
    }

    pub fn handle_esc(&mut self) -> KeyEffect {
        if self.mode == Mode::Insert {
            self.mode = Mode::Normal;
            KeyEffect::ToNormal
        } else {
            KeyEffect::None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn insert_mode() -> InputState {
        InputState { mode: Mode::Insert, ..Default::default() }
    }


    #[test]
    fn typing_appends_and_cursor_tracks_chars() {
        let mut input = InputState::default();
        for c in "héllo".chars() {
            input.handle_char(c);
        }
        assert_eq!(input.text, "héllo");
        assert_eq!(input.cursor, "héllo".len(), "cursor is a byte index");
    }

    #[test]
    fn readline_shortcuts_work() {
        let mut input = insert_mode();
        for c in "fix the bug".chars() {
            input.insert_char(c);
        }
        input.home();
        assert_eq!(input.cursor, 0);
        input.kill_to_end();
        assert_eq!(input.text, "");
        for c in "tail text".chars() {
            input.insert_char(c);
        }
        input.kill_to_start();
        assert_eq!(input.text, "");
        assert_eq!(input.cursor, 0);
    }

    #[test]
    fn backspace_never_splits_utf8() {
        let mut input = insert_mode();
        for c in "aé".chars() {
            input.insert_char(c);
        }
        input.backspace();
        assert_eq!(input.text, "a");
        input.backspace();
        assert_eq!(input.text, "");
        input.backspace(); // no-op at start
        assert_eq!(input.text, "");
    }

    #[test]
    fn modes_transition_like_opencode() {
        let mut input = InputState::default();
        assert_eq!(input.mode, Mode::Insert);
        // Esc drops to normal.
        assert_eq!(input.handle_esc(), KeyEffect::ToNormal);
        assert_eq!(input.mode, Mode::Normal);
        // i/a return to insert.
        assert_eq!(input.handle_char('a'), KeyEffect::ToInsert);
        assert_eq!(input.mode, Mode::Insert);
    }

    #[test]
    fn submit_clears_and_switches_to_normal() {
        let mut input = insert_mode();
        for c in "do it".chars() {
            input.insert_char(c);
        }
        assert_eq!(input.submit(), "do it");
        assert_eq!(input.text, "");
        assert_eq!(input.mode, Mode::Normal);
    }
}
