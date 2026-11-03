//! Terminal codes, named so the code that sends or reads them reads in plain words.

/// The OSC number of the prompt mark. Meowsh's shell prints `ESC ] 7701 ; status BEL`
/// before each prompt, after a command ran. Terminals ignore OSC numbers they do not know.
pub const PROMPT_MARK: &str = "7701";

/// Starts an OSC sequence, such as a window title or the prompt mark.
pub const OSC: &str = "\x1b]";

/// Ends an OSC sequence.
pub const BELL: char = '\x07';

/// Starts a CSI sequence, such as a cursor move.
pub const CSI: &str = "\x1b[";

/// Saves the cursor position, so it can be put back with `RESTORE_CURSOR`.
pub const SAVE_CURSOR: &str = "\x1b7";

/// Puts the cursor back where `SAVE_CURSOR` saved it.
pub const RESTORE_CURSOR: &str = "\x1b8";

/// Ends `CSI top ; bottom r`, which sets the rows that scroll. With no numbers, the whole
/// window scrolls again. Either way the terminal moves the cursor to the top left.
pub const SCROLL_REGION: char = 'r';

/// Ends `CSI count A`, which moves the cursor up. It stops at the top row that scrolls.
pub const CURSOR_UP: char = 'A';

/// Ends `CSI row ; column H`, which moves the cursor to a cell. Both count from 1.
pub const CURSOR_POSITION: char = 'H';

/// Erases the whole line the cursor is on.
pub const ERASE_LINE: &str = "\x1b[2K";

/// Moves the cursor down a row, and scrolls when it is on the bottom row that scrolls. In
/// raw mode it stays in the same column.
pub const LINE_FEED: char = '\n';
