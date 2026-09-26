//! A file the editor pane owns: text, cursor, and the edits a session makes.
//!
//! Line and byte offsets are kept apart on purpose. The cursor is a byte offset
//! because that is what a terminal reports and what an IME commits, while the
//! layout needs lines, so both are tracked and neither is guessed at draw time.

use std::path::{Path, PathBuf};

/// An open document.
pub struct Document {
    path: PathBuf,
    text: String,
    /// Byte offset of the first character of every line, so `starts[line]` is
    /// where line `line` begins. Always begins with 0, and a document ending
    /// in a newline has a final empty line, which is what an editor shows.
    starts: Vec<usize>,
    cursor: usize,
    top_line: usize,
    dirty: bool,
}

impl Document {
    /// Reads a file, or starts empty when it does not exist yet.
    pub fn open(path: impl Into<PathBuf>) -> std::io::Result<Self> {
        let path = path.into();
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(error) => return Err(error),
        };
        let mut document = Self {
            path,
            text,
            starts: vec![0],
            cursor: 0,
            top_line: 0,
            dirty: false,
        };
        document.reindex(0);
        Ok(document)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn top_line(&self) -> usize {
        self.top_line
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn lines(&self) -> usize {
        self.starts.len()
    }

    /// The visible window of lines, for a viewport `rows` tall.
    pub fn visible(&self, rows: usize) -> Vec<&str> {
        let start = self.top_line.min(self.starts.len().saturating_sub(1));
        let end = (start + rows.max(1)).min(self.starts.len());
        (start..end).map(|line| self.line_text(line)).collect()
    }

    /// The text of one line, without its newline.
    pub fn line_text(&self, line: usize) -> &str {
        let start = self.line_start(line);
        let end = self
            .starts
            .get(line + 1)
            .copied()
            .unwrap_or(self.text.len())
            .max(start);
        self.text[start..end].trim_end_matches('\n')
    }

    /// The line the cursor is on, zero-based.
    pub fn cursor_line(&self) -> usize {
        let cursor = self.cursor.min(self.text.len());
        // Every line start at or before the cursor is on the cursor's line or
        // above it, so the count is the line itself.
        self.starts.partition_point(|&start| start <= cursor) - 1
    }

    /// The byte offset of the start of a line, clamped to the document.
    pub fn line_start(&self, line: usize) -> usize {
        self.starts.get(line).copied().unwrap_or(self.text.len())
    }

    /// Moves the cursor to a line and column, keeping it inside the document and
    /// snapping the column to the line it lands on.
    pub fn goto(&mut self, line: usize, column: usize) {
        let start = self.line_start(line);
        let end = self
            .starts
            .get(line + 1)
            .map(|next| next.saturating_sub(1))
            .unwrap_or(self.text.len())
            .max(start);
        self.cursor = (start + column.min(end - start)).min(self.text.len());
        self.scroll_to_cursor();
    }

    /// Rebuilds the line index from the first line that can have changed.
    ///
    /// An edit at `from` only moves the starts at or after it, so the lines
    /// before that are kept: pasting into a large file costs a scan of the
    /// rest of the file, not of the whole file.
    fn reindex(&mut self, from: usize) {
        // Line 0 always starts at 0, so at least the first start survives.
        let keep = self.starts.partition_point(|&start| start <= from).max(1);
        self.starts.truncate(keep);
        let mut offset = self.starts[keep - 1];
        for (index, byte) in self.text.as_bytes()[offset..].iter().enumerate() {
            if *byte == b'\n' {
                offset += index + 1;
                self.starts.push(offset);
            }
        }
    }

    /// Keeps the cursor line inside the viewport.
    pub fn scroll_to_cursor(&mut self) {
        self.top_line = self.top_line.min(self.cursor_line());
    }

    pub fn scroll(&mut self, delta: i32) {
        let last = self.lines().saturating_sub(1);
        self.top_line = (self.top_line as i64 + delta as i64).clamp(0, last as i64) as usize;
    }

    /// Inserts text at the cursor.
    pub fn insert(&mut self, text: &str) {
        let at = self.cursor.min(self.text.len());
        self.text.insert_str(at, text);
        self.cursor = at + text.len();
        self.reindex(at);
        self.dirty = true;
    }

    /// Deletes the character before the cursor.
    pub fn backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let previous = self.text[..self.cursor]
            .char_indices()
            .next_back()
            .map(|(index, _)| index)
            .unwrap_or(0);
        self.text.replace_range(previous..self.cursor, "");
        self.cursor = previous;
        self.reindex(previous);
        self.dirty = true;
    }

    /// Moves the cursor left one character.
    pub fn left(&mut self) {
        if self.cursor == 0 {
            return;
        }
        self.cursor = self.text[..self.cursor]
            .char_indices()
            .next_back()
            .map(|(index, _)| index)
            .unwrap_or(0);
    }

    /// Moves the cursor right one character.
    pub fn right(&mut self) {
        if self.cursor >= self.text.len() {
            return;
        }
        self.cursor += self.text[self.cursor..]
            .chars()
            .next()
            .map(char::len_utf8)
            .unwrap_or(1);
    }

