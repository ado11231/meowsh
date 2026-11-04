//! The bar: the bottom rows of the window kept for the cat, and the codes that keep the
//! shell out of them.
//!
//! The shell is told the window is `BAR_ROWS` shorter than it is, and only the rows above
//! the bar may scroll. Row 1 stays row 1, so the shell's output needs no changes.

use super::escape::{
    CSI, CURSOR_POSITION, CURSOR_UP, ERASE_LINE, LINE_FEED, RESTORE_CURSOR, SAVE_CURSOR,
    SCROLL_REGION,
};
use anyhow::{Context, Result};
use portable_pty::PtySize;
use std::io::Write;

/// How many rows the bar takes at the bottom of the window.
pub const BAR_ROWS: u16 = 2;

/// The fewest rows the shell keeps while the bar is on. In a shorter window the bar turns
/// off, so the shell is not squeezed.
pub const MIN_SHELL_ROWS: u16 = 8;

/// How the window's rows are shared between the shell and the bar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Layout {
    /// Every row of the window.
    pub rows: u16,
    /// The rows the shell may use, from the top. The same as `rows` when the bar is off.
    pub shell_rows: u16,
}

impl Layout {
    /// Shares a window that is `rows` tall.
    pub fn new(rows: u16) -> Self {
        let shell_rows = if rows >= MIN_SHELL_ROWS + BAR_ROWS {
            rows - BAR_ROWS
        } else {
            rows
        };
        Self { rows, shell_rows }
    }

    /// The bar's top row, counted from 1, or `None` when the window is too short for it.
    pub fn first_bar_row(self) -> Option<u16> {
        (self.shell_rows < self.rows).then_some(self.shell_rows + 1)
    }

    /// The size the shell is told: the window without the bar.
    pub fn shell_size(self, window: PtySize) -> PtySize {
        if self.first_bar_row().is_none() {
            return window;
        }
        // Keep each row the same height in pixels, so programs that size pictures by the
        // cell still get it right. The bar is on, so `rows` is not zero.
        let pixel_height =
            u32::from(window.pixel_height) * u32::from(self.shell_rows) / u32::from(self.rows);
        PtySize {
            rows: self.shell_rows,
            // The result is smaller than the old height, so it always fits.
            pixel_height: u16::try_from(pixel_height).unwrap_or(window.pixel_height),
            ..window
        }
    }
}

/// Keeps the bar rows while it lives, and gives them back to the shell when dropped.
pub struct BarGuard {
    layout: Layout,
}

impl BarGuard {
    /// Keeps the bar rows for `layout`.
    ///
    /// Call this after raw mode is on, so line feeds do not also move to the first column.
    pub fn enter(layout: Layout, output: &mut impl Write) -> Result<Self> {
        write_codes(output, &setup_codes(layout))?;
        Ok(Self { layout })
    }

    /// How the rows are shared right now.
    pub fn layout(&self) -> Layout {
        self.layout
    }

    /// Shares the rows again after the window changed size.
    pub fn resize(&mut self, layout: Layout, output: &mut impl Write) -> Result<()> {
        write_codes(output, &setup_codes(layout))?;
        self.layout = layout;
        Ok(())
    }
}

impl Drop for BarGuard {
    fn drop(&mut self) {
        // Drop cannot be handed the main loop's writer, so this writes to the real terminal
        // directly. Cleanup must also run after an error, so a failure has nowhere to go.
        let _ = write_codes(&mut std::io::stdout(), &leave_codes(self.layout));
    }
}

fn write_codes(output: &mut impl Write, codes: &str) -> Result<()> {
    output
        .write_all(codes.as_bytes())
        .context("could not set up the bar")?;
    output.flush().context("could not set up the bar")
}

/// Codes that share the rows as `layout` says, keeping the cursor on the same line of text.
///
/// Saving the cursor here replaces a save the shell made itself. That is rare, because this
/// only runs at startup and after a resize.
fn setup_codes(layout: Layout) -> String {
    // Let the whole window scroll first. Not every terminal resets this on resize, and an
    // old limit would trap the line feeds below.
    let mut codes = format!("{SAVE_CURSOR}{CSI}{SCROLL_REGION}{RESTORE_CURSOR}");
    let Some(first_bar_row) = layout.first_bar_row() else {
        return codes;
    };
    // When the cursor is in the bottom rows, the line feeds scroll the window up, so that
    // text moves above the bar instead of being covered. Moving back up leaves the cursor
    // on the same line of text.
    for _ in 0..BAR_ROWS {
        codes.push(LINE_FEED);
    }
    codes.push_str(&format!("{CSI}{BAR_ROWS}{CURSOR_UP}{SAVE_CURSOR}"));
    codes.push_str(&limit_scrolling(layout.shell_rows));
    push_erase_bar(&mut codes, first_bar_row, layout.rows);
    codes.push_str(RESTORE_CURSOR);
    codes
}

/// The code that lets only the top `shell_rows` rows scroll. The terminal also moves the
/// cursor to the top left.
pub fn limit_scrolling(shell_rows: u16) -> String {
    format!("{CSI}1;{shell_rows}{SCROLL_REGION}")
}

