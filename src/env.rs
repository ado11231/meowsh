//! Names of the environment variables Meowsh reads & sets.

use anyhow::{Context, Result};
use std::ffi::OsString;

/// The user's login shell.
pub const SHELL: &str = "SHELL";

/// The user's home folder.
pub const HOME: &str = "HOME";

/// Where zsh looks for its settings, when it is not the home folder.
pub const ZDOTDIR: &str = "ZDOTDIR";

/// Set when running inside tmux.
pub const TMUX: &str = "TMUX";

/// Names the terminal program, in terminals that set it.
pub const TERM_PROGRAM: &str = "TERM_PROGRAM";

/// The `TERM_PROGRAM` of the terminal in VS Code.
pub const VSCODE_TERM_PROGRAM: &str = "vscode";

/// Holds the name of the PTY the shell runs in. The startup script skips Meowsh when this
/// matches the shell's own terminal, so Meowsh never starts inside itself.
pub const MEOWSH_TTY: &str = "MEOWSH_TTY";

/// For testing: the time the cat's clock starts at, such as `06:55`.
pub const MEOWSH_CLOCK: &str = "MEOWSH_CLOCK";

/// For testing: how many times faster than real time the cat's clock runs, such as `60`.
pub const MEOWSH_CLOCK_SPEED: &str = "MEOWSH_CLOCK_SPEED";

/// Reads the user's shell path.
pub fn shell() -> Result<OsString> {
    std::env::var_os(SHELL).context("SHELL is not set")
}

/// Reads the user's home folder.
pub fn home() -> Result<OsString> {
    std::env::var_os(HOME).context("HOME is not set")
}
