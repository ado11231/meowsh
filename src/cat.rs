//! The cat part: the brain that picks what the cat does. It never touches the terminal,
//! files, or the real clock.
//!
//! * `action`: picks a resting action for free time.
//! * `day`: turns the time of day into meal, nap, night, or free time.
//! * `motion`: where the cat is in the bar & walking it from one spot to another.
//! * `player`: plays one animation by its timing.

pub mod action;
pub mod day;
pub mod motion;
pub mod player;

use crate::anim::{self, Animation, CANVAS};
use crate::scene::{Facing, Scene, ScenePicture};
use action::Action;
use anyhow::{Context, Result};
use day::{PartOfDay, SECONDS_PER_DAY, Schedule};
use fastrand::Rng;
use motion::Walk;
use player::Player;
use std::ops::RangeInclusive;
use std::time::Duration;

/// The animation the cat sits & waits with.
const SIT_ANIMATION: &str = "sit";

/// The animation the cat walks with.
const WALK_ANIMATION: &str = "walk";

/// The animation the cat sleeps with.
const SLEEP_ANIMATION: &str = "sleep";

/// The animation the cat eats with. Until it is drawn, the cat sits.
const EAT_ANIMATION: &str = "eat";

/// The animation the cat plays with. Until it is drawn, the cat sits.
const PLAY_ANIMATION: &str = "play";

/// The animation the cat runs with. Until it is drawn, the cat walks.
const RUN_ANIMATION: &str = "run";

/// How far right a left facing picture is drawn from a right facing one at the same spot,
/// so the cat turns around `CANVAS.turn_x` instead of the middle of the canvas. Below 0, it
/// is drawn further left.
const LEFT_FACING_SHIFT: i64 = 2 * CANVAS.turn_x as i64 - (CANVAS.width as i64 - 1);

/// The furthest left the cat goes, so a left facing picture never starts left of the bar.
const LEFT_END: u32 = if LEFT_FACING_SHIFT < 0 {
    LEFT_FACING_SHIFT.unsigned_abs() as u32
} else {
    0
};

/// The cat's home spot, where it eats & sleeps: the left end of the bar.
const HOME_X: u32 = LEFT_END;

/// How long a meal lasts on the cat's clock, in seconds, from when it gets home.
const MEAL_SECONDS: u64 = 5 * 60;

/// How long the cat sits or plays before it picks again, in seconds.
const STAY_SECONDS: RangeInclusive<u64> = 10..=40;

/// The time, as the cat is told it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Time {
    /// How long Meowsh has run. Animations & resting actions go by it.
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
    /// Free time: it picks resting actions, one after another.
    Free(Rest),
    /// It walks to its home spot, to do `then` there.
    GoingHome { walk: Walk, then: AtHome },
    /// It eats until `until` on its clock.
    Eating { player: Player, until: u64 },
    /// It sleeps until the nap or the night ends.
    Sleeping(Player),
}

/// What the cat goes home to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AtHome {
    Eat,
    Sleep,
}

/// The resting action the cat is doing in free time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Rest {
    /// It walks or runs to a spot, then picks again.
    Moving(Walk),
    /// It sits or plays where it is until `until`, then picks again.
    Staying { player: Player, until: Duration },
}

/// The animations the cat plays, with the ones not drawn yet filled in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Animations {
    sit: &'static Animation,
    walk: &'static Animation,
    sleep: &'static Animation,
    eat: &'static Animation,
    play: &'static Animation,
    run: &'static Animation,
}

impl Animations {
    fn find() -> Result<Self> {
        let sit = find_animation(SIT_ANIMATION)?;
        let walk = find_animation(WALK_ANIMATION)?;
        Ok(Self {
            sit,
            walk,
            sleep: find_animation(SLEEP_ANIMATION)?,
            eat: anim::find(EAT_ANIMATION).unwrap_or(sit),
            play: anim::find(PLAY_ANIMATION).unwrap_or(sit),
            run: anim::find(RUN_ANIMATION).unwrap_or(walk),
        })
    }
}

/// The cat. It is handed the time & the bar's size, & says what should be in the bar.
#[derive(Clone, Debug)]
pub struct Cat {
    animations: Animations,
    schedule: Schedule,
    mode: Mode,
    /// Where the cat is, kept up to date while it moves.
    x: u32,
    facing: Facing,
    /// The last meal it started, so it does not eat the same one twice.
    last_meal: Option<Meal>,
    /// Makes the cat's choices. The same seed makes the same choices.
    rng: Rng,
}

