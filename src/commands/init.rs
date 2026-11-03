//! `meowsh init`: prints the startup script that `eval "$(meowsh init zsh)"` runs.

use crate::shell::{Shell, init_script};
use anyhow::{Context, Result};
use std::io::Write;

/// Prints the startup script for `shell`, set up to run this meowsh program.
pub fn run(shell: Shell) -> Result<()> {
    let executable = std::env::current_exe().context("could not find the meowsh program")?;
    let executable = executable
        .to_str()
        .context("meowsh program path is not valid UTF-8")?;
    std::io::stdout()
        .lock()
        .write_all(init_script(shell, executable).as_bytes())
        .context("could not print the startup script")
}