/// Codes that empty the bar and let the whole window scroll again.
fn leave_codes(layout: Layout) -> String {
    let mut codes = String::from(SAVE_CURSOR);
    if let Some(first_bar_row) = layout.first_bar_row() {
        push_erase_bar(&mut codes, first_bar_row, layout.rows);
    }
    codes.push_str(&format!("{CSI}{SCROLL_REGION}{RESTORE_CURSOR}"));
    codes
}

/// Adds codes that erase every row from `first_bar_row` to `last_row`. They move the cursor.
pub fn push_erase_bar(codes: &mut String, first_bar_row: u16, last_row: u16) {
    for row in first_bar_row..=last_row {
        codes.push_str(&format!("{CSI}{row};1{CURSOR_POSITION}{ERASE_LINE}"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A window tall enough for the bar.
    const TALL_ROWS: u16 = 24;

    fn window(rows: u16, pixel_height: u16) -> PtySize {
        PtySize {
            rows,
            cols: 80,
            pixel_width: 640,
            pixel_height,
        }
    }

    #[test]
    fn a_tall_window_keeps_the_bottom_rows_for_the_bar() {
        let layout = Layout::new(TALL_ROWS);

        assert_eq!(layout.shell_rows, TALL_ROWS - BAR_ROWS);
        assert_eq!(layout.first_bar_row(), Some(TALL_ROWS - BAR_ROWS + 1));
    }

    #[test]
    fn the_bar_turns_off_when_the_shell_would_be_too_short() {
        let shortest_with_bar = MIN_SHELL_ROWS + BAR_ROWS;

        assert!(Layout::new(shortest_with_bar).first_bar_row().is_some());
        for rows in [shortest_with_bar - 1, 1, 0] {
            let layout = Layout::new(rows);
            assert_eq!(layout.shell_rows, rows);
            assert_eq!(layout.first_bar_row(), None);
        }
    }

    #[test]
    fn the_shell_size_leaves_out_the_bar_and_keeps_the_row_height() {
        // 30 pixels a row.
        let size = Layout::new(40).shell_size(window(40, 1200));

        assert_eq!(size.rows, 40 - BAR_ROWS);
        assert_eq!(size.pixel_height, (40 - BAR_ROWS) * 30);
        assert_eq!((size.cols, size.pixel_width), (80, 640));
    }

    #[test]
    fn the_shell_size_keeps_an_unknown_pixel_height_unknown() {
        let size = Layout::new(TALL_ROWS).shell_size(window(TALL_ROWS, 0));

        assert_eq!(size.rows, TALL_ROWS - BAR_ROWS);
        assert_eq!(size.pixel_height, 0);
    }

    #[test]
    fn the_shell_gets_the_whole_window_when_the_bar_is_off() {
        let short = window(MIN_SHELL_ROWS, 300);

        assert_eq!(Layout::new(MIN_SHELL_ROWS).shell_size(short), short);
    }

    #[test]
    fn setup_moves_text_above_the_bar_then_limits_scrolling_and_empties_the_bar() {
        let feeds: String = std::iter::repeat_n(LINE_FEED, usize::from(BAR_ROWS)).collect();
        let expected = format!(
            "{SAVE_CURSOR}{CSI}{SCROLL_REGION}{RESTORE_CURSOR}\
             {feeds}{CSI}{BAR_ROWS}{CURSOR_UP}\
             {SAVE_CURSOR}{CSI}1;22{SCROLL_REGION}\
             {CSI}23;1{CURSOR_POSITION}{ERASE_LINE}{CSI}24;1{CURSOR_POSITION}{ERASE_LINE}\
             {RESTORE_CURSOR}"
        );

        assert_eq!(setup_codes(Layout::new(TALL_ROWS)), expected);
    }

    #[test]
    fn setup_without_a_bar_only_lets_the_whole_window_scroll() {
        let expected = format!("{SAVE_CURSOR}{CSI}{SCROLL_REGION}{RESTORE_CURSOR}");

        assert_eq!(setup_codes(Layout::new(MIN_SHELL_ROWS)), expected);
    }

    #[test]
    fn leaving_empties_the_bar_and_lets_the_whole_window_scroll() {
        let with_bar = format!(
            "{SAVE_CURSOR}\
             {CSI}23;1{CURSOR_POSITION}{ERASE_LINE}{CSI}24;1{CURSOR_POSITION}{ERASE_LINE}\
             {CSI}{SCROLL_REGION}{RESTORE_CURSOR}"
        );
        let without_bar = format!("{SAVE_CURSOR}{CSI}{SCROLL_REGION}{RESTORE_CURSOR}");

        assert_eq!(leave_codes(Layout::new(TALL_ROWS)), with_bar);
        assert_eq!(leave_codes(Layout::new(MIN_SHELL_ROWS)), without_bar);
    }

    #[test]
    fn the_guard_writes_setup_on_enter_and_resize() -> Result<()> {
        let mut output = Vec::new();
        let mut guard = BarGuard::enter(Layout::new(TALL_ROWS), &mut output)?;
        guard.resize(Layout::new(40), &mut output)?;
        // Dropping writes to the real stdout, which this test does not read.
        std::mem::forget(guard);

        let expected = setup_codes(Layout::new(TALL_ROWS)) + &setup_codes(Layout::new(40));
        assert_eq!(String::from_utf8(output)?, expected);
        Ok(())
    }
}
