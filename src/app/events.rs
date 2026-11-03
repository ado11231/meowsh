//! The helper threads that read keys, read shell output, and watch signals.
//!
//! Each thread only listens. It hands what it hears to the main loop through the event
//! queue, so only the main loop ever writes to the terminal.

use super::catch_panic;
use crate::term::pty::forward_output;
use anyhow::{Context, Result};
use signal_hook::consts::SIGWINCH;
use signal_hook::iterator::{Handle, Signals};
use std::io::{ErrorKind, Read, Write};
use std::sync::mpsc::SyncSender;
use std::thread::{self, JoinHandle};

/// Messages from the helper threads to the main loop.
pub enum Event {
    /// Bytes the shell printed.
    Output(Vec<u8>),
    /// Keyboard input ended, or failed.
    InputFinished(Result<()>),
    /// Shell output ended, which ends the session.
    OutputFinished(Result<()>),
    /// The terminal window changed size.
    Resized,
    /// The signal thread failed.
    ResizeFailed(anyhow::Error),
    /// A shutdown signal such as SIGTERM arrived.
    Terminated(i32),
}

/// Sends shell output to the main loop as events.
struct OutputSender {
    events: SyncSender<Event>,
}

impl Write for OutputSender {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        // The bytes are copied because the read buffer is reused for the next read.
        self.events
            .send(Event::Output(buffer.to_vec()))
            .map_err(|_| std::io::Error::new(ErrorKind::BrokenPipe, "session has ended"))?;
        Ok(buffer.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        // The main loop flushes the terminal after each output event.
        Ok(())
    }
}

/// Stops the signal thread when it leaves scope.
pub struct ResizeGuard {
    signal_handle: Handle,
    thread: Option<JoinHandle<Result<()>>>,
}

impl ResizeGuard {
    /// Closes the signal listener and waits for the thread. Safe to call more than once.
    pub fn stop(&mut self) -> Result<()> {
        self.signal_handle.close();
        if let Some(thread) = self.thread.take() {
            thread
                .join()
                .map_err(|_| anyhow::anyhow!("resize thread panicked"))??;
        }
        Ok(())
    }
}

impl Drop for ResizeGuard {
    fn drop(&mut self) {
        // Cleanup must not panic while another error is being handled.
        let _ = self.stop();
    }
}

/// Sends window resizes and shutdown requests to the main loop.
pub fn start_resize_thread(mut signals: Signals, events: SyncSender<Event>) -> Result<ResizeGuard> {
    let signal_handle = signals.handle();
    let thread = thread::Builder::new()
        .name("terminal-resize".into())
        .spawn(move || {
            let result = catch_panic("resize thread panicked", || {
                for signal in signals.forever() {
                    if signal != SIGWINCH {
                        // Any other signal we watch means the session should stop. A closed
                        // queue means it already has.
                        let _ = events.send(Event::Terminated(signal));
                        break;
                    }
                    if events.send(Event::Resized).is_err() {
                        break;
                    }
                }
                Ok(())
            });
            if let Err(error) = result {
                // A closed queue means the session has already ended.
                let _ = events.send(Event::ResizeFailed(error));
            }
            Ok(())
        })
        .context("could not start resize thread")?;

    Ok(ResizeGuard {
        signal_handle,
        thread: Some(thread),
    })
}

/// Passes keys straight to the shell and reports when that ends.
pub fn start_input_thread(
    writer: impl Write + Send + 'static,
    events: SyncSender<Event>,
) -> Result<JoinHandle<()>> {
    thread::Builder::new()
        .name("keyboard-input".into())
        .spawn(move || {
            let result = catch_panic("keyboard input thread panicked", || {
                forward_input(std::io::stdin().lock(), writer)
            });
            // A closed queue means the session has already ended.
            let _ = events.send(Event::InputFinished(result));
        })
        .context("could not start keyboard input thread")
}

/// Reads shell output and sends it to the main loop.
pub fn start_output_thread(
    reader: impl Read + Send + 'static,
    events: SyncSender<Event>,
) -> Result<JoinHandle<()>> {
    thread::Builder::new()
        .name("shell-output".into())
        .spawn(move || {
            let mut sender = OutputSender { events };
            let result = catch_panic("shell output thread panicked", || {
                forward_output(reader, &mut sender)
            });
            // A closed queue means the session has already ended.
            let _ = sender.events.send(Event::OutputFinished(result));
        })
        .context("could not start shell output thread")
}

/// Copies keyboard input to the shell until the input stream ends.
pub fn forward_input(mut reader: impl Read, mut writer: impl Write) -> Result<()> {
    std::io::copy(&mut reader, &mut writer).context("could not forward keyboard input")?;
    Ok(())
}
