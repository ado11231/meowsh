//! Joins the parts together and runs the shell inside Meowsh. It is the only part that may
//! use `term`, `cat`, and `art` together.
//!
//! * `events`: the threads that read keys, read shell output, and watch signals.
//! * `pictures`: turns the cat's scene into pictures for the painter.
//! * `session`: starts the shell in a PTY and runs the main loop.
//! * `show`: what plays in the bar, the cat or one animation.

mod events;
mod pictures;
mod session;
mod show;

use crate::anim::{self, Animation};
use crate::art::library::Library;
use crate::art::recolor::Colors;
use crate::cat::Cat;
use crate::cat::player::Player;
use crate::env;
use crate::store::{self, image_hint};
use crate::term::image_check::check_images;
use crate::term::pty::{build_shell_command, run_shell_directly};
use anyhow::Result;
use std::ffi::{OsStr, OsString};
use std::process::ExitCode;
use std::time::Duration;

pub use session::Session;
use show::Show;

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
    run(shell, login, None)
}

/// Runs the user's shell inside Meowsh with one animation playing in the bar, for checking
/// art. Unlike `start`, a name with no animation is an error, so a typo is not missed.
pub fn preview(name: &str) -> Result<ExitCode> {
    let Some(animation) = anim::find(name) else {
        let names: Vec<&str> = anim::ANIMATIONS
            .iter()
            .map(|animation| animation.name)
            .collect();
        anyhow::bail!(
            "there is no animation named {name}. There are: {}",
            names.join(", ")
        );
    };
    run(None, false, Some(animation))
}

/// Runs `shell` inside Meowsh with the cat in the bar, or `preview` playing in its place,
/// or runs it directly if Meowsh cannot wrap it.
fn run(
    shell: Option<OsString>,
    login: bool,
    preview: Option<&'static Animation>,
) -> Result<ExitCode> {
    let shell_path = match shell {
        Some(path) => path,
        None => env::shell()?,
    };
    let image_check = match check_images() {
        Ok(image_check) => image_check,
        Err(error) => return fall_back(&error, &shell_path, login),
    };
    if !image_check.supported {
        // There is no text version of the cat, so Meowsh steps aside. Keys typed during the
        // check cannot be handed to a shell that replaces this process, so they are lost.
        show_image_hint_once();
        return run_shell_directly(&shell_path, login);
    }
    let colors = match (image_check.text_color, image_check.background_color) {
        (Some(text), Some(background)) => Some(Colors { text, background }),
        // Without both colors the cat keeps the colors it was drawn in.
        _ => None,
    };
    let art = Library::load(colors).and_then(|library| {
        let show = match preview {
            // The show's time starts with the session, at zero.
            Some(animation) => Show::Preview(Player::start(animation, Duration::ZERO)),
            None => Show::Cat(Cat::new()?),
        };
        Ok((library, show))
    });
    let (library, show) = match art {
        Ok(art) => art,
        Err(error) => return fall_back(&error, &shell_path, login),
    };
    let mut session = match Session::start(build_shell_command(&shell_path, login)) {
        Ok(session) => session,
        Err(error) => return fall_back(&error, &shell_path, login),
    };
    session.pass_on_early_input(&image_check.early_input)?;
    session.run(&library, show)
}

/// Shown once when the terminal said no to pictures. Each stays on one line.
pub const IMAGE_HINT: &str = "meowsh: this terminal cannot show pictures, so the cat stays away";

/// Shown instead of `IMAGE_HINT` in VS Code, where pictures are a setting.
pub const VSCODE_IMAGE_HINT: &str =
    "meowsh: turn on terminal.integrated.enableImages in VS Code to see the cat";

/// Prints the hint about pictures, unless it was shown before in any window.
fn show_image_hint_once() {
    let first_time = store::data_folder().and_then(|folder| image_hint::first_time(&folder));
    // The hint is a nicety. If its file cannot be read or made, staying silent is better than
    // showing it in every new window or keeping the shell from starting.
    if let Ok(true) = first_time {
        eprintln!(
            "{}",
            image_hint_text(std::env::var_os(env::TERM_PROGRAM).as_deref())
        );
    }
}

/// Picks the hint for the terminal program named in `TERM_PROGRAM`.
fn image_hint_text(term_program: Option<&OsStr>) -> &'static str {
    if term_program == Some(OsStr::new(env::VSCODE_TERM_PROGRAM)) {
        VSCODE_IMAGE_HINT
    } else {
        IMAGE_HINT
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
    use anyhow::Context;
    use std::io::{self, ErrorKind};

    #[test]
    fn previewing_an_unknown_animation_names_the_ones_there_are() -> Result<()> {
        let error = preview("not_an_animation")
            .err()
            .context("an unknown name should be an error")?;

        let message = error.to_string();
        assert!(message.contains("not_an_animation"), "{message}");
        assert!(message.contains("sit"), "{message}");
        Ok(())
    }

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
    fn vs_code_gets_its_own_picture_hint() {
        let vscode = OsStr::new(env::VSCODE_TERM_PROGRAM);

        assert_eq!(image_hint_text(Some(vscode)), VSCODE_IMAGE_HINT);
        assert_eq!(image_hint_text(Some(OsStr::new("kitty"))), IMAGE_HINT);
        assert_eq!(image_hint_text(None), IMAGE_HINT);
    }

    #[test]
    fn panic_boundary_turns_a_panic_into_an_error() {
        let result = catch_panic::<()>("unexpected panic", || panic!("test panic"));

        assert!(matches!(result, Err(error) if error.to_string() == "unexpected panic"));
    }
}
