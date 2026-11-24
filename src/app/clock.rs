//! The cat's clock: the real time of day, or a made up one from `MEOWSH_CLOCK` &
//! `MEOWSH_CLOCK_SPEED` for checking a whole day in minutes.

use crate::cat::Time;
use crate::env::{MEOWSH_CLOCK, MEOWSH_CLOCK_SPEED};
use anyhow::{Context, Result, ensure};
use jiff::civil;
use std::time::Duration;

/// Seconds in one minute.
const SECONDS_PER_MINUTE: u64 = 60;

/// Seconds in one hour.
const SECONDS_PER_HOUR: u64 = 60 * SECONDS_PER_MINUTE;

/// The cat's clock, which starts at a time of day & runs at a speed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Clock {
    /// Seconds since midnight when Meowsh started.
    start: u64,
    /// How many times faster than real time it runs.
    speed: u32,
}

impl Clock {
    /// The clock for this run: the real time of day, unless the test settings say
    /// otherwise.
    pub fn from_env() -> Result<Self> {
        let start = match std::env::var(MEOWSH_CLOCK) {
            Ok(text) => parse_start(&text)?,
            Err(_) => seconds_since_midnight(jiff::ZonedDateTime::now().time()),
        };
        let speed = match std::env::var(MEOWSH_CLOCK_SPEED) {
            Ok(text) => parse_speed(&text)?,
            Err(_) => 1,
        };
        Ok(Self { start, speed })
    }

    /// The time `since_start` after Meowsh started.
    pub fn time(self, since_start: Duration) -> Time {
        let run_ms = since_start
            .as_millis()
            .saturating_mul(u128::from(self.speed));
        // Far more seconds than any run of Meowsh, so it always fits.
        let run = u64::try_from(run_ms / 1000).unwrap_or(u64::MAX);
        Time {
            since_start,
            clock: self.start.saturating_add(run),
        }
    }
}

/// Reads a `MEOWSH_CLOCK` time such as `06:55`.
fn parse_start(text: &str) -> Result<u64> {
    let time: civil::Time = text
        .parse()
        .with_context(|| format!("{MEOWSH_CLOCK} should be a time such as 06:55, not {text}"))?;
    Ok(seconds_since_midnight(time))
}

/// Reads a `MEOWSH_CLOCK_SPEED` such as `60`.
fn parse_speed(text: &str) -> Result<u32> {
    let speed: u32 = text.parse().with_context(|| {
        format!("{MEOWSH_CLOCK_SPEED} should be a whole number such as 60, not {text}")
    })?;
    ensure!(speed > 0, "{MEOWSH_CLOCK_SPEED} should be at least 1");
    Ok(speed)
}

fn seconds_since_midnight(time: civil::Time) -> u64 {
    // Hours, minutes & seconds are never below 0.
    let part = |value: i8| u64::try_from(value).unwrap_or(0);
    part(time.hour()) * SECONDS_PER_HOUR
        + part(time.minute()) * SECONDS_PER_MINUTE
        + part(time.second())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_made_up_clock_starts_at_its_time_and_runs_at_its_speed() -> Result<()> {
        let clock = Clock {
            start: parse_start("06:55")?,
            speed: parse_speed("60")?,
        };

        let time = clock.time(Duration::from_secs(10));

        // 10 seconds at 60 times faster is 10 minutes, so it is 07:05.
        assert_eq!(time.clock, 7 * SECONDS_PER_HOUR + 5 * SECONDS_PER_MINUTE);
        assert_eq!(time.since_start, Duration::from_secs(10));
        Ok(())
    }

    #[test]
    fn bad_test_settings_are_errors() {
        assert!(parse_start("7 am").is_err());
        assert!(parse_speed("0").is_err());
        assert!(parse_speed("fast").is_err());
    }
}