impl Cat {
    /// A cat at the left end of the bar, as Meowsh starts. `seed` decides its choices.
    pub fn new(schedule: Schedule, seed: u64) -> Result<Self> {
        let animations = Animations::find()?;
        Ok(Self {
            animations,
            schedule,
            mode: free_time(animations, Duration::ZERO),
            x: HOME_X,
            facing: Facing::Right,
            last_meal: None,
            rng: Rng::with_seed(seed),
        })
    }

    /// Moves the cat on to `time`, in a bar `bar_width` art pixels wide, & says what should
    /// be in the bar.
    pub fn update(&mut self, time: Time, bar_width: u32) -> Scene {
        // The furthest right the cat's left edge can be, with the whole cat in the bar.
        // In a bar too narrow for that, it stays at the left end.
        let right_end = bar_width.saturating_sub(CANVAS.width).max(LEFT_END);
        let now = time.since_start;
        self.pick_mode(time);
        self.pick_rest(now, right_end);
        self.arrive_home(time);
        let (animation, frame) = match self.mode {
            Mode::Free(Rest::Moving(walk)) | Mode::GoingHome { walk, .. } => {
                let walking = walk.at(now);
                self.x = walking.x;
                self.facing = walking.facing;
                (walk.animation(), walking.frame)
            }
            Mode::Free(Rest::Staying { player, .. })
            | Mode::Eating { player, .. }
            | Mode::Sleeping(player) => {
                self.x = self.x.clamp(LEFT_END, right_end);
                (player.animation(), player.at(now).frame)
            }
        };
        Scene {
            pictures: vec![ScenePicture {
                animation: animation.name,
                frame,
                x: self.picture_x(),
                facing: self.facing,
            }],
        }
    }

    /// How long after `since_start` the scene changes, or `None` if it stays the same
    /// until something happens. A new part of the day is noticed at the next frame, which
    /// is soon, because every animation the cat plays loops.
    pub fn until_next_change(&self, since_start: Duration) -> Option<Duration> {
        match self.mode {
            Mode::Free(Rest::Moving(walk)) | Mode::GoingHome { walk, .. } => {
                walk.until_next_frame(since_start)
            }
            Mode::Free(Rest::Staying { player, until }) => {
                let done_in = until.saturating_sub(since_start);
                let next_frame = player.until_next_frame(since_start);
                Some(next_frame.map_or(done_in, |next_frame| next_frame.min(done_in)))
            }
            Mode::Eating { player, .. } | Mode::Sleeping(player) => {
                player.until_next_frame(since_start)
            }
        }
    }

    /// Changes the mode when the part of the day calls for another one.
    fn pick_mode(&mut self, time: Time) {
        let now = time.since_start;
        match (self.schedule.part_at(time.second_of_day()), self.mode) {
            (
                PartOfDay::Nap | PartOfDay::Night,
                Mode::Sleeping(_)
                | Mode::GoingHome {
                    then: AtHome::Sleep,
                    ..
                },
            ) => {}
            (PartOfDay::Nap | PartOfDay::Night, _) => self.go_home(AtHome::Sleep, time),
            (part @ (PartOfDay::Breakfast | PartOfDay::Dinner), mode) => {
                let meal = Meal {
                    part,
                    day: time.day(),
                };
                if self.last_meal != Some(meal) {
                    // The meal starts now, also when Meowsh opens partway through meal time.
                    self.last_meal = Some(meal);
                    self.go_home(AtHome::Eat, time);
                } else if let Mode::Eating { until, .. } = mode
                    && time.clock >= until
                {
                    self.mode = free_time(self.animations, now);
                }
            }
            (PartOfDay::Free, Mode::Free(_)) => {}
            (PartOfDay::Free, _) => self.mode = free_time(self.animations, now),
        }
    }

    /// Walks home to do `then` there, or starts it at once if the cat is already home.
    fn go_home(&mut self, then: AtHome, time: Time) {
        if self.x == HOME_X {
            self.start_at_home(then, time);
        } else {
            let walk = Walk::start(self.animations.walk, self.x, HOME_X, time.since_start);
            self.mode = Mode::GoingHome { walk, then };
        }
    }

