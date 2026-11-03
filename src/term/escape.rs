//! Terminal codes, named so the code that sends or reads them reads in plain words.

/// The OSC number of the prompt mark. Meowsh's shell prints `ESC ] 7701 ; status BEL`
/// before each prompt, after a command ran. Terminals ignore OSC numbers they do not know.
pub const PROMPT_MARK: &str = "7701";

/// Starts an OSC sequence, such as a window title or the prompt mark.
pub const OSC: &str = "\x1b]";

/// Ends an OSC sequence.
pub const BELL: char = '\x07';
