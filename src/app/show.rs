//! What plays in the bar: the cat living its day, or one animation for checking art.

use crate::cat::Cat;
use crate::cat::player::Player;
use crate::scene::{Facing, Scene, ScenePicture};
use std::time::Duration;

/// Where a previewed animation stands, in art pixels from the left end of the bar.
const PREVIEW_X: u32 = 0;

/// What plays in the bar.
pub enum Show {
    /// The cat, picking what to do.
    Cat(Cat),
    /// One animation, played in place at the left end, for `meowsh preview`.
    Preview(Player),
}

impl Show {
    /// What should be in the bar at `now`, the time since Meowsh started, in a bar
    /// `bar_width` art pixels wide.
    pub fn scene(&mut self, now: Duration, bar_width: u32) -> Scene {
        match self {
            Self::Cat(cat) => cat.update(now, bar_width),
            Self::Preview(player) => Scene {
                pictures: vec![ScenePicture {
                    animation: player.animation().name,
                    frame: player.at(now).frame,
                    x: PREVIEW_X,
                    facing: Facing::Right,
                }],
            },
        }
    }

    /// How long after `now` the scene changes, or `None` if it stays the same until
    /// something happens.
    pub fn until_next_change(&self, now: Duration) -> Option<Duration> {
        match self {
            Self::Cat(cat) => cat.until_next_change(now),
            Self::Preview(player) => player.until_next_frame(now),
        }
    }
}