    /// Writes the file, clearing the dirty flag.
    pub fn save(&mut self) -> std::io::Result<()> {
        std::fs::write(&self.path, &self.text)?;
        self.dirty = false;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn document(text: &str) -> Document {
        let mut document = Document::open("/tmp/keplr-client-doc-test.rs").expect("opens");
        document.text = text.to_string();
        document.reindex(0);
        document
    }

    #[test]
    fn a_new_file_starts_empty_and_clean() {
        let document = Document::open("/tmp/keplr-client-missing-file.rs").expect("opens");
        assert_eq!(document.text(), "");
        assert!(!document.is_dirty());
        assert_eq!(document.lines(), 1);
    }

    #[test]
    fn the_cursor_line_counts_newlines() {
        let document = document("a\nbb\nccc");
        assert_eq!(document.cursor_line(), 0);
        let mut document = document;
        document.goto(2, 1);
        assert_eq!(document.cursor_line(), 2);
        assert_eq!(document.cursor(), 6);
    }

    #[test]
    fn a_column_past_the_end_of_a_line_snaps_to_it() {
        let mut document = document("ab\ncdef");
        document.goto(0, 99);
        assert_eq!(document.cursor(), 2, "the cursor stops at the newline");
        document.goto(1, 99);
        assert_eq!(
            document.cursor(),
            7,
            "the last line has no newline to stop at"
        );
    }

    #[test]
    fn goto_clamps_a_line_past_the_end() {
        let mut document = document("one\ntwo");
        document.goto(99, 0);
        assert_eq!(document.cursor_line(), 1);
    }

    #[test]
    fn typing_inserts_and_marks_the_file_dirty() {
        let mut document = document("ab");
        document.goto(0, 1);
        document.insert("X");
        assert_eq!(document.text(), "aXb");
        assert!(document.is_dirty());
        document.backspace();
        assert_eq!(document.text(), "ab");
    }

    #[test]
    fn backspace_at_the_start_does_nothing() {
        let mut document = document("ab");
        document.goto(0, 0);
        document.backspace();
        assert_eq!(document.text(), "ab");
    }

    #[test]
    fn the_cursor_moves_over_a_character_not_a_byte() {
        let mut document = document("héllo");
        document.goto(0, 0);
        document.right();
        // h is one byte, so the cursor sits on the start of the two-byte é.
        assert_eq!(document.cursor(), 1);
        document.right();
        assert_eq!(document.cursor(), 3, "the whole é is one step");
        document.left();
        assert_eq!(document.cursor(), 1);
    }

    #[test]
    fn scrolling_stays_inside_the_document() {
        let mut document = document("a\nb\nc\nd");
        assert_eq!(document.lines(), 4);
        document.scroll(-5);
        assert_eq!(document.top_line(), 0);
        document.scroll(99);
        assert_eq!(document.top_line(), 3, "the last line is the floor");
    }

    #[test]
    fn pasting_a_newline_moves_every_line_after_it() {
        let mut document = document("l0\nl1\nl2\nl3");
        document.goto(1, 1);
        document.insert("X\nY");
        assert_eq!(document.text(), "l0\nl1X\nYl2\nl3");
        assert_eq!(document.lines(), 5);
        assert_eq!(document.line_text(0), "l0");
        assert_eq!(document.line_text(1), "l1X");
        assert_eq!(document.line_text(2), "Yl2");
        assert_eq!(document.line_text(3), "l3");
        assert_eq!(document.line_start(3), document.text().find("l3").unwrap());
        assert_eq!(
            document.cursor_line(),
            2,
            "the cursor is on the pasted line"
        );
    }

    #[test]
    fn deleting_a_newline_pulls_the_lines_back_together() {
        let mut document = document("l0\nl1\nl2");
        document.goto(1, 2);
        document.backspace();
        assert_eq!(document.text(), "l0\nl2");
        assert_eq!(document.lines(), 2);
        assert_eq!(document.line_text(1), "l2");
        assert_eq!(document.cursor_line(), 1);
    }

    #[test]
    fn a_file_ending_in_a_newline_has_a_last_empty_line() {
        let document = document("l0\n");
        assert_eq!(document.lines(), 2);
        assert_eq!(document.line_text(1), "");
        assert_eq!(document.visible(4), vec!["l0", ""]);
    }

    #[test]
    fn the_line_index_is_kept_in_step_over_many_edits() {
        let mut document = document(&"line\n".repeat(500));
        for round in 0..50 {
            document.goto(round, 0);
            document.insert("x");
        }
        assert_eq!(document.lines(), 550, "every pasted line is a line");
        assert_eq!(document.line_text(0), "xline");
        assert_eq!(document.line_text(49), "xline");
        assert_eq!(document.line_text(50), "line");
        assert_eq!(document.cursor_line(), 49);
        assert_eq!(document.line_start(550), document.text().len());
    }

    #[test]
    fn the_visible_window_follows_the_scroll() {
        let mut document = document("l0\nl1\nl2\nl3");
        document.scroll(2);
        assert_eq!(document.visible(2), vec!["l2", "l3"]);
    }
}
