//! The `meowsh` command: reads the command line and runs the matching command.

use anyhow::Result;
use clap::{Parser, Subcommand};
use meowsh::app::{self, TerminationSignal, catch_panic};
use meowsh::commands::{init, install};
use meowsh::shell::Shell;
use std::ffi::OsString;
use std::process::ExitCode;

#[derive(Parser)]
#[command(name = "meowsh", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Adds the startup line to your shell settings.
    Install,
    /// Removes the startup line from your shell settings.
    Uninstall,
    // Start and Init are helpers for shell startup & Preview is for checking art, so they stay
    // out of the normal help.
    #[command(hide = true)]
    Start {
        /// Starts the shell as a login shell.
        #[arg(long)]
        login: bool,
        /// The shell to run. When left out, `SHELL` is used.
        #[arg(long)]
        shell: Option<OsString>,
    },
    /// Plays one animation in the bar, for checking art.
    #[command(hide = true)]
    Preview {
        /// The animation's folder name, such as `sit`.
        name: String,
    },
    #[command(hide = true)]
    Init {
        /// The shell to print the startup script for.
        shell: Shell,
    },
}

/// Shells report "stopped by signal N" with exit code 128 + N.
const SIGNAL_EXIT_BASE: i32 = 128;

fn main() -> ExitCode {
    let cli = Cli::parse();
    // Stay quiet on panic. The error is printed below, after the terminal is put back.
    std::panic::set_hook(Box::new(|_| {}));
    match catch_panic("meowsh crashed", || run(cli)) {
        Ok(code) => code,
        Err(error) => {
            // Keep the message on one line, so it cannot break the shell prompt.
            eprintln!("meowsh: {}", error.to_string().replace(['\r', '\n'], " "));
            match error.downcast_ref::<TerminationSignal>() {
                Some(signal) => signal_exit_code(signal.0),
                None => ExitCode::FAILURE,
            }
        }
    }
}

fn run(cli: Cli) -> Result<ExitCode> {
    match cli.command {
        Command::Install => install::run(true)?,
        Command::Uninstall => install::run(false)?,
        Command::Start { login, shell } => return app::start(shell, login),
        Command::Preview { name } => return app::preview(&name),
        Command::Init { shell } => init::run(shell)?,
    }
    Ok(ExitCode::SUCCESS)
}

fn signal_exit_code(signal: i32) -> ExitCode {
    u8::try_from(SIGNAL_EXIT_BASE + signal).map_or(ExitCode::FAILURE, ExitCode::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::error::ErrorKind;

    fn parse(arguments: &[&str]) -> Result<Command, clap::Error> {
        Cli::try_parse_from(arguments).map(|cli| cli.command)
    }

    fn error_kind(arguments: &[&str]) -> Option<ErrorKind> {
        parse(arguments).err().map(|error| error.kind())
    }

    #[test]
    fn commands_are_recognized() -> Result<()> {
        assert!(matches!(parse(&["meowsh", "install"])?, Command::Install));
        assert!(matches!(
            parse(&["meowsh", "uninstall"])?,
            Command::Uninstall
        ));
        assert!(matches!(
            parse(&["meowsh", "start"])?,
            Command::Start { login: false, .. }
        ));
        Ok(())
    }

    #[test]
    fn start_accepts_a_shell_path_and_login() -> Result<()> {
        let command = parse(&["meowsh", "start", "--shell", "/tmp/my shell", "--login"])?;

        assert!(matches!(
            command,
            Command::Start { shell: Some(path), login: true } if path == "/tmp/my shell"
        ));
        Ok(())
    }

    #[test]
    fn preview_needs_an_animation_name() -> Result<()> {
        assert!(matches!(
            parse(&["meowsh", "preview", "sit"])?,
            Command::Preview { name } if name == "sit"
        ));
        assert_eq!(
            error_kind(&["meowsh", "preview"]),
            Some(ErrorKind::MissingRequiredArgument)
        );
        Ok(())
    }

    #[test]
    fn init_accepts_supported_shells() -> Result<()> {
        assert!(matches!(
            parse(&["meowsh", "init", "zsh"])?,
            Command::Init { shell: Shell::Zsh }
        ));
        assert!(matches!(
            parse(&["meowsh", "init", "bash"])?,
            Command::Init { shell: Shell::Bash }
        ));
        Ok(())
    }

    #[test]
    fn init_rejects_missing_or_unsupported_shells() {
        assert_eq!(
            error_kind(&["meowsh", "init"]),
            Some(ErrorKind::MissingRequiredArgument)
        );
        assert_eq!(
            error_kind(&["meowsh", "init", "fish"]),
            Some(ErrorKind::InvalidValue)
        );
    }

    #[test]
    fn signals_become_shell_style_exit_codes() {
        assert_eq!(signal_exit_code(15), ExitCode::from(143));
        assert_eq!(signal_exit_code(200), ExitCode::FAILURE);
    }
}
