//! The cat part: the brain that picks what the cat does. It never touches the terminal,
//! files, or the real clock.
//!
//! * `day`: turns the time of day into meal, nap, night, or free time.
//! * `motion`: where the cat is in the bar & walking it from one spot to another.
//! * `player`: plays one animation by its timing.

pub mod day;
pub mod motion;
pub mod player;

use crate::anim::{self, Animation, CANVAS};
use crate::scene::{Facing, Scene, ScenePicture};
use anyhow::{Context, Result};
use day::{PartOfDay, SECONDS_PER_DAY, Schedule};
use motion::Walk;
use player::Player;
use std::time::Duration;

/// The animation the cat walks with.
const WALK_ANIMATION: &str = "walk";

/// The animation the cat sleeps with.
const SLEEP_ANIMATION: &str = "sleep";

/// The animation the cat eats with. It sits until `eat` is drawn.
const EAT_ANIMATION: &str = "sit";

/// How long a meal lasts on the cat's clock, in seconds.
const MEAL_SECONDS: u64 = 5 * 60;

/// The time, as the cat is told it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Time {
    /// How long Meowsh has run. Animations play by it.
    pub since_start: Duration,
    /// Seconds on the cat's clock since midnight of the day Meowsh started. Past
    /// `SECONDS_PER_DAY` it is a later day. The cat's day goes by it.
    pub clock: u64,
}

impl Time {
    /// Which day it is, counting the day Meowsh started as 0.
    fn day(self) -> u64 {
        self.clock / SECONDS_PER_DAY
    }

    /// Seconds since the last midnight.
    fn second_of_day(self) -> u32 {
        // Less than `SECONDS_PER_DAY`, so it always fits.
        u32::try_from(self.clock % SECONDS_PER_DAY).unwrap_or(0)
    }
}

/// One meal: which one & on which day, so each is only eaten once.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Meal {
    part: PartOfDay,
    day: u64,
}

/// What the cat is doing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    /// Free time: it walks the bar.
    Free(Walk),
    /// It eats until `until` on its clock.
    Eating { player: Player, until: u64 },
    /// It sleeps until the nap or the night ends.
    Sleeping(Player),
}

/// The animations the cat needs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Animations {
    walk: &'static Animation,
    sleep: &'static Animation,
    eat: &'static Animation,
}

/// The cat. It is handed the time & the bar's size, & says what should be in the bar.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cat {
    animations: Animations,
    schedule: Schedule,
    mode: Mode,
    /// Where the cat is, kept up to date while it walks.
    x: u32,
    facing: Facing,
    /// The last meal it started, so it does not eat the same one twice.
    last_meal: Option<Meal>,
}

impl Cat {
    /// A cat at the left end of the bar, as Meowsh starts.
    pub fn new(schedule: Schedule) -> Result<Self> {
        let animations = Animations {
            walk: find_animation(WALK_ANIMATION)?,
            sleep: find_animation(SLEEP_ANIMATION)?,
            eat: find_animation(EAT_ANIMATION)?,
        };
        Ok(Self {
            animations,
            schedule,
            // Already there, so the first update picks where to walk.
            mode: Mode::Free(Walk::start(animations.walk, 0, 0, Duration::ZERO)),
            x: 0,
            facing: Facing::Right,
            last_meal: None,
        })
    }

    /// Moves the cat on to `time`, in a bar `bar_width` art pixels wide, & says what should
    /// be in the bar.
    pub fn update(&mut self, time: Time, bar_width: u32) -> Scene {
        // The furthest right the cat's left edge can be, with the whole cat in the bar.
        let right_end = bar_width.saturating_sub(CANVAS.width);
        let now = time.since_start;
        self.pick_mode(time);
        let (animation, frame) = match &mut self.mode {
            Mode::Free(walk) => {
                let walking = walk.at(now);
                // After a resize, the spot it was walking to may be past the new end.
                if walking.arrived || walk.destination() > right_end {
                    let from = walking.x.min(right_end);
                    // It walks to whichever end is further away, so it paces the whole bar.
                    let to = if from < right_end - from {
                        right_end
                    } else {
                        0
                    };
                    *walk = Walk::start(self.animations.walk, from, to, now);
                }
                let walking = walk.at(now);
                self.x = walking.x;
                self.facing = walking.facing;
                (self.animations.walk, walking.frame)
            }
            Mode::Eating { player, .. } | Mode::Sleeping(player) => {
                self.x = self.x.min(right_end);
                (player.animation(), player.at(now).frame)
            }
        };
        Scene {
            pictures: vec![ScenePicture {
                animation: animation.name,
                frame,
                x: self.x,
                facing: self.facing,
            }],
        }
    }

    /// How long after `since_start` the scene changes, or `None` if it stays the same
    /// until something happens. A new part of the day is noticed at the next frame, which
    /// is soon, because every animation the cat plays loops.
    pub fn until_next_change(&self, since_start: Duration) -> Option<Duration> {
        match &self.mode {
            Mode::Free(walk) => walk.until_next_frame(since_start),
            Mode::Eating { player, .. } | Mode::Sleeping(player) => {
                player.until_next_frame(since_start)
            }
        }
    }