    /// Starts what the cat went home for, once its walk home is done.
    fn arrive_home(&mut self, time: Time) {
        if let Mode::GoingHome { walk, then } = self.mode
            && walk.at(time.since_start).arrived
        {
            self.x = HOME_X;
            self.start_at_home(then, time);
        }
    }

    /// Eats or sleeps at home, facing out into the bar.
    fn start_at_home(&mut self, then: AtHome, time: Time) {
        let now = time.since_start;
        self.facing = Facing::Right;
        self.mode = match then {
            AtHome::Eat => Mode::Eating {
                player: Player::start(self.animations.eat, now),
                until: time.clock + MEAL_SECONDS,
            },
            AtHome::Sleep => Mode::Sleeping(Player::start(self.animations.sleep, now)),
        };
    }

    /// In free time, picks the next resting action once the one before is done.
    fn pick_rest(&mut self, now: Duration, right_end: u32) {
        let Mode::Free(rest) = self.mode else {
            return;
        };
        let done = match rest {
            Rest::Moving(walk) => {
                let walking = walk.at(now);
                self.x = walking.x;
                // After a resize, the spot it was moving to may be past the new end.
                walking.arrived || walk.destination() > right_end
            }
            Rest::Staying { until, .. } => now >= until,
        };
        if !done {
            return;
        }
        let from = self.x.clamp(LEFT_END, right_end);
        let rest = match action::pick(&mut self.rng) {
            Action::Sit => self.stay(self.animations.sit, now),
            Action::Play => self.stay(self.animations.play, now),
            Action::Walk => {
                let to = self.rng.u32(LEFT_END..=right_end);
                Rest::Moving(Walk::start(self.animations.walk, from, to, now))
            }
            Action::Run => {
                // A dash to whichever end is further away.
                let to = if from - LEFT_END < right_end - from {
                    right_end
                } else {
                    LEFT_END
                };
                Rest::Moving(Walk::start(self.animations.run, from, to, now))
            }
        };
        self.mode = Mode::Free(rest);
    }

    /// Where the cat's picture starts: at its spot facing right, or shifted facing left, so
    /// it turns in place.
    fn picture_x(&self) -> u32 {
        match self.facing {
            Facing::Right => self.x,
            Facing::Left => {
                let x = i64::from(self.x) + LEFT_FACING_SHIFT;
                // The cat stays right of `LEFT_END`, so this is never below 0.
                u32::try_from(x).unwrap_or(0)
            }
        }
    }

    /// Sitting or playing where the cat is, for a while.
    fn stay(&mut self, animation: &'static Animation, now: Duration) -> Rest {
        let seconds = self.rng.u64(STAY_SECONDS);
        Rest::Staying {
            player: Player::start(animation, now),
            until: now + Duration::from_secs(seconds),
        }
    }
}

