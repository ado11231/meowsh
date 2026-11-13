//! A still picture: its size & its pixels, ready to show.

/// A still picture with its pixels.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Still {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// `width * height` pixels in RGBA order, row by row from the top left.
    pub rgba: Vec<u8>,
}
