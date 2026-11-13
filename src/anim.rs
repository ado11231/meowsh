//! The list of animations, made by `build.rs` from the settings in `assets/animations/`.
//! Plain shared data: every part may use it.

/// The size every frame shares & the row the cat's feet stand on, from `canvas.toml`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Canvas {
    /// Width of every frame, in pixels.
    pub width: u32,
    /// Height of every frame, in pixels.
    pub height: u32,
    /// The pixel row the feet stand on, counted from the top.
    pub floor: u32,
}

/// One animation's settings, from its `anim.toml`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Animation {
    /// The folder name, such as `sit`.
    pub name: &'static str,
    /// How long each frame shows, in milliseconds.
    pub frame_ms: u32,
    /// True: play again from the start. False: stop on the last frame.
    pub loops: bool,
    /// Pixels the cat moves forward each frame. 0 means it stays in place.
    pub step: u32,
    /// True: change white to the terminal's text color.
    pub recolor: bool,
    /// One time per frame, in milliseconds, for frames that hold longer.
    pub frame_ms_each: Option<&'static [u32]>,
}

include!(concat!(env!("OUT_DIR"), "/animations.rs"));

/// Finds an animation by its folder name.
pub fn find(name: &str) -> Option<&'static Animation> {
    ANIMATIONS.iter().find(|animation| animation.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::{Context, Result, ensure};

    #[test]
    fn finds_an_animation_by_name() -> Result<()> {
        let sit = find("sit").context("sit is missing")?;
        ensure!(sit.name == "sit");
        Ok(())
    }

    #[test]
    fn an_unknown_name_finds_nothing() -> Result<()> {
        ensure!(find("not_an_animation").is_none());
        Ok(())
    }
}
