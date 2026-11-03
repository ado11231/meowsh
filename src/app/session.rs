//! The session: starts the shell in a PTY and runs the main loop that passes data along.

use super::events::{Event, start_input_thread, start_output_thread, start_resize_thread};
use super::{TerminationSignal, catch_panic};
use crate::env::MEOWSH_TTY;
use crate::term::pty::{ShellGuard, open_pty, resize_pty, spawn_shell};
use crate::term::raw;
use anyhow::{Context, Result};
use portable_pty::{CommandBuilder, MasterPty};
use signal_hook::iterator::Signals;
use std::io::{Read, Write};
use std::process::ExitCode;
use std::sync::mpsc::{Receiver, SyncSender, sync_channel};

/// How many events can wait in the queue before the helper threads pause.
const EVENT_QUEUE_SIZE: usize = 16;

/// Exit code used when the shell's own code does not fit in a byte.
const FALLBACK_EXIT_CODE: u8 = 1;

/// A running shell, with the terminal in raw mode until the session is dropped.
pub struct Session {
    shell: ShellGuard,
    master: Box<dyn MasterPty + Send>,
    reader: Box<dyn Read + Send>,
    writer: Box<dyn Write + Send>,
    signals: Signals,
    _raw_mode: raw::RawModeGuard,
}

impl Session {
    /// Starts the shell in a new PTY the same size as the real terminal.
    pub fn start(mut command: CommandBuilder) -> Result<Self> {
        // Watch signals before raw mode, so a shutdown signal also takes the cleanup path.
        let signals = raw::watch_signals()?;
        let size = raw::read_size()?;
        let pty = open_pty(size)?;
        let tty_name = pty
            .master
            .tty_name()
            .context("could not read shell PTY name")?;
        command.env(MEOWSH_TTY, tty_name);

        let reader = pty
            .master
            .try_clone_reader()
            .context("could not open PTY reader")?;
        let writer = pty
            .master
            .take_writer()
            .context("could not open PTY writer")?;

        let raw_mode = raw::RawModeGuard::enable()?;
        let shell = ShellGuard {
            child: spawn_shell(&pty, command)?,
        };
        // Only the shell should hold its side of the PTY, so output ends when the shell exits.
        drop(pty.slave);

        Ok(Self {
            shell,
            master: pty.master,
            reader,
            writer,
            signals,
            _raw_mode: raw_mode,
        })
    }

    /// Runs the session until the shell exits, and returns the shell's exit code.
    pub fn run(mut self) -> Result<ExitCode> {
        let (sender, receiver) = sync_channel(EVENT_QUEUE_SIZE);
        // Each thread needs its own sender. Cloning a sender does not copy any events.
        let mut resize_guard = start_resize_thread(self.signals, sender.clone())?;
        let master = self.master;

        forward_session(
            self.reader,
            self.writer,
            sender,
            receiver,
            std::io::stdout(),
            |_| resize_pty(master.as_ref(), raw::read_size()?),
        )?;

        let status = self
            .shell
            .child
            .wait()
            .context("could not wait for shell")?;
        resize_guard.stop()?;

        let exit_code = u8::try_from(status.exit_code()).unwrap_or(FALLBACK_EXIT_CODE);
        Ok(ExitCode::from(exit_code))
    }
}

/// Starts the input and output threads, then runs the main loop.
fn forward_session<W: Write>(
    reader: impl Read + Send + 'static,
    writer: impl Write + Send + 'static,
    sender: SyncSender<Event>,
    receiver: Receiver<Event>,
    output: W,
    resize: impl FnMut(&mut W) -> Result<()>,
) -> Result<()> {
    // The input thread may stay blocked on stdin, so it is not joined.
    let _input_thread = start_input_thread(writer, sender.clone())?;
    let output_thread = start_output_thread(reader, sender)?;
    // Owning the receiver here closes the queue before the resize cleanup, on any error.
    forward_events(receiver, output, resize)?;
    output_thread
        .join()
        .map_err(|_| anyhow::anyhow!("shell output thread panicked"))
}

/// The main loop: handles events until shell output ends or a helper thread fails.
pub fn forward_events<W: Write>(
    receiver: Receiver<Event>,
    mut output: W,
    mut resize: impl FnMut(&mut W) -> Result<()>,
) -> Result<()> {
    loop {
        let event = receiver
            .recv()
            .context("session event queue closed unexpectedly")?;
        match event {
            Event::Output(bytes) => {
                output
                    .write_all(&bytes)
                    .context("could not write shell output")?;
                output.flush().context("could not flush shell output")?;
            }
            Event::InputFinished(result) => result?,
            Event::OutputFinished(result) => return result,
            Event::Resized => {
                catch_panic("resize handler panicked", || resize(&mut output))?;
            }
            Event::ResizeFailed(error) => return Err(error),
            Event::Terminated(signal) => return Err(TerminationSignal(signal).into()),
        }
    }
}
