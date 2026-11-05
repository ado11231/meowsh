//! The session: starts the shell in a PTY and runs the main loop that passes data along.

use super::events::{Event, start_input_thread, start_output_thread, start_resize_thread};
use super::{TerminationSignal, catch_panic};
use crate::art::placeholder::{Still, placeholder};
use crate::env::{MEOWSH_TTY, TERM_PROGRAM};
use crate::term::bar::{BarGuard, Layout};
use crate::term::filter::OutputFilter;
use crate::term::painter::{BarArea, BarPicture, DrawingWay, Painter};
use crate::term::picture::Pixels;
use crate::term::pty::{ShellGuard, open_pty, resize_pty, spawn_shell};
use crate::term::raw;
use anyhow::{Context, Result};
use portable_pty::{CommandBuilder, MasterPty};
use signal_hook::iterator::Signals;
use std::io::{Read, Write};
use std::process::ExitCode;
use std::sync::mpsc::{Receiver, RecvTimeoutError, SyncSender, sync_channel};
use std::time::Duration;

/// How many events can wait in the queue before the helper threads pause.
const EVENT_QUEUE_SIZE: usize = 16;

/// Exit code used when the shell's own code does not fit in a byte.
const FALLBACK_EXIT_CODE: u8 = 1;

/// How long the shell's output must stop before the bar is drawn, so a drawing never cuts
/// into the output.
const DRAW_PAUSE: Duration = Duration::from_millis(30);

/// The error when every helper thread is gone before the shell's output ended.
const QUEUE_CLOSED: &str = "session event queue closed unexpectedly";

/// The placeholder's `content` number. It never changes, because the placeholder never does.
const PLACEHOLDER_CONTENT: u64 = 0;

/// How far from the left end of the bar the placeholder starts, in its own pixels.
const PLACEHOLDER_X: u32 = 0;

/// A running shell, with the terminal in raw mode until the session is dropped.
pub struct Session {
    shell: ShellGuard,
    master: Box<dyn MasterPty + Send>,
    reader: Box<dyn Read + Send>,
    writer: Box<dyn Write + Send>,
    signals: Signals,
    // Dropped before raw mode ends, so the bar is given back in the same mode it was kept.
    bar: BarGuard,
    _raw_mode: raw::RawModeGuard,
}

impl Session {
    /// Keeps the bar rows, then starts the shell in a new PTY the size of the rows above.
    pub fn start(mut command: CommandBuilder) -> Result<Self> {
        // Watch signals before raw mode, so a shutdown signal also takes the cleanup path.
        let signals = raw::watch_signals()?;
        let size = raw::read_size()?;
        let layout = Layout::new(size.rows);
        let pty = open_pty(layout.shell_size(size))?;
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
        let bar = BarGuard::enter(layout, &mut std::io::stdout())?;
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
            bar,
            _raw_mode: raw_mode,
        })
    }

    /// Sends keys typed before the session started to the shell, ahead of any new keys.
    pub fn pass_on_early_input(&mut self, early_input: &[u8]) -> Result<()> {
        if early_input.is_empty() {
            return Ok(());
        }
        self.writer
            .write_all(early_input)
            .and_then(|()| self.writer.flush())
            .context("could not pass early keys to the shell")
    }

    /// Runs the session until the shell exits, and returns the shell's exit code.
    pub fn run(mut self) -> Result<ExitCode> {
        let (sender, receiver) = sync_channel(EVENT_QUEUE_SIZE);
        // Each thread needs its own sender. Cloning a sender does not copy any events.
        let mut resize_guard = start_resize_thread(self.signals, sender.clone())?;
        let master = self.master;
        let mut bar = self.bar;
        let filter = OutputFilter::new(bar.layout());
        let way = DrawingWay::for_terminal(std::env::var_os(TERM_PROGRAM).as_deref());
        let mut painter = Painter::new(way);
        let still = placeholder();
        let pictures = [placeholder_picture(&still)];

        let finished = forward_session(
            self.reader,
            self.writer,
            sender,
            receiver,
            std::io::stdout(),
            filter,
            |output| {
                let size = raw::read_size()?;
                let layout = Layout::new(size.rows);
                // Share the rows before the shell hears of the new size, so its redraw
                // lands above the bar.
                bar.resize(layout, output)?;
                resize_pty(master.as_ref(), layout.shell_size(size))?;
                Ok(layout)
            },
            |output, wiped| {
                let size = raw::read_size()?;
                let area = BarArea::new(Layout::new(size.rows), size);
                if wiped {
                    painter.bar_wiped();
                }
                let mut codes = String::new();
                painter.paint(&pictures, area, &mut codes)?;
                write_drawing(output, &codes)
            },
        );
        // In kitty pictures float above the text, so they would stay on screen after Meowsh
        // ends. The bar is not there anymore, so nothing new is drawn.
        let mut codes = String::new();
        let removed = painter
            .paint(&[], None, &mut codes)
            .and_then(|()| write_drawing(&mut std::io::stdout(), &codes));
        finished?;
        removed?;

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

/// The placeholder as a picture for the bar.
fn placeholder_picture(still: &Still) -> BarPicture<'_> {
    BarPicture {
        content: PLACEHOLDER_CONTENT,
        pixels: Pixels {
            width: still.width,
            height: still.height,
            rgba: &still.rgba,
        },
        x: PLACEHOLDER_X,
    }
}

