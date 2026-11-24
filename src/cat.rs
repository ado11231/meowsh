//! The cat part: the brain that picks what the cat does. It never touches the terminal,
//! files, or the real clock.
//!
//! * `motion`: where the cat is in the bar & walking it from one spot to another.
//! * `player`: plays one animation by its timing.

pub mod motion;
pub mod player;

use crate::anim::{self, Animation, CANVAS};
use crate::scene::{Scene, ScenePicture};
use anyhow::{Context, Result};
use motion::Walk;
use std::time::Duration;

/// The animation the cat walks with.
const WALK_ANIMATION: &str = "walk";

/// The cat. It is handed the time & the bar's size, & says what should be in the bar.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cat {
    walk_animation: &'static Animation,
    walk: Walk,
}

impl Cat {
    /// A cat at the left end of the bar, as Meowsh starts.
    pub fn new() -> Result<Self> {
        let walk_animation = anim::find(WALK_ANIMATION)
            .with_context(|| format!("there is no {WALK_ANIMATION} animation"))?;
        Ok(Self {
            walk_animation,
            // Already there, so the first update picks where to walk.
            walk: Walk::start(walk_animation, 0, 0, Duration::ZERO),
        })
    }

    /// Moves the cat on to `now`, the time since Meowsh started, in a bar `bar_width` art
    /// pixels wide, & says what should be in the bar.
    pub fn update(&mut self, now: Duration, bar_width: u32) -> Scene {
        // The furthest right the cat's left edge can be, with the whole cat in the bar.
        let right_end = bar_width.saturating_sub(CANVAS.width);
        let walking = self.walk.at(now);
        // After a resize, the spot it was walking to may be past the new end.
        if walking.arrived || self.walk.destination() > right_end {
            let from = walking.x.min(right_end);
            // It walks to whichever end is further away, so it paces the whole bar.
            let to = if from < right_end - from {
                right_end
            } else {
                0
            };
            self.walk = Walk::start(self.walk_animation, from, to, now);
        }
        let walking = self.walk.at(now);
        Scene {
            pictures: vec![ScenePicture {
                animation: self.walk_animation.name,
                frame: walking.frame,
                x: walking.x,
                facing: walking.facing,
            }],
        }
    }

    /// How long after `now` the scene changes, or `None` if it stays the same until
    /// something happens.
    pub fn until_next_change(&self, now: Duration) -> Option<Duration> {
        self.walk.until_next_frame(now)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::Facing;

    /// A bar with room for the cat to walk 120 art pixels.
    const BAR_WIDTH: u32 = CANVAS.width + 120;

    /// Where the cat is & which way it faces after an update at `now`.
    fn place(cat: &mut Cat, now: Duration, bar_width: u32) -> Result<(u32, Facing)> {
        let scene = cat.update(now, bar_width);
        let picture = scene
            .pictures
            .first()
            .context("the cat should be in the bar")?;
        Ok((picture.x, picture.facing))
    }

    #[test]
    fn the_cat_walks_to_the_far_end_and_turns_around() -> Result<()> {
        let mut cat = Cat::new()?;
        let step = cat.walk_animation.step;
        let frame_ms = u64::from(cat.walk_animation.frame_ms);
        let steps_across = u64::from(120 / step);
        let crossing = Duration::from_millis(steps_across * frame_ms);

        assert_eq!(
            place(&mut cat, Duration::ZERO, BAR_WIDTH)?,
            (0, Facing::Right)
        );
        assert_eq!(place(&mut cat, crossing, BAR_WIDTH)?, (120, Facing::Left));
        let one_step_back = crossing + Duration::from_millis(frame_ms);
        assert_eq!(
            place(&mut cat, one_step_back, BAR_WIDTH)?,
            (120 - step, Facing::Left)
        );
        Ok(())
    }

    #[test]
    fn a_bar_made_smaller_keeps_the_cat_inside_it() -> Result<()> {
        let mut cat = Cat::new()?;
        place(&mut cat, Duration::ZERO, BAR_WIDTH)?;

        let smaller = CANVAS.width + 30;
        let (x, facing) = place(&mut cat, Duration::from_secs(1), smaller)?;

        assert!(x <= 30, "the cat is at {x}, past the end at 30");
        assert_eq!(facing, Facing::Left);
        Ok(())
    }

    #[test]
    fn a_bar_narrower_than_the_cat_keeps_it_at_the_left_end() -> Result<()> {
        let mut cat = Cat::new()?;

        assert_eq!(place(&mut cat, Duration::ZERO, 10)?.0, 0);
        assert_eq!(place(&mut cat, Duration::from_secs(5), 10)?.0, 0);
        Ok(())
    }
}
