//! Joins the parts together and runs the shell inside Meowsh. It is the only part that may
//! use `term`, `cat`, and `art` together.
//!
//! * `events`: the threads that read keys, read shell output, and watch signals.
//! * `session`: starts the shell in a PTY and runs the main loop.

mod events;
mod session;

use crate::env;
use crate::term::pty::{build_shell_command, run_shell_directly};
use anyhow::Result;
use std::ffi::{OsStr, OsString};
use std::process::ExitCode;

pub use session::Session;

/// The session was stopped by a signal such as SIGTERM.
#[derive(Debug)]
pub struct TerminationSignal(pub i32);

impl std::fmt::Display for TerminationSignal {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "session stopped by signal {}", self.0)
    }
}

impl std::error::Error for TerminationSignal {}

/// Runs the shell inside Meowsh, or directly if Meowsh cannot wrap it.
///
/// `shell` is the shell to run. When it is missing, the user's `SHELL` is used.
pub fn start(shell: Option<OsString>, login: bool) -> Result<ExitCode> {
    let shell_path = match shell {
        Some(path) => path,
        None => env::shell()?,
    };
    match Session::start(build_shell_command(&shell_path, login)) {
        Ok(session) => session.run(),
        Err(error) => fall_back(&error, &shell_path, login),
    }
}

/// Says why Meowsh could not wrap the shell, then runs the shell directly.
fn fall_back(error: &anyhow::Error, shell_path: &OsStr, login: bool) -> Result<ExitCode> {
    // The full chain names the real cause, such as a missing terminal.
    eprintln!("meowsh: {error:#}; starting shell directly");
    run_shell_directly(shell_path, login)
}

/// Turns a panic into an error, after the closure's guards have put the terminal back.
pub fn catch_panic<T>(message: &'static str, operation: impl FnOnce() -> Result<T>) -> Result<T> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(operation))
        .map_err(|_| anyhow::anyhow!(message))?
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{self, ErrorKind};

    #[test]
    fn panic_boundary_keeps_success_and_regular_errors() -> Result<()> {
        assert_eq!(catch_panic("unexpected panic", || Ok(7))?, 7);

        let error = catch_panic::<()>("unexpected panic", || {
            Err(io::Error::new(ErrorKind::BrokenPipe, "test failure").into())
        })
        .err()
        .ok_or_else(|| anyhow::anyhow!("the error should pass through"))?;

        assert_eq!(
            error.downcast_ref::<io::Error>().map(io::Error::kind),
            Some(ErrorKind::BrokenPipe)
        );
        Ok(())
    }

    #[test]
    fn panic_boundary_turns_a_panic_into_an_error() {
        let result = catch_panic::<()>("unexpected panic", || panic!("test panic"));

        assert!(matches!(result, Err(error) if error.to_string() == "unexpected panic"));
    }
}