/// Sends the painter's codes to the terminal, if it made any.
fn write_drawing(output: &mut impl Write, codes: &str) -> Result<()> {
    if codes.is_empty() {
        return Ok(());
    }
    output
        .write_all(codes.as_bytes())
        .and_then(|()| output.flush())
        .context("could not draw the bar")
}

/// Starts the input and output threads, then runs the main loop.
// Each argument is handed straight on, so a struct to hold them would add nothing.
#[allow(clippy::too_many_arguments)]
fn forward_session<W: Write>(
    reader: impl Read + Send + 'static,
    writer: impl Write + Send + 'static,
    sender: SyncSender<Event>,
    receiver: Receiver<Event>,
    output: W,
    filter: OutputFilter,
    resize: impl FnMut(&mut W) -> Result<Layout>,
    draw: impl FnMut(&mut W, bool) -> Result<()>,
) -> Result<()> {
    // The input thread may stay blocked on stdin, so it is not joined.
    let _input_thread = start_input_thread(writer, sender.clone())?;
    let output_thread = start_output_thread(reader, sender)?;
    // Owning the receiver here closes the queue before the resize cleanup, on any error.
    forward_events(receiver, output, filter, resize, draw)?;
    output_thread
        .join()
        .map_err(|_| anyhow::anyhow!("shell output thread panicked"))
}

/// The main loop: handles events until shell output ends or a helper thread fails.
///
/// Shell output passes through `filter` on its way out. `resize` shares the rows again after
/// the window changed size and returns the new layout. `draw` draws the bar & is told whether
/// the bar may have been wiped. It is called at the start, after a wipe & after a resize, once
/// the output pauses.
pub fn forward_events<W: Write>(
    receiver: Receiver<Event>,
    mut output: W,
    mut filter: OutputFilter,
    mut resize: impl FnMut(&mut W) -> Result<Layout>,
    mut draw: impl FnMut(&mut W, bool) -> Result<()>,
) -> Result<()> {
    // Kept between events, so it is not allocated again for every piece of output.
    let mut filtered = Vec::new();
    // The bar starts empty, so it is drawn at the first pause.
    let mut draw_waiting = true;
    let mut wiped = false;
    loop {
        let Some(event) = next_event(&receiver, draw_waiting)? else {
            // A program holding the screen is still drawing, so the bar waits until it lets go.
            if !filter.screen_held() {
                catch_panic("draw handler panicked", || draw(&mut output, wiped))?;
                draw_waiting = false;
                wiped = false;
            }
            continue;
        };
        match event {
            Event::Output(bytes) => {
                filtered.clear();
                filter.filter(&bytes, &mut filtered);
                output
                    .write_all(&filtered)
                    .context("could not write shell output")?;
                output.flush().context("could not flush shell output")?;
                if filter.take_bar_wiped() {
                    wiped = true;
                    draw_waiting = true;
                }
            }
            Event::InputFinished(result) => result?,
            Event::OutputFinished(result) => return result,
            Event::Resized => {
                let layout = catch_panic("resize handler panicked", || resize(&mut output))?;
                filter.set_layout(layout);
                // Sharing the rows again erases the bar.
                wiped = true;
                draw_waiting = true;
            }
            Event::ResizeFailed(error) => return Err(error),
            Event::Terminated(signal) => return Err(TerminationSignal(signal).into()),
        }
    }
}

