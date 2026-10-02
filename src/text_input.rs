/// A single-line text buffer with a cursor, counted in chars (not bytes) so
/// multi-byte input is handled safely.
#[derive(Default)]
pub struct TextInput {
    value: String,
    cursor: usize,
}

impl TextInput {
    /// Replaces the contents and puts the cursor at the end.
    pub fn set(&mut self, value: String) {
        self.cursor = value.chars().count();
        self.value = value;
    }

    pub fn value(&self) -> &str {
        &self.value
    }

    /// The text before the cursor, e.g. for computing its display column.
    pub fn before_cursor(&self) -> &str {
        &self.value[..self.byte_index(self.cursor)]
    }

    fn len(&self) -> usize {
        self.value.chars().count()
    }

    /// Byte offset of the char at `char_pos`.
    fn byte_index(&self, char_pos: usize) -> usize {
        self.value
            .char_indices()
            .nth(char_pos)
            .map_or(self.value.len(), |(i, _)| i)
    }

    pub fn insert(&mut self, c: char) {
        let idx = self.byte_index(self.cursor);
        self.value.insert(idx, c);
        self.cursor += 1;
    }

    pub fn delete_before_cursor(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
            let idx = self.byte_index(self.cursor);
            self.value.remove(idx);
        }
    }

    pub fn delete_at_cursor(&mut self) {
        if self.cursor < self.len() {
            let idx = self.byte_index(self.cursor);
            self.value.remove(idx);
        }
    }

    pub fn delete_word_before_cursor(&mut self) {
        let chars: Vec<char> = self.value.chars().collect();
        let mut start = self.cursor;
        while start > 0 && chars[start - 1].is_whitespace() {
            start -= 1;
        }
        while start > 0 && !chars[start - 1].is_whitespace() {
            start -= 1;
        }
        let (from, to) = (self.byte_index(start), self.byte_index(self.cursor));
        self.value.replace_range(from..to, "");
        self.cursor = start;
    }

    pub fn delete_to_start(&mut self) {
        let to = self.byte_index(self.cursor);
        self.value.replace_range(..to, "");
        self.cursor = 0;
    }

    pub fn move_left(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    pub fn move_right(&mut self) {
        self.cursor = (self.cursor + 1).min(self.len());
    }

    pub fn move_home(&mut self) {
        self.cursor = 0;
    }

    pub fn move_end(&mut self) {
        self.cursor = self.len();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handles_multibyte_chars() {
        let mut input = TextInput::default();
        for c in "héllo wörld 🎉".chars() {
            input.insert(c);
        }
        assert_eq!(input.value(), "héllo wörld 🎉");
        input.delete_before_cursor();
        assert_eq!(input.value(), "héllo wörld ");
        input.delete_word_before_cursor();
        assert_eq!(input.value(), "héllo ");
        input.move_home();
        input.move_right();
        assert_eq!(input.before_cursor(), "h");
        input.delete_at_cursor();
        assert_eq!(input.value(), "hllo ");
        input.move_end();
        input.delete_to_start();
        assert_eq!(input.value(), "");
    }

    #[test]
    fn cursor_stays_in_bounds() {
        let mut input = TextInput::default();
        input.set("ab".to_string());
        input.move_right();
        assert_eq!(input.before_cursor(), "ab");
        input.move_home();
        input.move_left();
        input.delete_before_cursor();
        assert_eq!(input.value(), "ab");
    }
}
