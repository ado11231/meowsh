//! What is in the bar. Plain shared data: every part may use it.

/// Which way the cat faces. Frames are drawn facing right.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Facing {
    /// The way the frames are drawn.
    Right,
    /// A mirror copy of the drawn frames.
    Left,
}