/// Waits for the next event. While a drawing is waiting, gives up after `DRAW_PAUSE` with no
/// event & returns `None`.
fn next_event(receiver: &Receiver<Event>, draw_waiting: bool) -> Result<Option<Event>> {
    if !draw_waiting {
        return receiver.recv().map(Some).context(QUEUE_CLOSED);
    }
    match receiver.recv_timeout(DRAW_PAUSE) {
        Ok(event) => Ok(Some(event)),
        Err(RecvTimeoutError::Timeout) => Ok(None),
        Err(RecvTimeoutError::Disconnected) => Err(anyhow::anyhow!(QUEUE_CLOSED)),
    }
}

/// How tall the main loop tests pretend the window is.
#[cfg(test)]
const TEST_WINDOW_ROWS: u16 = 24;

/// A filter for the main loop tests, with the bar on.
#[cfg(test)]
pub fn test_filter() -> OutputFilter {
    OutputFilter::new(Layout::new(TEST_WINDOW_ROWS))
}

/// A resize handler for the main loop tests that keeps the window the same size.
#[cfg(test)]
pub fn keep_layout<W>(_output: &mut W) -> Result<Layout> {
    Ok(Layout::new(TEST_WINDOW_ROWS))
}

/// A draw handler for the main loop tests that draws nothing.
#[cfg(test)]
pub fn draw_nothing<W>(_output: &mut W, _wiped: bool) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::term::bar::{BAR_ROWS, limit_scrolling};
    use crate::term::escape::{CSI, ERASE_SCREEN, HOLD_SCREEN, RELEASE_SCREEN, SCROLL_REGION};
    use crate::term::pty::forward_output;
    use crate::term::test_terminal::{run_in_test_terminal, terminal_settings, test_name};
    use portable_pty::PtySize;
    use signal_hook::consts::{SIGHUP, SIGINT, SIGQUIT, SIGTERM};
    use std::cell::RefCell;
    use std::ffi::OsStr;
    use std::io::{self, BufWriter, ErrorKind};
    use std::process::{Command, Stdio};
    use std::thread;

    /// Prints the mark, then the real terminal name, so the two can be compared.
    const PRINT_MARK_AND_TTY: &str = "printf '%s\\n' \"$MEOWSH_TTY\"; /usr/bin/tty";

    /// Output that arrives after the session should have stopped.
    const TOO_LATE: &[u8] = b"too late";

    fn shell_command(script: &str) -> CommandBuilder {
        let mut command = CommandBuilder::new("/bin/sh");
        command.args(["-c", script]);
        command
    }

    fn raw_mode_enabled() -> Result<bool> {
        Ok(crossterm::terminal::is_raw_mode_enabled()?)
    }

    /// Checks that the main loop closed its queue, so no thread can send more events.
    fn queue_is_closed(sender: &SyncSender<Event>) -> bool {
        sender.send(Event::Output(Vec::new())).is_err()
    }

    #[test]
    fn session_mark_matches_the_inner_terminal_and_replaces_a_stale_mark() -> Result<()> {
        if run_in_test_terminal(test_name!(
            session_mark_matches_the_inner_terminal_and_replaces_a_stale_mark
        ))? {
            return Ok(());
        }
        let outer_terminal = raw::tty_name().context("test should have a terminal")?;
        let mut command = shell_command(PRINT_MARK_AND_TTY);
        command.env(MEOWSH_TTY, "/dev/stale-terminal");
        let mut session = Session::start(command)?;
        let inner_terminal = session
            .master
            .tty_name()
            .context("PTY should have a name")?;
        let mut output = Vec::new();

        forward_output(&mut session.reader, &mut output)?;
        let status = session.shell.child.wait()?;
        drop(session);

        assert!(status.success());
        assert_ne!(inner_terminal.as_os_str(), outer_terminal);
        let text = String::from_utf8(output)?;
        let lines: Vec<_> = text.lines().map(OsStr::new).collect();
        assert_eq!(lines, vec![inner_terminal.as_os_str(); 2]);
        assert_eq!(raw::tty_name(), Some(outer_terminal));
        Ok(())
    }

    #[test]
    fn typed_ctrl_c_reaches_the_inner_shell() -> Result<()> {
        if run_in_test_terminal(test_name!(typed_ctrl_c_reaches_the_inner_shell))? {
            return Ok(());
        }
        let before = terminal_settings()?;
        let command = shell_command("trap 'exit 7' INT; printf READY; read value");
        let mut session = Session::start(command)?;
        let mut ready = [0u8; 5];
        session.reader.read_exact(&mut ready)?;
        assert_eq!(&ready, b"READY");

        session.writer.write_all(b"\x03")?;
        session.writer.flush()?;
        let status = session.run()?;

        assert_eq!(status, ExitCode::from(7));
        assert!(!raw_mode_enabled()?);
        assert_eq!(terminal_settings()?, before);
        Ok(())
    }

    #[test]
    fn termination_signals_stop_the_shell_and_restore_the_terminal() -> Result<()> {
        if run_in_test_terminal(test_name!(
            termination_signals_stop_the_shell_and_restore_the_terminal
        ))? {
            return Ok(());
        }
        let before = terminal_settings()?;

        for signal in [SIGHUP, SIGINT, SIGQUIT, SIGTERM] {
            let session = Session::start(shell_command("read value"))?;
            let shell_id = session
                .shell
                .child
                .process_id()
                .context("shell should have a process ID")?;
            assert!(raw_mode_enabled()?);

            // Queue the signal after it is watched, before the main loop starts.
            signal_hook::low_level::raise(signal)?;
            let error = session
                .run()
                .err()
                .context("the signal should stop the session")?;

            assert_eq!(
                error.downcast_ref::<TerminationSignal>().map(|stop| stop.0),
                Some(signal)
            );
            assert!(!raw_mode_enabled()?);
            assert_eq!(terminal_settings()?, before);
            let alive = Command::new("/bin/kill")
                .args(["-0", &shell_id.to_string()])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()?;
            assert!(!alive.success(), "the shell must be stopped and collected");
        }
        Ok(())
    }

    #[test]
    fn termination_closes_the_queue_before_thread_cleanup() -> Result<()> {
        let (sender, receiver) = sync_channel(2);
        sender.send(Event::Terminated(SIGTERM))?;
        sender.send(Event::Output(TOO_LATE.to_vec()))?;
        let mut output = Vec::new();

        let error = forward_events(
            receiver,
            &mut output,
            test_filter(),
            keep_layout,
            draw_nothing,
        )
        .err()
        .context("termination should stop the loop")?;

        assert_eq!(
            error.to_string(),
            format!("session stopped by signal {SIGTERM}")
        );
        assert_eq!(
            error.downcast_ref::<TerminationSignal>().map(|stop| stop.0),
            Some(SIGTERM)
        );
        assert!(output.is_empty());
        assert!(queue_is_closed(&sender));
        Ok(())
    }

    #[test]
    fn session_panic_restores_terminal_before_returning_an_error() -> Result<()> {
        if run_in_test_terminal(test_name!(
            session_panic_restores_terminal_before_returning_an_error
        ))? {
            return Ok(());
        }
        let before = terminal_settings()?;
        let command = shell_command("read value");

        let result = catch_panic::<()>("session panicked", || {
            let _session = Session::start(command)?;
            assert!(raw_mode_enabled()?);
            panic!("test session panic");
        });

        assert!(matches!(result, Err(error) if error.to_string() == "session panicked"));
        assert!(!raw_mode_enabled()?);
        assert_eq!(terminal_settings()?, before);
        Ok(())
    }

    #[test]
    fn resize_panic_closes_the_session_before_cleanup() -> Result<()> {
        let (sender, receiver) = sync_channel(2);
        sender.send(Event::Resized)?;
        sender.send(Event::Output(TOO_LATE.to_vec()))?;
        let mut output = Vec::new();

        let result = forward_events(
            receiver,
            &mut output,
            test_filter(),
            |_| {
                panic!("test resize panic");
            },
            draw_nothing,
        );

        assert!(matches!(result, Err(error) if error.to_string() == "resize handler panicked"));
        assert!(output.is_empty());
        assert!(queue_is_closed(&sender));
        Ok(())
    }

    #[test]
    fn session_resizes_between_flushed_output_events() -> Result<()> {
        /// Records writes, and a `|` for every flush.
        struct Recorder<'a>(&'a RefCell<Vec<u8>>);
        impl Write for Recorder<'_> {
            fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
                self.0.borrow_mut().extend_from_slice(bytes);
                Ok(bytes.len())
            }
            fn flush(&mut self) -> io::Result<()> {
                self.0.borrow_mut().push(b'|');
                Ok(())
            }
        }
        let (sender, receiver) = sync_channel(4);
        sender.send(Event::Output(b"before".to_vec()))?;
        sender.send(Event::Resized)?;
        sender.send(Event::Output(b"after".to_vec()))?;
        sender.send(Event::OutputFinished(Ok(())))?;
        let calls = RefCell::new(Vec::new());
        let main_thread = thread::current().id();

        forward_events(
            receiver,
            Recorder(&calls),
            test_filter(),
            |_| {
                assert_eq!(thread::current().id(), main_thread);
                calls.borrow_mut().extend_from_slice(b"resize|");
                keep_layout(&mut ())
            },
            draw_nothing,
        )?;

        assert_eq!(*calls.borrow(), b"before|resize|after|");
        Ok(())
    }

    /// Written by the test draw handler, for a drawing with nothing wiped.
    const DRAWN: &[u8] = b"<drawn>";

    /// Written by the test draw handler, for a drawing after a wipe.
    const DRAWN_AFTER_WIPE: &[u8] = b"<drawn after wipe>";

    /// One step for the thread that feeds the main loop in the drawing tests.
    enum Step {
        /// Send this event.
        Send(Event),
        /// Wait until the loop has drawn.
        WaitForDrawing,
        /// Wait long enough for a drawing that should not happen.
        WaitPastPause,
    }

    /// Runs the main loop on events fed by another thread as `steps` say, then ends the
    /// output. Returns what the loop wrote.
    fn run_with_drawing(steps: Vec<Step>) -> Result<Vec<u8>> {
        let (sender, receiver) = sync_channel(steps.len() + 1);
        let (drawn_sender, drawn_receiver) = std::sync::mpsc::channel();
        let feeding = thread::spawn(move || -> Result<()> {
            for step in steps {
                match step {
                    Step::Send(event) => sender.send(event)?,
                    Step::WaitForDrawing => drawn_receiver.recv()?,
                    Step::WaitPastPause => thread::sleep(DRAW_PAUSE * 3),
                }
            }
            sender.send(Event::OutputFinished(Ok(())))?;
            Ok(())
        });
        let mut output = Vec::new();

        forward_events(
            receiver,
            &mut output,
            test_filter(),
            keep_layout,
            |output: &mut &mut Vec<u8>, wiped| {
                output.extend_from_slice(if wiped { DRAWN_AFTER_WIPE } else { DRAWN });
                // The feeding thread may have finished already, which is fine.
                let _ = drawn_sender.send(());
                Ok(())
            },
        )?;
        feeding
            .join()
            .map_err(|_| anyhow::anyhow!("feeding thread panicked"))??;
        Ok(output)
    }

    /// `bytes` as the filter passes them on.
    fn filtered(bytes: &[u8]) -> Vec<u8> {
        let mut output = Vec::new();
        test_filter().filter(bytes, &mut output);
        output
    }

    /// The code that clears the whole screen.
    fn clear_screen() -> Vec<u8> {
        format!("{CSI}2{ERASE_SCREEN}").into_bytes()
    }

    #[test]
    fn the_bar_is_drawn_at_the_first_pause_and_not_again() -> Result<()> {
        let output = run_with_drawing(vec![
            Step::Send(Event::Output(b"first".to_vec())),
            Step::WaitForDrawing,
            Step::Send(Event::Output(b"second".to_vec())),
            Step::WaitPastPause,
        ])?;

        assert_eq!(output, [b"first", DRAWN, b"second"].concat());
        Ok(())
    }

    #[test]
    fn clearing_the_screen_draws_the_bar_again() -> Result<()> {
        let output = run_with_drawing(vec![
            Step::WaitForDrawing,
            Step::Send(Event::Output(clear_screen())),
            Step::WaitForDrawing,
        ])?;

        assert_eq!(
            output,
            [DRAWN, &filtered(&clear_screen()), DRAWN_AFTER_WIPE].concat()
        );
        Ok(())
    }

    #[test]
    fn a_resize_draws_the_bar_again() -> Result<()> {
        let output = run_with_drawing(vec![
            Step::WaitForDrawing,
            Step::Send(Event::Resized),
            Step::WaitForDrawing,
        ])?;

        assert_eq!(output, [DRAWN, DRAWN_AFTER_WIPE].concat());
        Ok(())
    }

    #[test]
    fn the_bar_waits_while_a_program_holds_the_screen() -> Result<()> {
        let held_clear = [HOLD_SCREEN.as_bytes(), &clear_screen()].concat();

        let output = run_with_drawing(vec![
            Step::WaitForDrawing,
            Step::Send(Event::Output(held_clear.clone())),
            Step::WaitPastPause,
            Step::Send(Event::Output(RELEASE_SCREEN.as_bytes().to_vec())),
            Step::WaitForDrawing,
        ])?;

        let program_output = filtered(&[&held_clear, RELEASE_SCREEN.as_bytes()].concat());
        assert_eq!(output, [DRAWN, &program_output, DRAWN_AFTER_WIPE].concat());
        Ok(())
    }

    #[test]
    fn a_draw_panic_closes_the_session() -> Result<()> {
        let (sender, receiver) = sync_channel(1);
        let mut output = Vec::new();

        let result = forward_events(receiver, &mut output, test_filter(), keep_layout, |_, _| {
            panic!("test draw panic");
        });

        assert!(matches!(result, Err(error) if error.to_string() == "draw handler panicked"));
        assert!(queue_is_closed(&sender));
        Ok(())
    }

    #[test]
    fn a_resize_moves_the_filter_limit() -> Result<()> {
        let taller = Layout::new(TEST_WINDOW_ROWS + 10);
        let reset_scrolling = format!("{CSI}{SCROLL_REGION}");
        let (sender, receiver) = sync_channel(3);
        sender.send(Event::Resized)?;
        sender.send(Event::Output(reset_scrolling.into_bytes()))?;
        sender.send(Event::OutputFinished(Ok(())))?;
        let mut output = Vec::new();

        forward_events(
            receiver,
            &mut output,
            test_filter(),
            |_| Ok(taller),
            draw_nothing,
        )?;

        assert_eq!(output, limit_scrolling(taller.shell_rows).into_bytes());
        Ok(())
    }

    #[test]
    fn session_events_flush_output_after_input_finishes() -> Result<()> {
        let (sender, receiver) = sync_channel(4);
        sender.send(Event::Output(b"first\n".to_vec()))?;
        sender.send(Event::InputFinished(Ok(())))?;
        sender.send(Event::Output(b"\x1b[32mshell> ".to_vec()))?;
        sender.send(Event::OutputFinished(Ok(())))?;
        let mut bytes = Vec::new();
        let mut output = BufWriter::new(&mut bytes);

        forward_events(
            receiver,
            &mut output,
            test_filter(),
            keep_layout,
            draw_nothing,
        )?;

        assert!(
            output.buffer().is_empty(),
            "the final prompt must be flushed"
        );
        drop(output);
        assert_eq!(bytes, b"first\n\x1b[32mshell> ");
        assert!(queue_is_closed(&sender));
        Ok(())
    }

    #[test]
    fn session_events_return_thread_errors_and_close_the_receiver() -> Result<()> {
        let failure = || io::Error::new(ErrorKind::PermissionDenied, "test failure").into();
        let failing_events = [
            Event::InputFinished(Err(failure())),
            Event::OutputFinished(Err(failure())),
            Event::ResizeFailed(failure()),
            // The resize handler below fails too.
            Event::Resized,
        ];
        for event in failing_events {
            let (sender, receiver) = sync_channel(3);
            sender.send(event)?;
            sender.send(Event::Output(TOO_LATE.to_vec()))?;
            sender.send(Event::OutputFinished(Ok(())))?;
            let mut output = Vec::new();

            let error = forward_events(
                receiver,
                &mut output,
                test_filter(),
                |_| Err(failure()),
                draw_nothing,
            )
            .err()
            .context("the thread error should stop the loop")?;

            assert_eq!(
                error.downcast_ref::<io::Error>().map(io::Error::kind),
                Some(ErrorKind::PermissionDenied)
            );
            assert!(
                output.is_empty(),
                "output after the failure must not be written"
            );
            assert!(queue_is_closed(&sender));
        }
        Ok(())
    }

    #[test]
    fn session_events_need_the_output_to_end_before_the_queue_closes() -> Result<()> {
        let (sender, receiver) = sync_channel(1);
        sender.send(Event::InputFinished(Ok(())))?;
        drop(sender);

        let result = forward_events(
            receiver,
            io::sink(),
            test_filter(),
            keep_layout,
            draw_nothing,
        );

        assert!(matches!(
            result,
            Err(error) if error.to_string() == "session event queue closed unexpectedly"
        ));
        Ok(())
    }

    #[test]
    fn session_events_report_terminal_write_and_flush_failures() -> Result<()> {
        struct FailingWriter {
            fail_on_flush: bool,
        }
        impl Write for FailingWriter {
            fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
                if self.fail_on_flush {
                    Ok(bytes.len())
                } else {
                    Err(ErrorKind::BrokenPipe.into())
                }
            }
            fn flush(&mut self) -> io::Result<()> {
                Err(ErrorKind::BrokenPipe.into())
            }
        }

        for (fail_on_flush, expected) in [
            (false, "could not write shell output"),
            (true, "could not flush shell output"),
        ] {
            let (sender, receiver) = sync_channel(2);
            sender.send(Event::Output(b"shell> ".to_vec()))?;
            sender.send(Event::OutputFinished(Ok(())))?;

            let error = forward_events(
                receiver,
                FailingWriter { fail_on_flush },
                test_filter(),
                keep_layout,
                draw_nothing,
            )
            .err()
            .context("the terminal failure should stop the loop")?;

            assert_eq!(error.to_string(), expected);
            assert_eq!(
                error.downcast_ref::<io::Error>().map(io::Error::kind),
                Some(ErrorKind::BrokenPipe)
            );
            assert!(queue_is_closed(&sender));
        }
        Ok(())
    }

    #[test]
    fn session_start_restores_terminal_after_spawn_failure() -> Result<()> {
        if run_in_test_terminal(test_name!(
            session_start_restores_terminal_after_spawn_failure
        ))? {
            return Ok(());
        }
        let before = terminal_settings()?;
        assert!(!raw_mode_enabled()?);

        let result = Session::start(CommandBuilder::new("/dev/null/meowsh-shell"));

        assert!(matches!(result, Err(error) if error.to_string() == "could not start shell"));
        assert!(!raw_mode_enabled()?);
        assert_eq!(terminal_settings()?, before);
        Ok(())
    }

    #[test]
    fn session_drop_restores_terminal_after_success() -> Result<()> {
        if run_in_test_terminal(test_name!(session_drop_restores_terminal_after_success))? {
            return Ok(());
        }
        let before = terminal_settings()?;

        let mut session = Session::start(shell_command("exit 7"))?;

        assert!(raw_mode_enabled()?);
        assert_ne!(terminal_settings()?, before);
        let size = session.master.get_size()?;
        let window = PtySize::default();
        assert_eq!(
            (size.rows, size.cols),
            (window.rows - BAR_ROWS, window.cols)
        );
        let status = session.shell.child.wait()?;
        assert_eq!(status.exit_code(), 7);

        drop(session);

        assert!(!raw_mode_enabled()?);
        assert_eq!(terminal_settings()?, before);
        Ok(())
    }
}
