//! The startup scripts that `meowsh init` prints for bash and zsh.
//!
//! In a new window, the script swaps the shell for Meowsh. Inside Meowsh's own shell, it
//! adds the prompt mark instead.

use crate::term::escape::PROMPT_MARK;

/// Replaced with the quoted path to the meowsh program.
const EXECUTABLE_PLACEHOLDER: &str = "@MEOWSH_EXECUTABLE@";

/// Replaced with the OSC number of the prompt mark.
const PROMPT_MARK_PLACEHOLDER: &str = "@MEOWSH_PROMPT_MARK@";

/// The shells Meowsh can start from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum Shell {
    /// zsh, the default shell on macOS.
    Zsh,
    /// bash, the default shell on most Linux systems.
    Bash,
}

impl Shell {
    /// The shell's name, as typed after `meowsh init`.
    pub fn name(self) -> &'static str {
        match self {
            Self::Zsh => "zsh",
            Self::Bash => "bash",
        }
    }
}

/// Returns the startup script for `shell`, set up to run `executable`.
pub fn init_script(shell: Shell, executable: &str) -> String {
    let script = match shell {
        Shell::Bash => include_str!("shell/bash.sh"),
        Shell::Zsh => include_str!("shell/zsh.sh"),
    };
    script
        .replace(EXECUTABLE_PLACEHOLDER, &quote(executable))
        .replace(PROMPT_MARK_PLACEHOLDER, PROMPT_MARK)
}

/// Wraps a value in single quotes, so the shell reads it exactly as written.
fn quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}
