//! Opens the PTY and starts the shell in it, or starts the shell directly without Meowsh.

use super::raw;
use crate::env::MEOWSH_TTY;
use anyhow::{Context, Result};
use portable_pty::{Child, CommandBuilder, MasterPty, PtyPair, PtySize, native_pty_system};
use std::ffi::OsStr;
use std::io::{ErrorKind, Read, Write};
use std::os::unix::process::CommandExt;
use std::process::{Command, ExitCode};

/// Interactive mode lets the shell show a prompt and accept typed commands.
const INTERACTIVE_FLAG: &str = "-i";

/// Login mode loads startup files such as zsh's .zprofile.
const LOGIN_FLAG: &str = "-l";

/// How many bytes of shell output are read at a time.
const OUTPUT_BUFFER_SIZE: usize = 8192;

/// Stops and collects the shell when it leaves scope.
pub struct ShellGuard {
    /// The running shell.
    pub child: Box<dyn Child + Send + Sync>,
}

impl Drop for ShellGuard {
    fn drop(&mut self) {
        // A shell that already finished does not need further cleanup.
        if let Ok(Some(_)) = self.child.try_wait() {
            return;
        }
        // Stop the shell, then collect its exit status so it does not linger. Cleanup has
        // nowhere to report a failure.
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Prepares the shell command for the PTY without starting a process.
pub fn build_shell_command(shell_path: &OsStr, login: bool) -> CommandBuilder {
    let mut command = CommandBuilder::new(shell_path);
    command.arg(INTERACTIVE_FLAG);
    if login {
        command.arg(LOGIN_FLAG);
    }
    command
}

/// Replaces Meowsh with the shell on the existing terminal. Returns only on failure.
pub fn run_shell_directly(shell_path: &OsStr, login: bool) -> Result<ExitCode> {
    let error = build_direct_shell_command(shell_path, login).exec();
    Err(error).context("could not start shell directly")
}

fn build_direct_shell_command(shell_path: &OsStr, login: bool) -> Command {
    let mut command = Command::new(shell_path);
    command.arg(INTERACTIVE_FLAG);
    if login {
        command.arg(LOGIN_FLAG);
    }
    // Mark this terminal, so the startup script does not start Meowsh again here.
    match raw::tty_name() {
        Some(tty_name) => command.env(MEOWSH_TTY, tty_name),
        None => command.env_remove(MEOWSH_TTY),
    };
    command
}

/// Opens the terminal pair that Meowsh and the shell will use.
pub fn open_pty(size: PtySize) -> Result<PtyPair> {
    native_pty_system()
        .openpty(size)
        .context("could not open PTY")
}

/// Updates the shell's terminal size and tells the program in front.
pub fn resize_pty(master: &dyn MasterPty, size: PtySize) -> Result<()> {
    master.resize(size).context("could not resize PTY")
}

/// Starts the shell on the shell's side of the PTY.
pub fn spawn_shell(pty: &PtyPair, command: CommandBuilder) -> Result<Box<dyn Child + Send + Sync>> {
    pty.slave
        .spawn_command(command)
        .context("could not start shell")
}

/// Copies shell output to `writer` until the stream ends.
pub fn forward_output(mut reader: impl Read, mut writer: impl Write) -> Result<()> {
    let mut buffer = [0u8; OUTPUT_BUFFER_SIZE];
    loop {
        let count = match reader.read(&mut buffer) {
            Ok(0) => return Ok(()),
            Ok(count) => count,
            // A signal can interrupt a read, so try again.
            Err(error) if error.kind() == ErrorKind::Interrupted => continue,
            Err(error) => return Err(error).context("could not read shell output"),
        };
        writer
            .write_all(&buffer[..count])
            .context("could not write shell output")?;
        // Show prompts right away, even when no newline is present.
        writer.flush().context("could not flush shell output")?;
    }
}
