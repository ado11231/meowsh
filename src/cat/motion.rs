//! Where the cat is in the bar & which way it faces, while it walks from one spot to
//! another. Positions are the cat's left edge, in art pixels from the left end of the bar.

use super::player::Player;
use crate::anim::Animation;
use crate::scene::Facing;
use std::time::Duration;

/// A walk from one spot in the bar to another. The cat moves the animation's `step` pixels
/// each time the frame changes, so its feet keep pace with the drawing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Walk {
    player: Player,
    step: u32,
    from: u32,
    to: u32,
}

/// Where a walk is at one moment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Walking {
    /// The frame number, counting from 0.
    pub frame: usize,
    /// The cat's left edge, in art pixels.
    pub x: u32,
    /// Which way the cat faces: the way it walks.
    pub facing: Facing,
    /// True once the cat is at the spot it walked to.
    pub arrived: bool,
}

impl Walk {
    /// Starts walking from `from` to `to` at `now`, playing `animation`.
    pub fn start(animation: &'static Animation, from: u32, to: u32, now: Duration) -> Self {
        Self {
            player: Player::start(animation, now),
            step: animation.step,
            from,
            to,
        }
    }

    /// The spot the cat is walking to.
    pub fn destination(&self) -> u32 {
        self.to
    }

    /// Where the walk is at `now`.
    pub fn at(&self, now: Duration) -> Walking {
        let distance = self.from.abs_diff(self.to);
        let walked = self
            .player
            .frames_passed(now)
            .saturating_mul(u64::from(self.step));
        // An animation with no step never gets anywhere, so it counts as there at once.
        let arrived = self.step == 0 || walked >= u64::from(distance);
        // Less than `distance` here, so it fits.
        let walked = if arrived {
            distance
        } else {
            u32::try_from(walked).unwrap_or(distance)
        };
        let (x, facing) = if self.to >= self.from {
            (self.from + walked, Facing::Right)
        } else {
            (self.from - walked, Facing::Left)
        };
        Walking {
            frame: self.player.at(now).frame,
            x,
            facing,
            arrived,
        }
    }

    /// How long after `now` the next frame shows, & the cat moves.
    pub fn until_next_frame(&self, now: Duration) -> Option<Duration> {
        self.player.until_next_frame(now)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A made up walk of 4 frames, 100 ms each, moving 6 pixels a frame.
    const WALK: Animation = Animation {
        name: "test_walk",
        frame_count: 4,
        frame_ms: 100,
        loops: true,
        step: 6,
        recolor: true,
        frame_ms_each: None,
    };

    /// `WALK`, but moving nowhere.
    const IN_PLACE: Animation = Animation { step: 0, ..WALK };

    fn ms(milliseconds: u64) -> Duration {
        Duration::from_millis(milliseconds)
    }

    #[test]
    fn the_cat_moves_one_step_with_each_frame_and_stops_at_the_spot() {
        let walk = Walk::start(&WALK, 10, 40, ms(1000));

        assert_eq!(walk.at(ms(1099)).x, 10);
        assert_eq!(walk.at(ms(1100)).x, 16);
        // Past the end of the first loop, the steps keep adding up.
        assert_eq!(walk.at(ms(1450)).x, 34);
        assert!(!walk.at(ms(1450)).arrived);
        // 30 pixels is 5 steps. It stops there, not one step past it.
        let there = walk.at(ms(1500));
        assert_eq!((there.x, there.arrived), (40, true));
        assert_eq!(walk.at(ms(9000)).x, 40);
    }

    #[test]
    fn a_spot_that_is_not_a_whole_number_of_steps_away_is_reached_exactly() {
        let walk = Walk::start(&WALK, 0, 10, ms(0));

        assert_eq!(walk.at(ms(100)).x, 6);
        assert_eq!(walk.at(ms(200)).x, 10);
        assert!(walk.at(ms(200)).arrived);
    }

    #[test]
    fn walking_left_faces_left() {
        let walk = Walk::start(&WALK, 40, 10, ms(0));

        let walking = walk.at(ms(100));
        assert_eq!((walking.x, walking.facing), (34, Facing::Left));
        assert_eq!(walk.at(ms(0)).facing, Facing::Left);
    }

    #[test]
    fn an_animation_with_no_step_is_there_at_once() {
        let walking = Walk::start(&IN_PLACE, 0, 50, ms(0)).at(ms(0));
        assert_eq!((walking.x, walking.arrived), (50, true));
    }
}
