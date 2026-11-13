//! The art part: loads the cat's frames & gets them ready to show. It never uses `term`
//! or `cat`.
//!
//! * `flip`: makes the left facing copy of a frame.
//! * `library`: every frame, loaded once at startup.
//! * `recolor`: changes the drawn colors to the terminal's own.
//! * `still`: one picture with its pixels.

pub mod flip;
pub mod library;
pub mod recolor;
pub mod still;
