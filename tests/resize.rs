//! Tests that a real Meowsh process gives the shell the right size as the window changes,
//! and exits with the right code.

#![cfg(unix)]

use portable_pty::{Child, CommandBuilder, PtySize, native_pty_system};
use std::io::{ErrorKind, Read, Write};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};

type Result = std::result::Result<(), Box<dyn std::error::Error>>;

/// How long each step may take.
const TIMEOUT: Duration = Duration::from_secs(10);

/// How often the test checks whether Meowsh exited.
const POLL_INTERVAL: Duration = Duration::from_millis(10);

/// Exit code of a process stopped by SIGTERM (128 + 15).
const TERMINATED_EXIT_CODE: u32 = 143;

/// Printed by the test shell once it has started.
const READY: &[u8] = b"MEOWSH_READY";

fn size(rows: u16, cols: u16) -> PtySize {
    PtySize {
        rows,
        cols,
        pixel_width: 0,
        pixel_height: 0,
    }
}

/// Stops Meowsh if a test ends early.
struct TestSession {
    child: Box<dyn Child + Send + Sync>,
}

impl Drop for TestSession {
    fn drop(&mut self) {
        if !matches!(self.child.try_wait(), Ok(Some(_))) {
            // Cleanup has nowhere to report a failure.
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

/// Reads output until `expected` appears.
fn wait_for_output(output: &Receiver<Vec<u8>>, expected: &[u8]) -> Result {
    let deadline = Instant::now() + TIMEOUT;
    let mut received = Vec::new();

    loop {
        let chunk = output
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .map_err(|error| {
                format!(
                    "missing {:?}: {error}; output: {}",
                    String::from_utf8_lossy(expected),
                    String::from_utf8_lossy(&received)
                )
            })?;
        received.extend_from_slice(&chunk);
        if contains(&received, expected) {
            return Ok(());
        }
    }
}

fn contains(bytes: &[u8], part: &[u8]) -> bool {
    bytes.windows(part.len()).any(|window| window == part)
}

#[test]
fn start_forwards_resizes_and_keeps_shell_exit_code() -> Result {
    check_session(false)
}

#[test]
fn termination_exits_with_the_signal_code() -> Result {
    check_session(true)
}

/// Runs Meowsh in a test terminal, resizes it a few times, and checks the shell's size.
/// Then ends the shell, or stops Meowsh with SIGTERM when `terminate` is true.
fn check_session(terminate: bool) -> Result {
    let pty = native_pty_system().openpty(size(24, 80))?;
    let mut reader = pty.master.try_clone_reader()?;
    let mut writer = pty.master.take_writer()?;
    let mut command = CommandBuilder::new(env!("CARGO_BIN_EXE_meowsh"));
    command.arg("start");
    command.env("SHELL", "/bin/sh");
    command.env("ENV", "/dev/null");
    command.env("PS1", "$ ");
    command.env_remove("TMUX");
    let mut session = TestSession {
        child: pty.slave.spawn_command(command)?,
    };
    drop(pty.slave);

    let (sender, output) = mpsc::channel();
    let reader_thread = thread::spawn(move || {
        let mut buffer = [0u8; 4096];
        loop {
            match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(count) => {
                    if sender.send(buffer[..count].to_vec()).is_err() {
                        break;
                    }
                }
                Err(error) if error.kind() == ErrorKind::Interrupted => continue,
                // Linux reports the end of a PTY as an error.
                Err(_) => break,
            }
        }
    });

    // The shell sees the first line once it is ready, so typing cannot be lost.
    writeln!(writer, "printf '%s\\n' {}", String::from_utf8_lossy(READY))?;
    writer.flush()?;
    wait_for_output(&output, READY)?;

    for (rows, cols) in [(24, 80), (40, 120), (9, 30), (24, 80)] {
        pty.master.resize(size(rows, cols))?;

        // Give the main loop time to resize, with a shell loop that gives up.
        writeln!(
            writer,
            "for attempt in 1 2 3 4 5 6 7 8 9 10; do size=$(stty size); if [ \"$size\" = '{rows} {cols}' ]; then printf '\\nMEOWSH_SIZE_%s_DONE\\n' \"$size\"; break; fi; sleep 0.05; done"
        )?;
        writer.flush()?;
        let done = format!("MEOWSH_SIZE_{rows} {cols}_DONE");
        wait_for_output(&output, done.as_bytes())?;
    }

    if terminate {
        let process_id = session
            .child
            .process_id()
            .ok_or("Meowsh should have a process ID")?;
        let status = std::process::Command::new("/bin/kill")
            .args(["-TERM", &process_id.to_string()])
            .status()?;
        assert!(status.success());
    } else {
        writeln!(writer, "exit 7")?;
        writer.flush()?;
    }
    let deadline = Instant::now() + TIMEOUT;
    let status = loop {
        if let Some(status) = session.child.try_wait()? {
            break status;
        }
        assert!(Instant::now() < deadline, "Meowsh should exit promptly");
        thread::sleep(POLL_INTERVAL);
    };

    let expected_code = if terminate { TERMINATED_EXIT_CODE } else { 7 };
    assert_eq!(status.exit_code(), expected_code);
    drop(pty.master);
    reader_thread
        .join()
        .map_err(|_| "output reader thread panicked")?;
    Ok(())
}
