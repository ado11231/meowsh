//! The terminal part: the PTY, the real terminal, and the codes sent to it. It never uses
//! `cat` or `art`.
//!
//! * `escape`: named terminal codes.
//! * `raw`: the real terminal's raw mode, size, name, and signals.

pub mod escape;
pub mod raw;
