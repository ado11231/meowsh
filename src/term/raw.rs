//! The real terminal that Meowsh runs in: its name, its size, raw mode, and signals.

use anyhow::{Context, Result};
use portable_pty::PtySize;
use signal_hook::consts::{SIGHUP, SIGINT, SIGQUIT, SIGTERM, SIGWINCH};
use signal_hook::iterator::Signals;
use std::ffi::{CStr, OsStr, OsString};
use std::os::unix::ffi::OsStrExt;

/// Room for the terminal's path, such as /dev/ttys003.
const TTY_NAME_BUFFER_SIZE: usize = 1024;

/// Reads stdin's terminal name without depending on shell variables or PATH.
pub fn tty_name() -> Option<OsString> {
    let mut buffer = [0 as libc::c_char; TTY_NAME_BUFFER_SIZE];
    // SAFETY: the buffer is writable, and its length is passed along with it.
    let result = unsafe { libc::ttyname_r(libc::STDIN_FILENO, buffer.as_mut_ptr(), buffer.len()) };
    if result != 0 {
        return None;
    }
    // SAFETY: on success, ttyname_r wrote a name that ends with a zero byte into the buffer.
    let name = unsafe { CStr::from_ptr(buffer.as_ptr()) };
    Some(OsStr::from_bytes(name.to_bytes()).to_os_string())
}

/// Reads the terminal's size in rows and columns, and in pixels when the terminal says.
pub fn read_size() -> Result<PtySize> {
    let window = crossterm::terminal::window_size().context("could not read terminal size")?;

    Ok(PtySize {
        rows: window.rows,
        cols: window.columns,
        // Zero means the pixel size is unknown.
        pixel_width: window.width,
        pixel_height: window.height,
    })
}

/// Watches window resizes and shutdown signals.
///
/// Register this before raw mode, so shutdown signals also take the cleanup path.
pub fn watch_signals() -> Result<Signals> {
    Signals::new([SIGWINCH, SIGHUP, SIGINT, SIGQUIT, SIGTERM])
        .context("could not watch terminal signals")
}

/// Puts the terminal back in its normal mode when this value leaves scope.
pub struct RawModeGuard;

impl RawModeGuard {
    /// Switches the terminal to raw mode, so every key passes straight through.
    pub fn enable() -> Result<Self> {
        crossterm::terminal::enable_raw_mode().context("could not enable raw terminal mode")?;

        Ok(Self)
    }
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        // Cleanup must also run when a function returns an error, so a failure here has
        // nowhere to go.
        let _ = crossterm::terminal::disable_raw_mode();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resize_listener_receives_window_size_signal() -> Result<()> {
        let mut signals = watch_signals()?;

        signal_hook::low_level::raise(SIGWINCH).context("resize signal should be sent")?;

        // Read pending signals without blocking, in case registration is broken.
        let received: Vec<_> = signals.pending().collect();
        assert_eq!(received, vec![SIGWINCH]);
        Ok(())
    }
}
