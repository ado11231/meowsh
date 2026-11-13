//! Plays one animation: which frame to show at a given time & whether it is done. It is
//! handed the time, so tests can make up any time.

use crate::anim::Animation;
use std::time::Duration;

/// One animation being played, from the time it started.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Player {
    animation: &'static Animation,
    started: Duration,
}

/// What to show at one moment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Playing {
    /// The frame number, counting from 0.
    pub frame: usize,
    /// True once an animation that does not loop has shown its last frame for its full time.
    /// A looping animation is never done.
    pub done: bool,
}

impl Player {
    /// Starts playing `animation` at `now`, which is time since Meowsh started.
    pub fn start(animation: &'static Animation, now: Duration) -> Self {
        Self {
            animation,
            started: now,
        }
    }

    /// What to show at `now`.
    pub fn at(&self, now: Duration) -> Playing {
        self.position(now).0
    }

    /// How long after `now` a different frame shows, or `None` if the frame shown now stays.
    pub fn until_next_frame(&self, now: Duration) -> Option<Duration> {
        let milliseconds = self.position(now).1?;
        // A frame shows for at most `u32::MAX` milliseconds, so this always fits.
        Some(Duration::from_millis(
            u64::try_from(milliseconds).unwrap_or(u64::MAX),
        ))
    }

    /// What to show at `now`, & how many milliseconds until a different frame shows.
    fn position(&self, now: Duration) -> (Playing, Option<u128>) {
        let frame_count = self.animation.frame_count;
        let last_frame = frame_count.saturating_sub(1);
        let held = Playing {
            frame: last_frame,
            done: true,
        };
        let total: u128 = (0..frame_count).map(|index| self.frame_ms(index)).sum();
        // The build lets in no animation without frames or with a time of 0, but dividing
        // by 0 below would crash, so this is checked anyway.
        if total == 0 {
            return (held, None);
        }
        let elapsed = now.saturating_sub(self.started).as_millis();
        if !self.animation.loops && elapsed >= total {
            return (held, None);
        }
        let mut time_in = elapsed % total;
        for frame in 0..frame_count {
            let time = self.frame_ms(frame);
            if time_in < time {
                let playing = Playing { frame, done: false };
                // Played once, the last frame stays when it ends.
                let stays = !self.animation.loops && frame == last_frame;
                return (playing, (!stays).then_some(time - time_in));
            }
            time_in -= time;
        }
        // Not reached: `time_in` is less than `total`, the sum of the frame times.
        (held, None)
    }

    /// How long one frame shows, in milliseconds.
    fn frame_ms(&self, index: usize) -> u128 {
        let each = self
            .animation
            .frame_ms_each
            .and_then(|times| times.get(index).copied());
        u128::from(each.unwrap_or(self.animation.frame_ms))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A made up animation of 3 frames, 100 ms each.
    const THREE_FRAMES: Animation = Animation {
        name: "test",
        frame_count: 3,
        frame_ms: 100,
        loops: true,
        step: 0,
        recolor: true,
        frame_ms_each: None,
    };

    /// `THREE_FRAMES`, played once.
    const THREE_FRAMES_ONCE: Animation = Animation {
        loops: false,
        ..THREE_FRAMES
    };

    /// `THREE_FRAMES`, with the middle frame held longer, like a blink.
    const MIDDLE_HELD: Animation = Animation {
        frame_ms_each: Some(&[100, 500, 100]),
        ..THREE_FRAMES
    };

    fn ms(milliseconds: u64) -> Duration {
        Duration::from_millis(milliseconds)
    }

    /// What shows `milliseconds` after an animation started at 1 second.
    fn playing_at(animation: &'static Animation, milliseconds: u64) -> Playing {
        Player::start(animation, ms(1000)).at(ms(1000 + milliseconds))
    }

    #[test]
    fn each_frame_shows_for_frame_ms() {
        assert_eq!(playing_at(&THREE_FRAMES, 0).frame, 0);
        assert_eq!(playing_at(&THREE_FRAMES, 99).frame, 0);
        assert_eq!(playing_at(&THREE_FRAMES, 100).frame, 1);
        assert_eq!(playing_at(&THREE_FRAMES, 299).frame, 2);
    }

    #[test]
    fn a_looping_animation_starts_again_and_is_never_done() {
        assert_eq!(
            playing_at(&THREE_FRAMES, 300),
            Playing {
                frame: 0,
                done: false
            }
        );
        assert_eq!(playing_at(&THREE_FRAMES, 3 * 300 + 150).frame, 1);
    }

    #[test]
    fn an_animation_played_once_holds_its_last_frame_when_done() {
        assert_eq!(
            playing_at(&THREE_FRAMES_ONCE, 299),
            Playing {
                frame: 2,
                done: false
            }
        );
        assert_eq!(
            playing_at(&THREE_FRAMES_ONCE, 300),
            Playing {
                frame: 2,
                done: true
            }
        );
        assert_eq!(playing_at(&THREE_FRAMES_ONCE, 10_000).frame, 2);
    }

    #[test]
    fn frame_ms_each_holds_a_frame_longer() {
        assert_eq!(playing_at(&MIDDLE_HELD, 599).frame, 1);
        assert_eq!(playing_at(&MIDDLE_HELD, 600).frame, 2);
        assert_eq!(playing_at(&MIDDLE_HELD, 700).frame, 0);
    }

    /// How long until the next frame, `milliseconds` after an animation started at 1 second.
    fn next_frame_at(animation: &'static Animation, milliseconds: u64) -> Option<Duration> {
        Player::start(animation, ms(1000)).until_next_frame(ms(1000 + milliseconds))
    }

    #[test]
    fn the_next_frame_comes_when_the_shown_one_ends() {
        assert_eq!(next_frame_at(&THREE_FRAMES, 0), Some(ms(100)));
        assert_eq!(next_frame_at(&THREE_FRAMES, 150), Some(ms(50)));
        // The last frame of a loop is followed by the first.
        assert_eq!(next_frame_at(&THREE_FRAMES, 250), Some(ms(50)));
        assert_eq!(next_frame_at(&MIDDLE_HELD, 100), Some(ms(500)));
    }

    #[test]
    fn the_last_frame_of_an_animation_played_once_has_no_next_frame() {
        assert_eq!(next_frame_at(&THREE_FRAMES_ONCE, 150), Some(ms(50)));
        assert_eq!(next_frame_at(&THREE_FRAMES_ONCE, 250), None);
        assert_eq!(next_frame_at(&THREE_FRAMES_ONCE, 10_000), None);
    }

    #[test]
    fn a_time_before_the_start_shows_the_first_frame() {
        assert_eq!(Player::start(&THREE_FRAMES, ms(1000)).at(ms(0)).frame, 0);
    }
}
