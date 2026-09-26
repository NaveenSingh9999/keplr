//! Terminal state for Keplr, shared by the browser client and the native one.
//!
//! The grid is an alacritty terminal with a typed snapshot instead of the JSON
//! the websocket path used to build by hand, so a native renderer can consume
//! cells directly and the browser path can serialize the same structure.

pub mod pty;

use std::path::{Path, PathBuf};

use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::{Config, Term, TermMode};
use alacritty_terminal::vte::ansi::{Color, NamedColor, Processor, StdSyncHandler};
use serde::Serialize;

/// Scrollback kept per terminal.
pub const HISTORY_LINES: usize = 10_000;

/// One cell of the grid: a character with its colors and flags.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Cell {
    pub ch: String,
    /// CSS color, or `None` for the terminal default.
    pub fg: Option<String>,
    pub bg: Option<String>,
    pub flags: u8,
}

impl Cell {
    pub const BOLD: u8 = 1;
    pub const ITALIC: u8 = 2;
    pub const INVERSE: u8 = 4;
}

/// Where the caret is, in grid coordinates.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Cursor {
    pub line: i32,
    pub column: usize,
}

/// A frame of terminal state: the visible grid plus recent scrollback.
#[derive(Clone, Debug, Serialize)]
pub struct Snapshot {
    pub cols: usize,
    pub rows: usize,
    pub cursor: Cursor,
    pub show_cursor: bool,
    /// Plain-text scrollback lines, oldest first, already trimmed.
    pub history: Vec<String>,
    pub cells: Vec<Vec<Cell>>,
}

/// A terminal size, satisfying the alacritty dimension trait.
#[derive(Clone, Copy, Debug)]
pub struct GridSize {
    pub cols: u16,
    pub rows: u16,
}

impl alacritty_terminal::grid::Dimensions for GridSize {
    fn total_lines(&self) -> usize {
        HISTORY_LINES + self.rows as usize
    }

    fn screen_lines(&self) -> usize {
        self.rows as usize
    }

    fn columns(&self) -> usize {
        self.cols as usize
    }
}

/// The event listener alacritty requires; Keplr renders by snapshot instead.
pub struct Listener;

impl alacritty_terminal::event::EventListener for Listener {}

/// A terminal grid with an ANSI processor, driven by raw bytes.
pub struct TerminalGrid {
    term: Term<Listener>,
    processor: Processor<StdSyncHandler>,
    cols: usize,
    rows: usize,
}

impl TerminalGrid {
    pub fn new(cols: usize, rows: usize) -> Self {
        let size = GridSize {
            cols: cols.max(1) as u16,
            rows: rows.max(1) as u16,
        };
        let config = Config {
            scrolling_history: HISTORY_LINES,
            ..Config::default()
        };
        Self {
            term: Term::new(config, &size, Listener),
            processor: Processor::new(),
            cols: size.cols as usize,
            rows: size.rows as usize,
        }
    }

    pub fn cols(&self) -> usize {
        self.cols
    }

    pub fn rows(&self) -> usize {
        self.rows
    }

    /// Feeds shell output through the ANSI processor.
    pub fn advance(&mut self, bytes: &[u8]) {
        self.processor.advance(&mut self.term, bytes);
    }

    /// Resizes the grid, keeping as much scrollback as fits.
    pub fn resize(&mut self, cols: usize, rows: usize) {
        let cols = cols.max(1);
        let rows = rows.max(1);
        if cols == self.cols && rows == self.rows {
            return;
        }
        let size = GridSize {
            cols: cols as u16,
            rows: rows as u16,
        };
        self.term.resize(size);
        self.cols = cols;
        self.rows = rows;
    }

    /// Number of scrollback lines currently held.
    pub fn history_size(&self) -> usize {
        self.term.grid().history_size()
    }

    /// The visible grid plus the most recent scrollback, as plain text.
    pub fn snapshot(&self) -> Snapshot {
        let content = self.term.renderable_content();
        let cursor = content.cursor.point;
        let history_limit = self.term.grid().history_size().min(500);
        let mut history = Vec::with_capacity(history_limit);
        for line in -(history_limit as i32)..0 {
            let mut text = String::with_capacity(self.cols);
            for column in 0..self.cols {
                text.push(self.term.grid()[Line(line)][Column(column)].c);
            }
            history.push(text.trim_end().to_string());
        }
        while history.last().is_some_and(|line| line.is_empty()) {
            history.pop();
        }

        let mut cells = Vec::with_capacity(self.rows);
        for line in 0..self.rows {
            let mut row = Vec::with_capacity(self.cols);
            for column in 0..self.cols {
                let cell = &self.term.grid()[Line(line as i32)][Column(column)];
                let mut flags = 0u8;
                if cell.flags.contains(Flags::BOLD) {
                    flags |= Cell::BOLD;
                }
                if cell.flags.contains(Flags::ITALIC) {
                    flags |= Cell::ITALIC;
                }
                if cell.flags.contains(Flags::INVERSE) {
                    flags |= Cell::INVERSE;
                }
                row.push(Cell {
                    ch: cell.c.to_string(),
                    fg: css(&cell.fg),
                    bg: css(&cell.bg),
                    flags,
                });
            }
            cells.push(row);
        }

        Snapshot {
            cols: self.cols,
            rows: self.rows,
            cursor: Cursor {
                line: cursor.line.0,
                column: cursor.column.0,
            },
            show_cursor: content.mode.contains(TermMode::SHOW_CURSOR),
            history,
            cells,
        }
    }
}

