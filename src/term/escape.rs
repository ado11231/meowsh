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

/// The escape byte that starts every terminal code.
pub const ESCAPE: u8 = 0x1b;

/// Cancels a code that is half sent. The terminal drops what came before it.
pub const CANCEL: u8 = 0x18;

/// Cancels a code that is half sent, like `CANCEL`.
pub const SUBSTITUTE: u8 = 0x1a;

/// Ends `CSI row ; column f`, an older way to write `CURSOR_POSITION`.
pub const CURSOR_POSITION_OLD: char = 'f';

/// Ends `CSI row d`, which moves the cursor to a row and keeps its column.
pub const ROW_POSITION: char = 'd';

/// Ends `CSI number J`, which erases part of the screen. 0 or no number erases from the
/// cursor to the end, 1 from the start to the cursor, 2 everything, 3 also the scrollback.
pub const ERASE_SCREEN: char = 'J';

/// The `ERASE_SCREEN` number that erases from the start of the screen to the cursor only.
pub const ERASE_ABOVE: u16 = 1;

/// Marks a CSI code as private to the terminal, such as `CSI ? 1049 h`.
pub const PRIVATE_MARKER: u8 = b'?';

/// Ends `CSI ? number h`, which turns a terminal mode on.
pub const MODE_ON: char = 'h';

/// Ends `CSI ? number l`, which turns a terminal mode off.
pub const MODE_OFF: char = 'l';

/// Modes that switch to or from the second screen that full screen programs draw on.
pub const OTHER_SCREEN_MODES: [u16; 3] = [47, 1047, 1049];

/// The mode that holds the screen still while a program draws, so nothing shows half done.
pub const SYNCHRONIZED_OUTPUT_MODE: u16 = 2026;

/// Comes before `SOFT_RESET` in `CSI ! p`.
pub const SOFT_RESET_MARKER: u8 = b'!';

/// Ends `CSI ! p`, which puts most terminal settings back, such as the rows that scroll.
pub const SOFT_RESET: char = 'p';

/// Ends `ESC c`, which resets the terminal fully.
pub const FULL_RESET: u8 = b'c';

/// Starts a kitty picture code: `ESC _ G settings ; data ESC \`.
pub const PICTURE_START: &str = "\x1b_G";

/// Ends a kitty picture code, and any other code that is not ended by `BELL`.
pub const STRING_END: &str = "\x1b\\";

/// What a kitty picture reply says after its settings when the picture worked.
pub const PICTURE_OK: &str = "OK";

/// Asks the terminal what it is. Every terminal answers, so it marks the end of the replies
/// to any question sent before it.
pub const ASK_DEVICE_ATTRIBUTES: &str = "\x1b[c";

/// Ends the answer to `ASK_DEVICE_ATTRIBUTES`: `CSI ? numbers c`.
pub const DEVICE_ATTRIBUTES: u8 = b'c';

/// Starts the answer to `ASK_DEVICE_ATTRIBUTES`.
pub const DEVICE_ATTRIBUTES_START: &str = "\x1b[?";

/// Asks the terminal for its text color: `OSC 10 ; ? ST`.
pub const ASK_TEXT_COLOR: &str = "\x1b]10;?\x1b\\";

/// Starts the answer to `ASK_TEXT_COLOR`: `OSC 10 ; rgb:RRRR/GGGG/BBBB`, ended by
/// `STRING_END` or `BELL`.
pub const TEXT_COLOR_START: &str = "\x1b]10;";

/// Comes before the red, green & blue numbers in a color answer.
pub const RGB_COLOR_PREFIX: &str = "rgb:";

/// Splits the red, green & blue numbers in a color answer.
pub const RGB_COLOR_SPLIT: char = '/';

/// Holds the screen still until `RELEASE_SCREEN`, so a drawing shows all at once. It is
/// `CSI ? SYNCHRONIZED_OUTPUT_MODE h`.
pub const HOLD_SCREEN: &str = "\x1b[?2026h";

/// Lets the screen show again after `HOLD_SCREEN`.
pub const RELEASE_SCREEN: &str = "\x1b[?2026l";
