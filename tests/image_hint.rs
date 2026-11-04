//! Tests that Meowsh steps aside in a terminal that cannot show pictures, and says why only
//! the first time.

#![cfg(unix)]

use meowsh::app::IMAGE_HINT;
use meowsh::store::image_hint::IMAGE_HINT_FILE;
use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use std::io::{ErrorKind, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

/// How long each step may take. Longer than Meowsh waits for the picture check.
const TIMEOUT: Duration = Duration::from_secs(10);

/// The test shell's prompt, printed once it is ready for a command.
const PROMPT: &[u8] = b"MEOWSH_PROMPT$ ";

/// A home folder of its own, removed when the test ends.
struct TestHome(PathBuf);

impl TestHome {
    fn new() -> Result<Self> {
        let unique = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let path =
            std::env::temp_dir().join(format!("meowsh-hint-{}-{unique}", std::process::id()));
        std::fs::create_dir(&path)?;
        Ok(Self(path))
    }
}

impl Drop for TestHome {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Whether a file named `name` is anywhere inside `folder`.
fn holds_file(folder: &Path, name: &str) -> Result<bool> {
    for entry in std::fs::read_dir(folder)? {
        let path = entry?.path();
        if path.file_name().is_some_and(|file_name| file_name == name)
            || (path.is_dir() && holds_file(&path, name)?)
        {
            return Ok(true);
        }
    }
    Ok(false)
}

fn contains(bytes: &[u8], part: &[u8]) -> bool {
    bytes.windows(part.len()).any(|window| window == part)
}

/// Reads output until `expected` appears, and returns everything read.
fn read_until(output: &Receiver<Vec<u8>>, expected: &[u8]) -> Result<Vec<u8>> {
    let deadline = Instant::now() + TIMEOUT;
    let mut received = Vec::new();
    while !contains(&received, expected) {
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
    }
    Ok(received)
}

/// Runs Meowsh in a test terminal that never answers the picture check, waits for the
/// shell's prompt, ends the shell & returns what was shown before the prompt.
fn run_in_silent_terminal(home: &Path) -> Result<Vec<u8>> {
    let pty = native_pty_system().openpty(PtySize::default())?;
    let mut reader = pty.master.try_clone_reader()?;
    let mut writer = pty.master.take_writer()?;
    let mut command = CommandBuilder::new(env!("CARGO_BIN_EXE_meowsh"));
    command.arg("start");
    command.env("HOME", home);
    command.env_remove("XDG_DATA_HOME");
    command.env_remove("TERM_PROGRAM");
    command.env("SHELL", "/bin/sh");
    command.env("ENV", "/dev/null");
    command.env("PS1", String::from_utf8_lossy(PROMPT).as_ref());
    let mut child = pty.slave.spawn_command(command)?;
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

    let shown = read_until(&output, PROMPT)?;
    writeln!(writer, "exit 0")?;
    writer.flush()?;
    let status = child.wait()?;
    assert!(status.success());
    drop(pty.master);
    reader_thread
        .join()
        .map_err(|_| "output reader thread panicked")?;
    Ok(shown)
}

#[test]
fn the_picture_hint_is_shown_only_in_the_first_window() -> Result {
    let home = TestHome::new()?;

    let first = run_in_silent_terminal(&home.0)?;
    let second = run_in_silent_terminal(&home.0)?;

    assert!(contains(&first, IMAGE_HINT.as_bytes()));
    assert!(!contains(&second, IMAGE_HINT.as_bytes()));
    // The data folder is in a different place on each system, but always under the home.
    assert!(holds_file(&home.0, IMAGE_HINT_FILE)?);
    Ok(())
}