    /// Changes the mode when the part of the day calls for another one.
    fn pick_mode(&mut self, time: Time) {
        let now = time.since_start;
        match (self.schedule.part_at(time.second_of_day()), self.mode) {
            (PartOfDay::Nap | PartOfDay::Night, Mode::Sleeping(_)) => {}
            (PartOfDay::Nap | PartOfDay::Night, _) => {
                self.mode = Mode::Sleeping(Player::start(self.animations.sleep, now));
            }
            (part @ (PartOfDay::Breakfast | PartOfDay::Dinner), mode) => {
                let meal = Meal {
                    part,
                    day: time.day(),
                };
                if self.last_meal != Some(meal) {
                    // The meal starts now, also when Meowsh opens partway through meal time.
                    self.last_meal = Some(meal);
                    self.mode = Mode::Eating {
                        player: Player::start(self.animations.eat, now),
                        until: time.clock + MEAL_SECONDS,
                    };
                } else if let Mode::Eating { until, .. } = mode
                    && time.clock >= until
                {
                    self.mode = self.free(now);
                }
            }
            (PartOfDay::Free, Mode::Free(_)) => {}
            (PartOfDay::Free, _) => self.mode = self.free(now),
        }
    }

    /// Free time, starting where the cat is.
    fn free(&self, now: Duration) -> Mode {
        // Already there, so the next update picks where to walk.
        Mode::Free(Walk::start(self.animations.walk, self.x, self.x, now))
    }
}

/// Finds an animation the cat needs, by its folder name.
fn find_animation(name: &str) -> Result<&'static Animation> {
    anim::find(name).with_context(|| format!("there is no {name} animation"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A bar with room for the cat to walk 120 art pixels.
    const BAR_WIDTH: u32 = CANVAS.width + 120;

    /// Ten in the morning, in free time.
    const MORNING: u64 = 10 * 60 * 60;

    /// Half past seven in the morning, at breakfast.
    const BREAKFAST: u64 = 7 * 60 * 60 + 30 * 60;

    /// One in the afternoon, when the nap starts.
    const NAP_START: u64 = 13 * 60 * 60;

    /// The time `seconds` after Meowsh started at `clock`, with the cat's clock running at
    /// the same speed as Meowsh.
    fn time(clock: u64, seconds: u64) -> Time {
        Time {
            since_start: Duration::from_secs(seconds),
            clock: clock + seconds,
        }
    }

    /// The only picture in the scene after an update at `time`.
    fn update(cat: &mut Cat, time: Time, bar_width: u32) -> Result<ScenePicture> {
        let scene = cat.update(time, bar_width);
        scene
            .pictures
            .first()
            .copied()
            .context("the cat should be in the bar")
    }

    #[test]
    fn in_free_time_the_cat_walks_to_the_far_end_and_turns_around() -> Result<()> {
        let mut cat = Cat::new(Schedule::default())?;
        let step = cat.animations.walk.step;
        let frame_ms = u64::from(cat.animations.walk.frame_ms);
        let crossing_ms = u64::from(120 / step) * frame_ms;
        let at = |milliseconds: u64| Time {
            since_start: Duration::from_millis(milliseconds),
            clock: MORNING,
        };

        let start = update(&mut cat, at(0), BAR_WIDTH)?;
        let across = update(&mut cat, at(crossing_ms), BAR_WIDTH)?;
        let back = update(&mut cat, at(crossing_ms + frame_ms), BAR_WIDTH)?;

        assert_eq!((start.x, start.facing), (0, Facing::Right));
        assert_eq!((across.x, across.facing), (120, Facing::Left));
        assert_eq!(back.x, 120 - step);
        Ok(())
    }

    #[test]
    fn a_bar_made_smaller_keeps_the_cat_inside_it() -> Result<()> {
        let mut cat = Cat::new(Schedule::default())?;
        update(&mut cat, time(MORNING, 0), BAR_WIDTH)?;

        let picture = update(&mut cat, time(MORNING, 1), CANVAS.width + 30)?;

        assert!(picture.x <= 30, "the cat is at {}, past 30", picture.x);
        assert_eq!(update(&mut cat, time(MORNING, 2), 10)?.x, 0);
        Ok(())
    }

    #[test]
    fn the_cat_sleeps_through_the_nap_where_it_is_then_walks_again() -> Result<()> {
        let mut cat = Cat::new(Schedule::default())?;
        let before_nap = NAP_START - 10;
        update(&mut cat, time(before_nap, 0), BAR_WIDTH)?;
        let walked_to = update(&mut cat, time(before_nap, 9), BAR_WIDTH)?.x;

        let asleep = update(&mut cat, time(before_nap, 10), BAR_WIDTH)?;
        let still_asleep = update(&mut cat, time(before_nap, 600), BAR_WIDTH)?;
        let after_nap = update(&mut cat, time(before_nap, 2 * 60 * 60 + 10), BAR_WIDTH)?;

        assert_eq!((asleep.animation, asleep.x), (SLEEP_ANIMATION, walked_to));
        assert_eq!(still_asleep.animation, SLEEP_ANIMATION);
        assert_eq!(after_nap.animation, WALK_ANIMATION);
        Ok(())
    }

    #[test]
    fn each_meal_is_eaten_once_for_its_time_even_if_meowsh_opens_partway_through() -> Result<()> {
        let mut cat = Cat::new(Schedule::default())?;
        let at = |seconds: u64| time(BREAKFAST, seconds);

        let eating = update(&mut cat, at(0), BAR_WIDTH)?;
        let near_the_end = update(&mut cat, at(MEAL_SECONDS - 1), BAR_WIDTH)?;
        let done = update(&mut cat, at(MEAL_SECONDS), BAR_WIDTH)?;
        let later = update(&mut cat, at(MEAL_SECONDS + 600), BAR_WIDTH)?;
        let tomorrow = update(&mut cat, at(SECONDS_PER_DAY), BAR_WIDTH)?;

        assert_eq!(eating.animation, EAT_ANIMATION);
        assert_eq!(near_the_end.animation, EAT_ANIMATION);
        assert_eq!(done.animation, WALK_ANIMATION);
        assert_eq!(later.animation, WALK_ANIMATION);
        assert_eq!(tomorrow.animation, EAT_ANIMATION);
        Ok(())
    }
}
