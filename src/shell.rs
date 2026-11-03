//! The startup scripts that `meowsh init` prints for bash and zsh.
//!
//! In a new window, the script swaps the shell for Meowsh. Inside Meowsh's own shell, it
//! adds the prompt mark instead.

use crate::term::escape::PROMPT_MARK;

/// Replaced with the quoted path to the meowsh program.
const EXECUTABLE_PLACEHOLDER: &str = "@MEOWSH_EXECUTABLE@";

/// Replaced with the OSC number of the prompt mark.
const PROMPT_MARK_PLACEHOLDER: &str = "@MEOWSH_PROMPT_MARK@";

/// The shells Meowsh can start from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum Shell {
    /// zsh, the default shell on macOS.
    Zsh,
    /// bash, the default shell on most Linux systems.
    Bash,
}

impl Shell {
    /// The shell's name, as typed after `meowsh init`.
    pub fn name(self) -> &'static str {
        match self {
            Self::Zsh => "zsh",
            Self::Bash => "bash",
        }
    }
}

/// Returns the startup script for `shell`, set up to run `executable`.
pub fn init_script(shell: Shell, executable: &str) -> String {
    let script = match shell {
        Shell::Bash => include_str!("shell/bash.sh"),
        Shell::Zsh => include_str!("shell/zsh.sh"),
    };
    script
        .replace(EXECUTABLE_PLACEHOLDER, &quote(executable))
        .replace(PROMPT_MARK_PLACEHOLDER, PROMPT_MARK)
}

