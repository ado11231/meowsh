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

#[cfg(test)]
mod tests {
    use super::super::session::{draw_nothing, forward_events, keep_layout, test_filter};
    use super::*;
    use crate::term::raw;
    use crate::term::test_terminal::{run_in_test_terminal, test_name};
    use std::io;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::mpsc::{Receiver, TryRecvError, sync_channel};
    use std::time::Duration;

    const EVENT_TIMEOUT: Duration = Duration::from_secs(5);

    /// A signal listener with no signals, which keeps a test apart from signal tests.
    fn quiet_signals() -> Result<Signals> {
        Ok(Signals::new([] as [i32; 0])?)
    }

    fn next_event(receiver: &Receiver<Event>) -> Result<Event> {
        receiver
            .recv_timeout(EVENT_TIMEOUT)
            .context("thread should report an event")
    }

    fn join(thread: JoinHandle<()>) -> Result<()> {
        thread
            .join()
            .map_err(|_| anyhow::anyhow!("helper thread panicked"))
    }

    #[test]
    fn output_panic_reports_failure_while_other_senders_stay_open() -> Result<()> {
        struct PanickingReader;
        impl Read for PanickingReader {
            fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
                panic!("test output panic");
            }
        }
        let (sender, receiver) = sync_channel(1);
        // Keep a sender of our own, so the queue stays open after the thread ends.
        let thread = start_output_thread(PanickingReader, sender.clone())?;

