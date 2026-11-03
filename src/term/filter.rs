//! Passes the shell's output on, changing only codes that would reach the bar.
//!
//! Scroll limits and cursor moves are stopped above the bar. Clears, screen switches and
//! resets are passed on and noted, so the bar can be drawn again. Everything else passes
//! through unchanged, and a code split across two reads is held until it ends.

use super::bar::{Layout, limit_scrolling};
use super::escape::{
    CANCEL, CSI, CURSOR_POSITION, CURSOR_POSITION_OLD, ERASE_ABOVE, ERASE_SCREEN, ESCAPE,
    FULL_RESET, MODE_OFF, MODE_ON, OTHER_SCREEN_MODES, PRIVATE_MARKER, RESTORE_CURSOR,
    ROW_POSITION, SAVE_CURSOR, SCROLL_REGION, SOFT_RESET, SOFT_RESET_MARKER, SUBSTITUTE,
    SYNCHRONIZED_OUTPUT_MODE,
};
use vte::{Params, Parser, Perform};

/// Watches the shell's output on its way to the real terminal.
pub struct OutputFilter {
    parser: Parser,
    /// The bytes of a code that has not ended yet.
    held: Vec<u8>,
    /// Controls, such as a line feed, that arrived inside the held code. Terminals act on
    /// them right away, so they are kept when the code around them is changed.
    controls_inside: Vec<u8>,
    /// The rows the shell may use, or `None` while the bar is off.
    shell_rows: Option<u16>,
    bar_wiped: bool,
    screen_held: bool,
}

impl OutputFilter {
    /// Starts a filter for the rows in `layout`.
    pub fn new(layout: Layout) -> Self {
        Self {
            parser: Parser::new(),
            held: Vec::new(),
            controls_inside: Vec::new(),
            shell_rows: shell_rows(layout),
            bar_wiped: false,
            screen_held: false,
        }
    }

    /// Uses the rows in `layout` from now on, after the window changed size.
    pub fn set_layout(&mut self, layout: Layout) {
        self.shell_rows = shell_rows(layout);
    }

    /// Adds `bytes` to `output`, with codes that would reach the bar changed. The end of an
    /// unfinished code is held back until a later call.
    pub fn filter(&mut self, bytes: &[u8], output: &mut Vec<u8>) {
        for &byte in bytes {
            self.held.push(byte);
            let mut watcher = CodeWatcher {
                shell_rows: self.shell_rows,
                inside_code: self.held.len() > 1 && self.held.first() == Some(&ESCAPE),
                ending: None,
                control_inside: None,
                bar_wiped: false,
                screen_held: None,
            };
            self.parser
                .advance(&mut watcher, std::slice::from_ref(&byte));

            self.bar_wiped |= watcher.bar_wiped;
            if let Some(held) = watcher.screen_held {
                self.screen_held = held;
            }
            if let Some(control) = watcher.control_inside {
                self.controls_inside.push(control);
            }
            let Some(ending) = watcher.ending else {
                continue;
            };
            match ending {
                Ending::Unchanged => output.extend_from_slice(&self.held),
                Ending::Replaced(code) => {
                    output.extend_from_slice(&self.controls_inside);
                    output.extend_from_slice(code.as_bytes());
                }
                Ending::FollowedBy(code) => {
                    output.extend_from_slice(&self.held);
                    output.extend_from_slice(code.as_bytes());
                }
            }
            self.held.clear();
            self.controls_inside.clear();
        }
    }

    /// Whether the output may have wiped the bar since the last call, so it needs drawing
    /// again.
    pub fn take_bar_wiped(&mut self) -> bool {
        std::mem::take(&mut self.bar_wiped)
    }

    /// Whether a program is holding the screen still while it draws.
    pub fn screen_held(&self) -> bool {
        self.screen_held
    }
}

fn shell_rows(layout: Layout) -> Option<u16> {
    layout.first_bar_row().map(|_| layout.shell_rows)
}

/// What to send once a code has ended.
enum Ending {
    /// The code as it came.
    Unchanged,
    /// A different code instead.
    Replaced(String),
    /// The code as it came, then more codes.
    FollowedBy(String),
}

/// Hears from the parser about one byte, and decides what happens to the held code.
struct CodeWatcher {
    shell_rows: Option<u16>,
    /// Whether the byte came inside a code, after its escape byte.
    inside_code: bool,
    ending: Option<Ending>,
    control_inside: Option<u8>,
    bar_wiped: bool,
    screen_held: Option<bool>,
}