/// Wraps a value in single quotes, so the shell reads it exactly as written.
fn quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::env::{self, MEOWSH_TTY};
    use crate::term::escape::{BELL, OSC};
    use crate::term::pty::{ShellGuard, forward_output};
    use crate::term::test_terminal::one_pty_test_at_a_time;
    use anyhow::{Context, Result};
    use portable_pty::{CommandBuilder, PtySize, native_pty_system};
    use std::io::{Read, Write};
    use std::process::Command;
    use std::sync::mpsc::Receiver;
    use std::time::{Duration, Instant};

    const SHELLS: [Shell; 2] = [Shell::Bash, Shell::Zsh];

    /// Printed when the startup script decides not to start Meowsh.
    const SKIPPED: &str = "SKIPPED";

    /// Stands in for meowsh, so the test sees the arguments it would get.
    const FAKE_EXECUTABLE: &str = "/bin/echo";

    /// How the shell's own terminal is passed as the mark in `evaluate`.
    const OWN_TERMINAL: &str = "own";

    /// The comment that starts the part of each script for Meowsh's own shell.
    const INNER_SHELL_COMMENT: &str = "Inside Meowsh's own shell";

    /// How long a test shell may take to start, to answer a line, or to exit.
    const SHELL_WAIT: Duration = Duration::from_secs(10);

    /// A shell that prints nothing for this long is waiting for the next line.
    const SHELL_QUIET: Duration = Duration::from_millis(300);

    fn shell_path(shell: Shell) -> &'static str {
        match shell {
            Shell::Bash => "/bin/bash",
            Shell::Zsh => "/bin/zsh",
        }
    }

    /// Flags that make the shell skip the user's own settings files.
    fn no_settings_flags(shell: Shell) -> &'static [&'static str] {
        match shell {
            Shell::Bash => &["--noprofile", "--norc"],
            Shell::Zsh => &["-f"],
        }
    }

    /// The startup script, followed by a line that only runs if Meowsh was not started.
    fn script_then_skipped(shell: Shell) -> String {
        format!("{}\nprintf {SKIPPED}", init_script(shell, FAKE_EXECUTABLE))
    }

    /// Settings for one run of the startup script.
    struct Case<'a> {
        login: bool,
        mark: &'a str,
        ssh: bool,
        tmux: bool,
    }

    /// Runs the startup script in a real terminal and returns what it printed.
    fn evaluate(shell: Shell, case: Case) -> Result<String> {
        let pty = native_pty_system().openpty(PtySize::default())?;
        let reader = pty.master.try_clone_reader()?;
        let mut command = CommandBuilder::new(shell_path(shell));
        command.args(no_settings_flags(shell));
        command.arg("-i");
        if case.login {
            command.arg("-l");
        }
        command.args(["-c", &script_then_skipped(shell)]);

        let tty_name = pty
            .master
            .tty_name()
            .context("test terminal should have a name")?;
        let mark = if case.mark == OWN_TERMINAL {
            tty_name.to_str().context("terminal name should be text")?
        } else {
            case.mark
        };
        command.env(MEOWSH_TTY, mark);
        command.env("PATH", "/bin:/usr/bin");
        command.env(env::SHELL, "/deliberately/wrong");
        command.env(
            "SSH_CONNECTION",
            if case.ssh { "test connection" } else { "" },
        );
        command.env(env::TMUX, if case.tmux { "test tmux" } else { "" });
        let mut child = ShellGuard {
            child: pty.slave.spawn_command(command)?,
        };
        drop(pty.slave);

        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut bytes = Vec::new();
            let result = forward_output(reader, &mut bytes).map(|()| bytes);
            // The test may have timed out and stopped listening.
            let _ = sender.send(result);
        });
        let bytes = receiver
            .recv_timeout(SHELL_WAIT)
            .context("shell init timed out")??;
        anyhow::ensure!(child.child.wait()?.success(), "shell should succeed");
        Ok(String::from_utf8(bytes)?.trim().to_owned())
    }

    #[test]
    fn init_keeps_login_mode_and_starts_new_windows_and_standalone_tmux() -> Result<()> {
        let _pty_turn = one_pty_test_at_a_time();
        for shell in SHELLS {
            for login in [false, true] {
                for (mark, tmux) in [("", false), ("/dev/stale", false), ("", true)] {
                    let case = Case {
                        login,
                        mark,
                        ssh: false,
                        tmux,
                    };
                    let output = evaluate(shell, case)?;
                    let arguments = output
                        .strip_prefix("start --shell ")
                        .context("meowsh should be started")?;
                    let actual_path = if login {
                        arguments
                            .strip_suffix(" --login")
                            .context("login mode should be kept")?
                    } else {
                        arguments
                    };
                    // Linux may find /bin/zsh through /usr/bin after startup.
                    assert_eq!(
                        std::fs::canonicalize(actual_path)?,
                        std::fs::canonicalize(shell_path(shell))?
                    );
                }
            }
        }
        Ok(())
    }

    #[test]
    fn init_skips_own_shell_ssh_and_tmux_with_an_inherited_mark() -> Result<()> {
        let _pty_turn = one_pty_test_at_a_time();
        for shell in SHELLS {
            for (mark, ssh, tmux) in [
                (OWN_TERMINAL, false, false),
                ("", true, false),
                ("/dev/stale", false, true),
            ] {
                let case = Case {
                    login: false,
                    mark,
                    ssh,
                    tmux,
                };
                assert_eq!(evaluate(shell, case)?, SKIPPED);
            }
        }
        Ok(())
    }

    #[test]
    fn init_skips_noninteractive_or_redirected_shells() -> Result<()> {
        for shell in SHELLS {
            for interactive in [false, true] {
                let mut command = Command::new(shell_path(shell));
                command.args(no_settings_flags(shell));
                if interactive {
                    command.arg("-i");
                }
                // Output goes to a pipe here, not a terminal, so Meowsh should step aside.
                let output = command
                    .arg("-c")
                    .arg(script_then_skipped(shell))
                    .env("ENV", "/dev/null")
                    .output()?;
                assert!(output.status.success());
                assert_eq!(output.stdout, SKIPPED.as_bytes());
            }
        }
        Ok(())
    }

    #[test]
    fn executable_paths_are_quoted_literally() -> Result<()> {
        let path = "/tmp/meowsh 'quoted' $(printf wrong)";
        for shell in SHELLS {
            let output = Command::new(shell_path(shell))
                .args(["-f", "-c", &format!("printf '%s' {}", quote(path))])
                .output()?;
            assert!(output.status.success());
            assert_eq!(output.stdout, path.as_bytes());
        }
        Ok(())
    }

    /// Reads shell output into `bytes` until the shell has printed something new and then
    /// gone quiet, so it is waiting at its prompt.
    fn wait_until_quiet(receiver: &Receiver<Vec<u8>>, bytes: &mut Vec<u8>) -> Result<()> {
        let before = bytes.len();
        let deadline = Instant::now() + SHELL_WAIT;
        loop {
            match receiver.recv_timeout(SHELL_QUIET) {
                Ok(chunk) => bytes.extend(chunk),
                Err(_) if bytes.len() > before => return Ok(()),
                Err(_) => {}
            }
            if Instant::now() >= deadline {
                anyhow::bail!(
                    "the shell did not answer; it printed: {:?}",
                    String::from_utf8_lossy(bytes)
                );
            }
        }
    }

    /// Starts `shell` as Meowsh's own inner shell with the startup script in its settings
    /// file, types each line of `lines`, and returns the exit status in every prompt mark.
    fn prompt_marks(shell: Shell, lines: &[&str]) -> Result<Vec<String>> {
        let folder = std::env::temp_dir().join(format!(
            "meowsh-prompt-mark-{}-{}",
            std::process::id(),
            shell.name()
        ));
        std::fs::create_dir_all(&folder)?;
        let settings = folder.join(match shell {
            Shell::Bash => "bashrc",
            Shell::Zsh => ".zshrc",
        });
        std::fs::write(&settings, init_script(shell, FAKE_EXECUTABLE))?;

        let pty = native_pty_system().openpty(PtySize::default())?;
        let mut reader = pty.master.try_clone_reader()?;
        let mut writer = pty.master.take_writer()?;
        let mut command = CommandBuilder::new(shell_path(shell));
        // Skip the system's own startup files, so only the test's settings file runs. Some
        // systems set zsh up in ways that swallow the first key.
        match shell {
            Shell::Bash => {
                command.args(["--noprofile", "--rcfile"]);
                command.arg(&settings);
            }
            Shell::Zsh => {
                command.arg("-d");
            }
        }
        command.arg("-i");
        command.env(env::ZDOTDIR, &folder);
        command.env("HISTFILE", folder.join("history"));
        command.env("PATH", "/bin:/usr/bin");
        let tty_name = pty
            .master
            .tty_name()
            .context("test terminal should have a name")?;
        command.env(MEOWSH_TTY, tty_name);
        let mut child = ShellGuard {
            child: pty.slave.spawn_command(command)?,
        };
        drop(pty.slave);

        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut buffer = [0u8; 4096];
            // Stops when the shell exits and its terminal closes.
            while let Ok(count @ 1..) = reader.read(&mut buffer) {
                if sender.send(buffer[..count].to_vec()).is_err() {
                    break;
                }
            }
        });
        // Each line waits until the shell is quiet at its prompt, as a person typing would.
        // Some systems take a while to start zsh, and keys typed too early can be lost.
        let mut bytes = Vec::new();
        for line in lines.iter().chain(&["exit"]) {
            wait_until_quiet(&receiver, &mut bytes)
                .with_context(|| format!("{} before {line:?}", shell.name()))?;
            writer.write_all(format!("{line}\r").as_bytes())?;
        }
        // Collect the rest until the shell exits and the reader stops.
        while let Ok(chunk) = receiver.recv_timeout(SHELL_WAIT) {
            bytes.extend(chunk);
        }
        child.child.wait()?;
        std::fs::remove_dir_all(&folder)?;

        let text = String::from_utf8_lossy(&bytes);
        let start = format!("{OSC}{PROMPT_MARK};");
        Ok(text
            .split(start.as_str())
            .skip(1)
            .filter_map(|after| after.split_once(BELL))
            .map(|(status, _)| status.to_owned())
            .collect())
    }

    #[test]
    fn the_inner_shell_marks_each_prompt_after_a_command() -> Result<()> {
        let _pty_turn = one_pty_test_at_a_time();
        for shell in SHELLS {
            let marks = prompt_marks(shell, &["false", "", "true"])?;
            // The empty line ran nothing, so it has no mark.
            assert_eq!(marks, ["1", "0"], "{}", shell.name());
        }
        Ok(())
    }

    #[test]
    fn an_outer_shell_has_no_prompt_mark() -> Result<()> {
        for shell in SHELLS {
            let script = init_script(shell, FAKE_EXECUTABLE);
            // The marks live only inside the part for Meowsh's own shell.
            let (outer, inner) = script
                .split_once(INNER_SHELL_COMMENT)
                .context("the script should have a part for Meowsh's own shell")?;
            assert!(!outer.contains(PROMPT_MARK), "{}", shell.name());
            assert!(inner.contains(PROMPT_MARK), "{}", shell.name());
        }
        Ok(())
    }
}
