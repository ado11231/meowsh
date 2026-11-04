//! The terminal part: the PTY, the real terminal, and the codes sent to it. It never uses
//! `cat` or `art`.
//!
//! * `bar`: the bottom rows kept for the cat, and the codes that keep the shell out of them.
//! * `escape`: named terminal codes.
//! * `filter`: passes the shell's output on, changing only codes that would reach the bar.
//! * `image_check`: asks the real terminal whether it can show pictures.
//! * `pty`: opens the PTY and starts the shell in it, or starts the shell directly.
//! * `raw`: the real terminal's raw mode, size, name, and signals.

pub mod bar;
pub mod escape;
pub mod filter;
pub mod image_check;
pub mod pty;
pub mod raw;
#[cfg(test)]
pub mod test_terminal;