        match next_event(&receiver)? {
            Event::OutputFinished(Err(error)) => {
                assert_eq!(error.to_string(), "shell output thread panicked");
            }
            _ => anyhow::bail!("expected an output panic error"),
        }
        join(thread)?;
        assert!(matches!(receiver.try_recv(), Err(TryRecvError::Empty)));
        drop(sender);
        Ok(())
    }

    #[test]
    fn output_thread_keeps_bytes_before_reporting_the_end() -> Result<()> {
        let expected: Vec<u8> = (0..=255).cycle().take(25_000).collect();
        // The thread needs its own copy, because it may outlive this function.
        let reader = io::Cursor::new(expected.clone());
        let (sender, receiver) = sync_channel(1);
        let thread = start_output_thread(reader, sender)?;
        let mut received = Vec::new();

        loop {
            match next_event(&receiver)? {
                Event::Output(bytes) => received.extend_from_slice(&bytes),
                Event::OutputFinished(result) => {
                    result?;
                    break;
                }
                _ => anyhow::bail!("output thread sent an unexpected event"),
            }
        }

        join(thread)?;
        assert_eq!(received, expected);
        assert!(matches!(
            receiver.try_recv(),
            Err(TryRecvError::Disconnected)
        ));
        Ok(())
    }

    #[test]
    fn output_thread_reports_read_errors() -> Result<()> {
        struct FailingReader;
        impl Read for FailingReader {
            fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
                Err(io::Error::new(
                    ErrorKind::PermissionDenied,
                    "test read failure",
                ))
            }
        }

        let (sender, receiver) = sync_channel(1);
        let thread = start_output_thread(FailingReader, sender)?;

        match next_event(&receiver)? {
            Event::OutputFinished(Err(error)) => {
                assert_eq!(error.to_string(), "could not read shell output");
                assert_eq!(
                    error.downcast_ref::<io::Error>().map(io::Error::kind),
                    Some(ErrorKind::PermissionDenied)
                );
            }
            _ => anyhow::bail!("expected a failed output event"),
        }
        join(thread)
    }

    #[test]
    fn output_sender_reports_a_closed_receiver() {
        let (sender, receiver) = sync_channel(1);
        let mut writer = OutputSender { events: sender };
        drop(receiver);

        let result = writer.write(b"shell> ");

        assert!(matches!(
            result,
            Err(error) if error.kind() == ErrorKind::BrokenPipe
                && error.to_string() == "session has ended"
        ));
    }

    #[test]
    fn output_thread_exits_when_the_receiver_closes() -> Result<()> {
        let (sender, receiver) = sync_channel(1);
        let reader = io::Cursor::new(vec![b'x'; 25_000]);
        let thread = start_output_thread(reader, sender)?;
        next_event(&receiver)?;

        drop(receiver);

        join(thread)
    }

    #[test]
    fn resize_thread_stops_without_a_signal_and_can_stop_twice() -> Result<()> {
        let signals = quiet_signals()?;
        let signal_handle = signals.handle();
        let (sender, receiver) = sync_channel(1);
        let mut guard = start_resize_thread(signals, sender)?;

        guard.stop()?;

        assert!(signal_handle.is_closed());
        assert!(guard.thread.is_none());
        guard.stop()?;
        assert!(matches!(
            receiver.try_recv(),
            Err(TryRecvError::Disconnected)
        ));
        Ok(())
    }

    #[test]
    fn resize_signal_reaches_the_main_loop() -> Result<()> {
        if run_in_test_terminal(test_name!(resize_signal_reaches_the_main_loop))? {
            return Ok(());
        }
        let signals = raw::watch_signals()?;
        let (sender, receiver) = sync_channel(1);
        let mut guard = start_resize_thread(signals, sender)?;
        signal_hook::low_level::raise(SIGWINCH)?;

        let event = next_event(&receiver)?;

        assert!(matches!(event, Event::Resized));
        guard.stop()
    }

    #[test]
    fn closing_the_session_releases_a_resize_thread_with_a_full_queue() -> Result<()> {
        if run_in_test_terminal(test_name!(
            closing_the_session_releases_a_resize_thread_with_a_full_queue
        ))? {
            return Ok(());
        }
        let signals = raw::watch_signals()?;
        let (sender, receiver) = sync_channel(1);
        sender.send(Event::OutputFinished(Ok(())))?;
        signal_hook::low_level::raise(SIGWINCH)?;
        let mut guard = start_resize_thread(signals, sender)?;

        forward_events(
            receiver,
            io::sink(),
            test_filter(),
            keep_layout,
            draw_nothing,
        )?;
        guard.stop()?;

        assert!(guard.thread.is_none());
        assert!(guard.signal_handle.is_closed());
        Ok(())
    }

    #[test]
    fn resize_guard_drop_closes_listener_and_waits_for_thread() -> Result<()> {
        let mut signals = quiet_signals()?;
        let signal_handle = signals.handle();
        let finished = Arc::new(AtomicBool::new(false));
        // The thread sets the flag, and the test reads it afterwards.
        let thread_finished = Arc::clone(&finished);
        let thread = thread::spawn(move || {
            for _ in signals.forever() {}
            thread_finished.store(true, Ordering::SeqCst);
            Ok(())
        });
        let guard = ResizeGuard {
            // The guard closes the listener, and the test checks that it did.
            signal_handle: signal_handle.clone(),
            thread: Some(thread),
        };

        drop(guard);

        assert!(signal_handle.is_closed());
        assert!(finished.load(Ordering::SeqCst));
        Ok(())
    }

    #[test]
    fn resize_guard_stop_returns_thread_error() -> Result<()> {
        let thread =
            thread::spawn(|| Err(io::Error::new(ErrorKind::BrokenPipe, "PTY closed").into()));
        let mut guard = ResizeGuard {
            signal_handle: quiet_signals()?.handle(),
            thread: Some(thread),
        };

        let error = guard
            .stop()
            .err()
            .context("thread error should reach the caller")?;

        assert_eq!(
            error.downcast_ref::<io::Error>().map(io::Error::kind),
            Some(ErrorKind::BrokenPipe)
        );
        assert!(guard.signal_handle.is_closed());
        assert!(guard.thread.is_none());
        guard.stop()
    }

    #[test]
    fn resize_guard_stop_reports_thread_panic() -> Result<()> {
        let thread = thread::spawn(|| panic!("test resize thread panic"));
        let mut guard = ResizeGuard {
            signal_handle: quiet_signals()?.handle(),
            thread: Some(thread),
        };

        let error = guard
            .stop()
            .err()
            .context("thread panic should become an error")?;

        assert_eq!(error.to_string(), "resize thread panicked");
        assert!(guard.thread.is_none());
        guard.stop()
    }

    #[test]
    fn input_keeps_text_and_control_keys() -> Result<()> {
        // Enter, Tab, Backspace, Ctrl C, and an arrow key.
        let input = b"echo hello\r\t\x7f\x03\x1b[A";
        let mut output = Vec::new();

        forward_input(&input[..], &mut output)?;

        assert_eq!(output, input);
        Ok(())
    }

    #[test]
    fn input_reports_a_write_failure() -> Result<()> {
        // A fixed slice runs out of space instead of growing like a Vec.
        let mut output = [0u8; 2];

        let error = forward_input(&b"echo hello\r"[..], &mut output[..])
            .err()
            .context("writing past the available space should fail")?;

        assert_eq!(error.to_string(), "could not forward keyboard input");
        assert_eq!(
            error.downcast_ref::<io::Error>().map(io::Error::kind),
            Some(ErrorKind::WriteZero)
        );
        Ok(())
    }
}
