//! What is in the bar. Plain shared data: every part may use it.

/// Which way the cat faces. Frames are drawn facing right.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Facing {
    /// The way the frames are drawn.
    Right,
    /// A mirror copy of the drawn frames.
    Left,
}

/// One picture in the bar: a frame of an animation or a prop, at a place, facing a way.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScenePicture {
    /// The animation's folder name, such as `sit` or `bowl`.
    pub animation: &'static str,
    /// The frame number, counting from 0.
    pub frame: usize,
    /// How far from the left end of the bar it starts, in art pixels.
    pub x: u32,
    /// Which way it faces.
    pub facing: Facing,
}

/// What should be in the bar right now, back to front: later pictures are drawn on top.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Scene {
    /// The pictures to show.
    pub pictures: Vec<ScenePicture>,
}