/// The start of free time, at `now`.
fn free_time(animations: Animations, now: Duration) -> Mode {
    // Already done, so the next update picks a resting action.
    Mode::Free(Rest::Staying {
        player: Player::start(animations.sit, now),
        until: now,
    })
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

    /// The seed for the cats in these tests, so they make the same choices every run.
    const SEED: u64 = 7;

    fn cat() -> Result<Cat> {
        Cat::new(Schedule::default(), SEED)
    }

    #[test]
    fn in_free_time_the_cat_sits_and_moves_and_stays_in_the_bar() -> Result<()> {
        let mut cat = cat()?;
        let mut sat = false;
        let mut moved = false;
        // Ten minutes, a frame at a time.
        for tenth in 0..6000 {
            let time = Time {
                since_start: Duration::from_millis(tenth * 100),
                clock: MORNING,
            };
            let picture = update(&mut cat, time, BAR_WIDTH)?;
            assert!(picture.x <= 120, "the cat is at {}, past 120", picture.x);
            sat |= picture.animation == SIT_ANIMATION;
            moved |= picture.animation == WALK_ANIMATION;
        }
        assert!(sat && moved, "sat: {sat}, moved: {moved}");
        Ok(())
    }

    #[test]
    fn a_bar_made_smaller_keeps_the_cat_inside_it() -> Result<()> {
        let mut cat = cat()?;
        update(&mut cat, time(MORNING, 0), BAR_WIDTH)?;

        let picture = update(&mut cat, time(MORNING, 1), CANVAS.width + 60)?;
        assert!((LEFT_END..=60).contains(&cat.x), "the cat is at {}", cat.x);
        assert!(
            picture.x <= 60,
            "the picture starts at {}, past 60",
            picture.x
        );

        update(&mut cat, time(MORNING, 2), 10)?;
        assert_eq!(cat.x, LEFT_END);
        Ok(())
    }

    #[test]
    fn facing_left_the_picture_shifts_so_the_cat_turns_in_place() -> Result<()> {
        let mut cat = cat()?;
        cat.facing = Facing::Left;

        cat.x = 100;
        assert_eq!(i64::from(cat.picture_x()), 100 + LEFT_FACING_SHIFT);
        // At the left end, the picture starts right at the bar's edge.
        cat.x = LEFT_END;
        assert_eq!(cat.picture_x(), 0);
        Ok(())
    }

    /// Plays free time at `clock` a frame at a time until the cat is away from home, &
    /// says when that was.
    fn leave_home(cat: &mut Cat, clock: u64) -> Result<Duration> {
        for tenth in 0..6000 {
            let since_start = Duration::from_millis(tenth * 100);
            if update(cat, Time { since_start, clock }, BAR_WIDTH)?.x > HOME_X {
                return Ok(since_start);
            }
        }
        anyhow::bail!("the cat never left home")
    }

    /// Plays on a frame at a time from `since_start`, with the clock running 60 times
    /// faster from `clock`, until the cat is not walking home anymore. Returns the picture
    /// then & the time it was.
    fn walk_home(cat: &mut Cat, since_start: Duration, clock: u64) -> Result<(ScenePicture, Time)> {
        for tenth in 1..6000 {
            let time = Time {
                since_start: since_start + Duration::from_millis(tenth * 100),
                clock: clock + tenth * 6,
            };
            let picture = update(cat, time, BAR_WIDTH)?;
            if !matches!(cat.mode, Mode::GoingHome { .. }) {
                return Ok((picture, time));
            }
            assert_eq!(picture.facing, Facing::Left, "it faces home on the way");
        }
        anyhow::bail!("the cat never got home")
    }

    #[test]
    fn at_nap_time_the_cat_walks_home_and_sleeps_there_then_is_free_again() -> Result<()> {
        let mut cat = cat()?;
        let left = leave_home(&mut cat, MORNING)?;

        let (home, arrived) = walk_home(&mut cat, left, NAP_START)?;
        let later = |seconds: u64| Time {
            since_start: arrived.since_start + Duration::from_secs(seconds),
            clock: arrived.clock + seconds,
        };
        update(&mut cat, later(600), BAR_WIDTH)?;
        let still_asleep = matches!(cat.mode, Mode::Sleeping(_));
        update(&mut cat, later(2 * 60 * 60), BAR_WIDTH)?;

        let at_home = (SLEEP_ANIMATION, HOME_X, Facing::Right);
        assert_eq!((home.animation, home.x, home.facing), at_home);
        assert!(still_asleep);
        assert!(matches!(cat.mode, Mode::Free(_)));
        Ok(())
    }

    #[test]
    fn a_meal_lasts_its_time_from_when_the_cat_gets_home() -> Result<()> {
        let mut cat = cat()?;
        let left = leave_home(&mut cat, MORNING)?;

        let (home, arrived) = walk_home(&mut cat, left, BREAKFAST)?;

        assert_eq!((home.x, home.facing), (HOME_X, Facing::Right));
        let Mode::Eating { until, .. } = cat.mode else {
            anyhow::bail!("the cat should eat once home, not {:?}", cat.mode);
        };
        assert_eq!(until, arrived.clock + MEAL_SECONDS);
        Ok(())
    }

    #[test]
    fn each_meal_is_eaten_once_for_its_time_even_if_meowsh_opens_partway_through() -> Result<()> {
        let mut cat = cat()?;
        let mut eating_at = |seconds: u64| -> Result<bool> {
            update(&mut cat, time(BREAKFAST, seconds), BAR_WIDTH)?;
            Ok(matches!(cat.mode, Mode::Eating { .. }))
        };

        assert!(eating_at(0)?);
        assert!(eating_at(MEAL_SECONDS - 1)?);
        assert!(!eating_at(MEAL_SECONDS)?);
        assert!(!eating_at(MEAL_SECONDS + 600)?);
        assert!(eating_at(SECONDS_PER_DAY)?);
        Ok(())
    }
}
