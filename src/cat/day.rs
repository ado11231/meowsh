//! The cat's day: turns the time of day into meal, nap, night, or free time.

/// Seconds in one day.
pub const SECONDS_PER_DAY: u64 = 24 * SECONDS_PER_HOUR as u64;

/// Seconds in one hour.
const SECONDS_PER_HOUR: u32 = 60 * 60;

/// A part of the cat's day.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PartOfDay {
    /// It eats from its bowl.
    Breakfast,
    /// It sleeps in the afternoon.
    Nap,
    /// It eats from its bowl.
    Dinner,
    /// It sleeps through the night.
    Night,
    /// It wanders the bar & picks a resting action.
    Free,
}

/// A stretch of the day, from `start` up to but not including `end`, in seconds since
/// midnight. When `end` comes before `start`, it runs past midnight, like the night.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stretch {
    /// The first second of the stretch.
    pub start: u32,
    /// The first second after the stretch.
    pub end: u32,
}

impl Stretch {
    /// The stretch from the start of hour `start` to the start of hour `end`.
    const fn hours(start: u32, end: u32) -> Self {
        Self {
            start: start * SECONDS_PER_HOUR,
            end: end * SECONDS_PER_HOUR,
        }
    }

    fn contains(self, second: u32) -> bool {
        if self.start <= self.end {
            self.start <= second && second < self.end
        } else {
            second >= self.start || second < self.end
        }
    }
}

/// When the cat eats & sleeps. Any other time is free time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Schedule {
    /// Breakfast time.
    pub breakfast: Stretch,
    /// The afternoon nap.
    pub nap: Stretch,
    /// Dinner time.
    pub dinner: Stretch,
    /// The night's sleep.
    pub night: Stretch,
}

impl Default for Schedule {
    /// The times from DESIGN.md.
    fn default() -> Self {
        Self {
            breakfast: Stretch::hours(7, 9),
            nap: Stretch::hours(13, 15),
            dinner: Stretch::hours(18, 19),
            night: Stretch::hours(23, 7),
        }
    }
}

impl Schedule {
    /// The part of the day at `second`, counted from midnight. When stretches overlap, the
    /// night wins, then the meals, then the nap.
    pub fn part_at(&self, second: u32) -> PartOfDay {
        let parts = [
            (self.night, PartOfDay::Night),
            (self.breakfast, PartOfDay::Breakfast),
            (self.dinner, PartOfDay::Dinner),
            (self.nap, PartOfDay::Nap),
        ];
        parts
            .into_iter()
            .find(|(stretch, _)| stretch.contains(second))
            .map_or(PartOfDay::Free, |(_, part)| part)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Seconds since midnight at `hour`:`minute`:`second`.
    fn at(hour: u32, minute: u32, second: u32) -> u32 {
        hour * SECONDS_PER_HOUR + minute * 60 + second
    }

    #[test]
    fn each_part_starts_on_its_hour_and_ends_just_before_the_next() {
        let schedule = Schedule::default();
        let cases = [
            (at(0, 0, 0), PartOfDay::Night),
            (at(6, 59, 59), PartOfDay::Night),
            (at(7, 0, 0), PartOfDay::Breakfast),
            (at(8, 59, 59), PartOfDay::Breakfast),
            (at(9, 0, 0), PartOfDay::Free),
            (at(13, 0, 0), PartOfDay::Nap),
            (at(15, 0, 0), PartOfDay::Free),
            (at(18, 30, 0), PartOfDay::Dinner),
            (at(19, 0, 0), PartOfDay::Free),
            (at(22, 59, 59), PartOfDay::Free),
            (at(23, 0, 0), PartOfDay::Night),
            (at(23, 59, 59), PartOfDay::Night),
        ];
        for (second, part) in cases {
            assert_eq!(schedule.part_at(second), part, "at second {second}");
        }
    }
}