impl CodeWatcher {
    fn csi(&mut self, params: &Params, intermediates: &[u8], action: char) -> Ending {
        let Some(shell_rows) = self.shell_rows else {
            if let ([PRIVATE_MARKER], MODE_ON | MODE_OFF) = (intermediates, action) {
                self.watch_modes(params, action == MODE_ON);
            }
            return Ending::Unchanged;
        };
        match (intermediates, action) {
            ([], SCROLL_REGION) => {
                let top = number(params, 0).max(1);
                let bottom = match number(params, 1) {
                    0 => shell_rows,
                    bottom => bottom.min(shell_rows),
                };
                Ending::Replaced(format!("{CSI}{top};{bottom}{SCROLL_REGION}"))
            }
            ([], CURSOR_POSITION | CURSOR_POSITION_OLD) if number(params, 0) > shell_rows => {
                let column = number(params, 1);
                Ending::Replaced(format!("{CSI}{shell_rows};{column}{action}"))
            }
            ([], ROW_POSITION) if number(params, 0) > shell_rows => {
                Ending::Replaced(format!("{CSI}{shell_rows}{ROW_POSITION}"))
            }
            ([], ERASE_SCREEN) => {
                self.bar_wiped |= number(params, 0) != ERASE_ABOVE;
                Ending::Unchanged
            }
            ([PRIVATE_MARKER], MODE_ON | MODE_OFF) => {
                if self.watch_modes(params, action == MODE_ON) {
                    self.bar_wiped = true;
                    Ending::FollowedBy(limit_scrolling_in_place(shell_rows))
                } else {
                    Ending::Unchanged
                }
            }
            ([SOFT_RESET_MARKER], SOFT_RESET) => {
                Ending::FollowedBy(limit_scrolling_in_place(shell_rows))
            }
            _ => Ending::Unchanged,
        }
    }

    /// Notes modes being turned on or off. Returns whether the screen was switched.
    fn watch_modes(&mut self, params: &Params, on: bool) -> bool {
        let mut switched_screen = false;
        for mode in params.iter().filter_map(|param| param.first().copied()) {
            switched_screen |= OTHER_SCREEN_MODES.contains(&mode);
            if mode == SYNCHRONIZED_OUTPUT_MODE {
                self.screen_held = Some(on);
            }
        }
        switched_screen
    }
}

impl Perform for CodeWatcher {
    fn print(&mut self, _character: char) {
        self.ending = Some(Ending::Unchanged);
    }

    fn execute(&mut self, byte: u8) {
        // Inside a code, terminals act on most controls and carry on with the code. Cancel
        // and substitute end it instead.
        if self.inside_code && !matches!(byte, CANCEL | SUBSTITUTE) {
            self.control_inside = Some(byte);
        } else {
            self.ending = Some(Ending::Unchanged);
        }
    }

    fn hook(&mut self, _params: &Params, _intermediates: &[u8], _ignore: bool, _action: char) {
        self.ending = Some(Ending::Unchanged);
    }

    fn put(&mut self, _byte: u8) {
        // Pass long device strings on as they come, instead of holding all of them.
        self.ending = Some(Ending::Unchanged);
    }

    fn unhook(&mut self) {
        self.ending = Some(Ending::Unchanged);
    }

    fn osc_dispatch(&mut self, _params: &[&[u8]], _bell_terminated: bool) {
        self.ending = Some(Ending::Unchanged);
    }

    fn csi_dispatch(&mut self, params: &Params, intermediates: &[u8], ignore: bool, action: char) {
        let ending = if ignore {
            Ending::Unchanged
        } else {
            self.csi(params, intermediates, action)
        };
        self.ending = Some(ending);
    }

    fn esc_dispatch(&mut self, intermediates: &[u8], ignore: bool, byte: u8) {
        let full_reset = !ignore && intermediates.is_empty() && byte == FULL_RESET;
        let ending = match self.shell_rows {
            Some(shell_rows) if full_reset => {
                self.bar_wiped = true;
                // A reset already moved the cursor to the top left, so there is nothing to keep.
                Ending::FollowedBy(limit_scrolling(shell_rows))
            }
            _ => Ending::Unchanged,
        };
        self.ending = Some(ending);
    }
}

/// The number at `index` in a code. A missing number reads as 0, which terminals treat as
/// "use the default".
fn number(params: &Params, index: usize) -> u16 {
    params
        .iter()
        .nth(index)
        .and_then(|param| param.first().copied())
        .unwrap_or(0)
}

