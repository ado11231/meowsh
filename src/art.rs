//! The art part: loads the cat's frames & gets them ready to show. It never uses `term`
//! or `cat`.
//!
//! * `flip`: makes the left facing copy of a frame.
//! * `library`: every frame, loaded once at startup.
//! * `recolor`: changes the drawn colors to the terminal's own.
//! * `placeholder`: a plain white oval that stands in for the cat until real frames exist.
//! * `still`: one picture with its pixels.

pub mod flip;
pub mod library;
pub mod placeholder;
pub mod recolor;
pub mod still;