/// An ANSI color as a CSS string, or `None` for the terminal default.
pub fn css(color: &Color) -> Option<String> {
    let rgb = match color {
        Color::Named(named) => match named {
            NamedColor::Black => (0, 0, 0),
            NamedColor::Red => (205, 49, 49),
            NamedColor::Green => (13, 188, 121),
            NamedColor::Yellow => (229, 229, 16),
            NamedColor::Blue => (36, 114, 200),
            NamedColor::Magenta => (188, 63, 188),
            NamedColor::Cyan => (17, 168, 205),
            NamedColor::White => (229, 229, 229),
            NamedColor::BrightBlack => (102, 102, 102),
            NamedColor::BrightRed => (241, 76, 76),
            NamedColor::BrightGreen => (35, 209, 139),
            NamedColor::BrightYellow => (245, 245, 67),
            NamedColor::BrightBlue => (59, 142, 234),
            NamedColor::BrightMagenta => (214, 112, 214),
            NamedColor::BrightCyan => (41, 184, 219),
            NamedColor::BrightWhite => (255, 255, 255),
            _ => return None,
        },
        Color::Spec(rgb) => (rgb.r, rgb.g, rgb.b),
        Color::Indexed(index) => {
            let index = *index;
            if index < 16 {
                let named = match index {
                    0 => NamedColor::Black,
                    1 => NamedColor::Red,
                    2 => NamedColor::Green,
                    3 => NamedColor::Yellow,
                    4 => NamedColor::Blue,
                    5 => NamedColor::Magenta,
                    6 => NamedColor::Cyan,
                    7 => NamedColor::White,
                    8 => NamedColor::BrightBlack,
                    9 => NamedColor::BrightRed,
                    10 => NamedColor::BrightGreen,
                    11 => NamedColor::BrightYellow,
                    12 => NamedColor::BrightBlue,
                    13 => NamedColor::BrightMagenta,
                    14 => NamedColor::BrightCyan,
                    _ => NamedColor::BrightWhite,
                };
                return css(&Color::Named(named));
            }
            if index >= 232 {
                let level = 8 + (index - 232) * 10;
                return Some(format!("#{level:02x}{level:02x}{level:02x}"));
            }
            let value = index - 16;
            let steps = [0u8, 95, 135, 175, 215, 255];
            let r = steps[(value / 36) as usize];
            let g = steps[((value % 36) / 6) as usize];
            let b = steps[(value % 6) as usize];
            (r, g, b)
        }
    };
    Some(format!("#{:02x}{:02x}{:02x}", rgb.0, rgb.1, rgb.2))
}

/// Resolves a requested working directory inside the workspace root.
///
/// A terminal is a shell, so containment matters: an absolute path, a parent
/// component, or anything that escapes the root falls back to the root itself.
pub fn contained_cwd(root: &Path, requested: &str) -> PathBuf {
    let raw = Path::new(requested);
    if requested.is_empty() || raw.is_absolute() {
        return root.to_path_buf();
    }
    if raw
        .components()
        .any(|part| matches!(part, std::path::Component::ParentDir))
    {
        return root.to_path_buf();
    }
    let candidate = root.join(raw);
    if candidate.starts_with(root) {
        candidate
    } else {
        root.to_path_buf()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid_with(bytes: &[u8]) -> TerminalGrid {
        let mut grid = TerminalGrid::new(20, 4);
        grid.advance(bytes);
        grid
    }

    #[test]
    fn plain_text_lands_in_the_visible_grid() {
        let grid = grid_with(b"hello");
        let snapshot = grid.snapshot();
        assert_eq!(snapshot.cols, 20);
        assert_eq!(snapshot.rows, 4);
        let first: String = snapshot.cells[0]
            .iter()
            .map(|cell| cell.ch.as_str())
            .collect();
        assert_eq!(first, "hello               ");
    }

    #[test]
    fn ansi_colors_become_css_values() {
        let grid = grid_with(b"\x1b[31mred\x1b[0m");
        let snapshot = grid.snapshot();
        let cell = &snapshot.cells[0][0];
        assert_eq!(cell.ch, "r");
        assert_eq!(cell.fg.as_deref(), Some("#cd3131"));
    }

    #[test]
    fn bold_and_italic_set_their_flags() {
        let grid = grid_with(b"\x1b[1;3mhi\x1b[0m");
        let snapshot = grid.snapshot();
        assert_eq!(snapshot.cells[0][0].flags & Cell::BOLD, Cell::BOLD);
        assert_eq!(snapshot.cells[0][0].flags & Cell::ITALIC, Cell::ITALIC);
    }

    #[test]
    fn scrolling_keeps_lines_in_history() {
        let mut grid = TerminalGrid::new(20, 2);
        grid.advance(b"one\r\ntwo\r\nthree\r\n");
        let snapshot = grid.snapshot();
        assert!(snapshot.history.iter().any(|line| line == "one"));
        assert!(snapshot.history.iter().any(|line| line == "two"));
    }

    #[test]
    fn resize_keeps_the_grid_consistent() {
        let mut grid = grid_with(b"hello");
        grid.resize(40, 8);
        let snapshot = grid.snapshot();
        assert_eq!(snapshot.cols, 40);
        assert_eq!(snapshot.rows, 8);
        assert_eq!(snapshot.cells[0][0].ch, "h");
    }

    #[test]
    fn cursor_reports_its_cell() {
        let grid = grid_with(b"ab\r\ncd");
        let snapshot = grid.snapshot();
        assert_eq!(snapshot.cursor.line, 1);
        assert_eq!(snapshot.cursor.column, 2);
    }

    #[test]
    fn a_terminal_cwd_stays_inside_the_root() {
        let root = std::path::Path::new("/tmp/keplr-root");
        assert_eq!(contained_cwd(root, "src"), root.join("src"));
        assert_eq!(contained_cwd(root, "../escape"), root);
        assert_eq!(contained_cwd(root, "/etc"), root);
        assert_eq!(contained_cwd(root, ""), root);
    }
}