/// Limits scrolling again without moving the cursor.
///
/// Saving the cursor here replaces a save the program made itself. Programs that switch
/// screens save the cursor in the same place while switching, so this keeps the same spot.
fn limit_scrolling_in_place(shell_rows: u16) -> String {
    format!(
        "{SAVE_CURSOR}{}{RESTORE_CURSOR}",
        limit_scrolling(shell_rows)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::term::bar::{BAR_ROWS, MIN_SHELL_ROWS};

    /// A window tall enough for the bar.
    const WINDOW_ROWS: u16 = 24;

    /// The rows the shell may use in that window.
    const SHELL_ROWS: u16 = WINDOW_ROWS - BAR_ROWS;

    fn with_bar() -> OutputFilter {
        OutputFilter::new(Layout::new(WINDOW_ROWS))
    }

    /// Filters each chunk in turn and returns what came out of each.
    fn run(filter: &mut OutputFilter, chunks: &[&[u8]]) -> Vec<Vec<u8>> {
        chunks
            .iter()
            .map(|chunk| {
                let mut output = Vec::new();
                filter.filter(chunk, &mut output);
                output
            })
            .collect()
    }

    fn filtered(filter: &mut OutputFilter, input: &str) -> String {
        let mut output = Vec::new();
        filter.filter(input.as_bytes(), &mut output);
        String::from_utf8_lossy(&output).into_owned()
    }

    #[test]
    fn text_and_codes_that_cannot_reach_the_bar_pass_through_unchanged() {
        let inputs = [
            "plain text\r\nand a second line\r\n".to_owned(),
            format!("{CSI}1;31mred{CSI}0m {CSI}10;5H{CSI}5{ROW_POSITION}{CSI}K"),
            format!("{CSI}?25l{CSI}?25h{CSI}1{ERASE_SCREEN}"),
            "\x1b]0;window title\x07\x1b]7701;0\x07\x1b]8;;https://example.com\x1b\\".to_owned(),
            "\x1b_Ga=T,f=100;AAAA\x1b\\\x1bP+q544e\x1b\\".to_owned(),
            "snowman \u{2603} cat \u{1F408}".to_owned(),
            "\x1b(B\x1b=\x1b>\x07\x08\t".to_owned(),
        ];
        for input in inputs {
            assert_eq!(filtered(&mut with_bar(), &input), input);
        }
    }

    #[test]
    fn a_code_split_across_reads_is_held_until_it_ends() {
        let mut filter = with_bar();

        let outputs = run(&mut filter, &[b"ab\x1b[3", b"1", b"mcd"]);

        assert_eq!(
            outputs,
            [b"ab".to_vec(), Vec::new(), b"\x1b[31mcd".to_vec()]
        );
    }

    #[test]
    fn a_split_multibyte_character_comes_out_whole() {
        let cat = "\u{1F408}".as_bytes();
        let mut filter = with_bar();

        let outputs = run(&mut filter, &[&cat[..2], &cat[2..]]);

        assert_eq!(outputs, [Vec::new(), cat.to_vec()]);
    }

    #[test]
    fn scroll_regions_stop_above_the_bar() {
        let cases = [
            (
                format!("{CSI}{SCROLL_REGION}"),
                format!("{CSI}1;{SHELL_ROWS}{SCROLL_REGION}"),
            ),
            (
                format!("{CSI}5;40{SCROLL_REGION}"),
                format!("{CSI}5;{SHELL_ROWS}{SCROLL_REGION}"),
            ),
            (
                format!("{CSI}2;10{SCROLL_REGION}"),
                format!("{CSI}2;10{SCROLL_REGION}"),
            ),
        ];
        for (input, expected) in cases {
            assert_eq!(filtered(&mut with_bar(), &input), expected, "for {input:?}");
        }
    }

    #[test]
    fn a_split_scroll_region_is_still_changed() {
        let mut filter = with_bar();

        let outputs = run(&mut filter, &[b"\x1b", b"[", b"r"]);

        let expected = format!("{CSI}1;{SHELL_ROWS}{SCROLL_REGION}");
        assert_eq!(outputs, [Vec::new(), Vec::new(), expected.into_bytes()]);
    }

    #[test]
    fn cursor_moves_into_the_bar_stop_at_the_last_shell_row() {
        let cases = [
            (
                format!("{CSI}999;5{CURSOR_POSITION}"),
                format!("{CSI}{SHELL_ROWS};5{CURSOR_POSITION}"),
            ),
            (
                format!("{CSI}999{CURSOR_POSITION_OLD}"),
                format!("{CSI}{SHELL_ROWS};0{CURSOR_POSITION_OLD}"),
            ),
            (
                format!("{CSI}{WINDOW_ROWS}{ROW_POSITION}"),
                format!("{CSI}{SHELL_ROWS}{ROW_POSITION}"),
            ),
            (
                format!("{CSI}{SHELL_ROWS};1{CURSOR_POSITION}"),
                format!("{CSI}{SHELL_ROWS};1{CURSOR_POSITION}"),
            ),
        ];
        for (input, expected) in cases {
            assert_eq!(filtered(&mut with_bar(), &input), expected, "for {input:?}");
        }
    }

    #[test]
    fn a_control_inside_a_changed_code_is_kept() {
        let mut filter = with_bar();

        let output = filtered(&mut filter, &format!("{CSI}99\n9;1{CURSOR_POSITION}"));

        assert_eq!(output, format!("\n{CSI}{SHELL_ROWS};1{CURSOR_POSITION}"));
    }

    #[test]
    fn a_cancelled_code_passes_through_unchanged() {
        let input = format!("{CSI}99{}after", char::from(CANCEL));

        assert_eq!(filtered(&mut with_bar(), &input), input);
    }

    #[test]
    fn erasing_the_screen_notes_that_the_bar_may_be_wiped() {
        for (number, wipes) in [
            ("", true),
            ("0", true),
            ("2", true),
            ("3", true),
            ("1", false),
        ] {
            let mut filter = with_bar();
            let input = format!("{CSI}{number}{ERASE_SCREEN}");

            assert_eq!(filtered(&mut filter, &input), input);
            assert_eq!(filter.take_bar_wiped(), wipes, "for {input:?}");
            assert!(!filter.take_bar_wiped(), "reading should reset it");
        }
    }

    #[test]
    fn switching_screens_limits_scrolling_again_in_place() {
        for mode in OTHER_SCREEN_MODES {
            for end in [MODE_ON, MODE_OFF] {
                let mut filter = with_bar();
                let input = format!("{CSI}?{mode}{end}");

                let expected = format!(
                    "{input}{SAVE_CURSOR}{}{RESTORE_CURSOR}",
                    limit_scrolling(SHELL_ROWS)
                );
                assert_eq!(filtered(&mut filter, &input), expected);
                assert!(filter.take_bar_wiped());
            }
        }
    }

    #[test]
    fn resets_limit_scrolling_again() {
        let mut filter = with_bar();
        let full_reset = format!("\x1b{}", char::from(FULL_RESET));
        let expected = format!("{full_reset}{}", limit_scrolling(SHELL_ROWS));
        assert_eq!(filtered(&mut filter, &full_reset), expected);
        assert!(filter.take_bar_wiped());

        let soft_reset = format!("{CSI}{}{SOFT_RESET}", char::from(SOFT_RESET_MARKER));
        let expected = format!(
            "{soft_reset}{SAVE_CURSOR}{}{RESTORE_CURSOR}",
            limit_scrolling(SHELL_ROWS)
        );
        assert_eq!(filtered(&mut filter, &soft_reset), expected);
    }

    #[test]
    fn holding_the_screen_is_noted_even_with_other_modes() {
        let mut filter = with_bar();

        filtered(
            &mut filter,
            &format!("{CSI}?{SYNCHRONIZED_OUTPUT_MODE}{MODE_ON}"),
        );
        assert!(filter.screen_held());
        filtered(
            &mut filter,
            &format!("{CSI}?25;{SYNCHRONIZED_OUTPUT_MODE}{MODE_OFF}"),
        );
        assert!(!filter.screen_held());
    }

    #[test]
    fn without_a_bar_everything_passes_through_unchanged() {
        let mut filter = OutputFilter::new(Layout::new(MIN_SHELL_ROWS));
        let input = format!(
            "{CSI}{SCROLL_REGION}{CSI}999;1{CURSOR_POSITION}{CSI}?1049{MODE_ON}\x1b{}",
            char::from(FULL_RESET)
        );

        assert_eq!(filtered(&mut filter, &input), input);
        filtered(
            &mut filter,
            &format!("{CSI}?{SYNCHRONIZED_OUTPUT_MODE}{MODE_ON}"),
        );
        assert!(filter.screen_held());
    }

    #[test]
    fn a_new_layout_moves_the_limit() {
        let mut filter = with_bar();
        filter.set_layout(Layout::new(40));

        let output = filtered(&mut filter, &format!("{CSI}{SCROLL_REGION}"));

        assert_eq!(output, format!("{CSI}1;{}{SCROLL_REGION}", 40 - BAR_ROWS));
    }
}
