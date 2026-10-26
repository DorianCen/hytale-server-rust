//! World clock: game time, day/night progression and pause state.
//!
//! Ported from `CoreServer/.../modules/time/WorldTimeResource.java`. Java keeps
//! the game time as an `Instant` and advances it with a segmented mapping: the
//! configured daytime and nighttime durations are *real* seconds, while the
//! world always spends [`DAYTIME_SECONDS`] of a day in daylight and
//! [`NIGHTTIME_SECONDS`] at night. When both durations follow Java's default
//! 60/40 split, one real second is exactly [`SECONDS_PER_DAY`] /
//! (`daytime + nighttime`) game seconds.
//!
//! The clock is independent of the server; `WorldThread` owns one instance and
//! persists its state through [`crate::world_config::WorldConfig::game_time`].

use std::fmt;

/// Seconds in one game day.
pub const SECONDS_PER_DAY: i64 = 86_400;
/// Nanoseconds in one second.
pub const NANOS_PER_SECOND: i128 = 1_000_000_000;
/// Nanoseconds in one game day.
pub const NANOS_PER_DAY: i128 = SECONDS_PER_DAY as i128 * NANOS_PER_SECOND;

/// Java `WorldTimeResource.ZERO_YEAR` (`0001-01-01T00:00:00Z`) in epoch seconds.
pub const ZERO_YEAR_SECONDS: i64 = -62_135_596_800;
/// Java `WorldTimeResource.MAX_TIME` in epoch seconds (year 2969).
pub const MAX_TIME_SECONDS: i64 = 31_553_789_759;
/// Java `WorldTimeResource.MAX_TIME` nanos (`99_999_999`).
pub const MAX_TIME_NANOS: i32 = 99_999_999;

/// Java `WorldTimeResource.DAYTIME_SECONDS` (60% of a day).
pub const DAYTIME_SECONDS: i32 = 51_840;
/// Java `WorldTimeResource.NIGHTTIME_SECONDS` (40% of a day).
pub const NIGHTTIME_SECONDS: i32 = 34_560;
/// Java `WorldTimeResource.SUNRISE_SECONDS` (half of the night: 04:48).
pub const SUNRISE_SECONDS: i32 = NIGHTTIME_SECONDS / 2;

/// Java `WorldConfig.DEFAULT_DAYTIME_DURATION_SECONDS` (28:48 real).
pub const DEFAULT_DAYTIME_DURATION_SECONDS: i32 = 1_728;
/// Java `WorldConfig.DEFAULT_NIGHTTIME_DURATION_SECONDS` (19:12 real).
pub const DEFAULT_NIGHTTIME_DURATION_SECONDS: i32 = 1_152;
/// Java `WorldConfig` default total moon phases.
pub const DEFAULT_TOTAL_MOON_PHASES: i8 = 5;
/// Java `WorldConfig.INITIAL_GAME_DAY_START_HOUR`.
pub const INITIAL_DAY_START_HOUR: i64 = 5;
/// Java `WorldConfig.INITIAL_GAME_DAY_START_MINS`.
pub const INITIAL_DAY_START_MINUTE: i64 = 30;

/// An absolute game instant with Java `Instant` range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GameInstant {
    /// Epoch seconds.
    pub seconds: i64,
    /// Nanosecond adjustment in `0..1_000_000_000`.
    pub nanos: i32,
}

impl GameInstant {
    /// Java `WorldTimeResource.ZERO_YEAR`.
    pub const ZERO_YEAR: Self = Self {
        seconds: ZERO_YEAR_SECONDS,
        nanos: 0,
    };
    /// Java `WorldTimeResource.MAX_TIME`.
    pub const MAX_TIME: Self = Self {
        seconds: MAX_TIME_SECONDS,
        nanos: MAX_TIME_NANOS,
    };

    /// Build a normalized instant.
    pub fn new(seconds: i64, nanos: i32) -> Self {
        Self::from_total_nanos(seconds as i128 * NANOS_PER_SECOND + i128::from(nanos))
    }

    /// Build a normalized instant from a total nanosecond count.
    pub fn from_total_nanos(total: i128) -> Self {
        let seconds = total.div_euclid(NANOS_PER_SECOND);
        let nanos = total.rem_euclid(NANOS_PER_SECOND) as i32;
        let seconds = seconds.clamp(i64::MIN as i128, i64::MAX as i128) as i64;
        Self { seconds, nanos }
    }

    /// Total nanoseconds since the Unix epoch.
    pub fn total_nanos(self) -> i128 {
        self.seconds as i128 * NANOS_PER_SECOND + i128::from(self.nanos)
    }

    /// Add nanoseconds, saturating at the `i64` seconds range.
    pub fn plus_nanos(self, nanos: i128) -> Self {
        Self::from_total_nanos(self.total_nanos().saturating_add(nanos))
    }

    /// Seconds since midnight UTC (always `0..86_400`).
    pub fn second_of_day(self) -> i32 {
        self.seconds.rem_euclid(SECONDS_PER_DAY) as i32
    }
}

impl Default for GameInstant {
    fn default() -> Self {
        // `0001-01-01T00:00:00Z`.
        Self::ZERO_YEAR
    }
}

impl fmt::Display for GameInstant {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&format_iso8601(*self))
    }
}

/// Day/night configuration used by [`WorldClock`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorldClockSettings {
    /// Real seconds spent in daylight.
    pub daytime_duration_seconds: i32,
    /// Real seconds spent at night.
    pub nighttime_duration_seconds: i32,
    /// Total moon phases (`WorldGameplayConfig.WorldConfig.TotalMoonPhases`).
    pub total_moon_phases: i8,
}

impl Default for WorldClockSettings {
    fn default() -> Self {
        Self {
            daytime_duration_seconds: DEFAULT_DAYTIME_DURATION_SECONDS,
            nighttime_duration_seconds: DEFAULT_NIGHTTIME_DURATION_SECONDS,
            total_moon_phases: DEFAULT_TOTAL_MOON_PHASES,
        }
    }
}

impl WorldClockSettings {
    /// Replace non-positive durations with Java's defaults.
    pub fn sanitized(self) -> Self {
        Self {
            daytime_duration_seconds: if self.daytime_duration_seconds > 0 {
                self.daytime_duration_seconds
            } else {
                DEFAULT_DAYTIME_DURATION_SECONDS
            },
            nighttime_duration_seconds: if self.nighttime_duration_seconds > 0 {
                self.nighttime_duration_seconds
            } else {
                DEFAULT_NIGHTTIME_DURATION_SECONDS
            },
            total_moon_phases: if self.total_moon_phases > 0 {
                self.total_moon_phases
            } else {
                DEFAULT_TOTAL_MOON_PHASES
            },
        }
    }
}

/// Snapshot of the clock sent to sessions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorldClockState {
    /// Game time, seconds since the Unix epoch.
    pub game_time_seconds: i64,
    /// Game time sub-second nanos.
    pub game_time_nanos: i32,
    /// Configured daytime duration.
    pub daytime_duration_seconds: i32,
    /// Configured nighttime duration.
    pub nighttime_duration_seconds: i32,
    /// Total moon phases.
    pub total_moon_phases: i8,
    /// Whether world time is paused.
    pub time_paused: bool,
}

impl Default for WorldClockState {
    fn default() -> Self {
        let clock = WorldClock::default();
        clock.state()
    }
}

/// One world's running clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorldClock {
    game_time: GameInstant,
    settings: WorldClockSettings,
    paused: bool,
}

impl Default for WorldClock {
    fn default() -> Self {
        Self::from_game_time(None, WorldClockSettings::default(), false)
    }
}

impl WorldClock {
    /// Build a clock from the persisted game time (or Java's 05:30 default),
    /// settings and pause flag.
    pub fn from_game_time(
        game_time: Option<GameInstant>,
        settings: WorldClockSettings,
        paused: bool,
    ) -> Self {
        Self {
            game_time: game_time.unwrap_or_else(default_game_time),
            settings: settings.sanitized(),
            paused,
        }
    }

    /// The configured settings.
    pub fn settings(&self) -> WorldClockSettings {
        self.settings
    }

    /// The current game instant.
    pub fn game_time(&self) -> GameInstant {
        self.game_time
    }

    /// Whether time is paused.
    pub fn is_paused(&self) -> bool {
        self.paused
    }

    /// Pause or resume progression. The state is kept across ticks and saves.
    pub fn set_paused(&mut self, paused: bool) {
        self.paused = paused;
    }

    /// Jump to an absolute instant, clamped to the Java range.
    pub fn set_game_time(&mut self, game_time: GameInstant) {
        self.game_time = clamp_instant(game_time);
    }

    /// Current in-game hour (`0..24`).
    pub fn current_hour(&self) -> i32 {
        self.game_time.second_of_day() / 3_600
    }

    /// Java `WorldTimeResource.getDayProgress()` (`0.0..1.0`).
    pub fn day_progress(&self) -> f64 {
        f64::from(self.game_time.second_of_day()) / f64::from(SECONDS_PER_DAY as i32)
    }

    /// Java `WorldTimeResource.scaledTime` (`0.0..1.0`).
    ///
    /// The fixed 60/40 split maps midnight to `0`, sunrise to `0.25`, midday
    /// to `0.5`, sunset to `0.75` and the following midnight to `1`.
    pub fn scaled_day_time(&self) -> f64 {
        let day_progress = f64::from(self.game_time.second_of_day());
        let day_duration = f64::from(DAYTIME_SECONDS);
        let half_night = f64::from(NIGHTTIME_SECONDS) * 0.5;
        if day_progress <= half_night {
            lerp(0.0, 0.25, day_progress / half_night)
        } else if day_progress - half_night <= day_duration {
            lerp(0.25, 0.75, (day_progress - half_night) / day_duration)
        } else {
            lerp(
                0.75,
                1.0,
                (day_progress - half_night - day_duration) / half_night,
            )
        }
    }

    /// Java `WorldTimeResource.isDayTimeWithinRange`; both bounds are
    /// normalized day progress (`hour / 24`).
    pub fn is_day_time_within_range(&self, min: f64, max: f64) -> bool {
        within_range(self.day_progress(), min, max)
    }

    /// Java `WorldTimeResource.isScaledDayTimeWithinRange`; both bounds are
    /// normalized scaled time (`hour / 24`).
    pub fn is_scaled_day_time_within_range(&self, min: f64, max: f64) -> bool {
        within_range(self.scaled_day_time(), min, max)
    }

    /// Java `WorldTimeResource.isMoonPhaseWithinRange`. A wrapped range
    /// (`min > max`) spans the configured moon cycle.
    pub fn is_moon_phase_within_range(&self, min: i32, max: i32) -> bool {
        let phase = i32::from(self.moon_phase());
        if min > max {
            (phase >= min && phase <= i32::from(self.settings.total_moon_phases))
                || (phase >= 0 && phase <= max)
        } else {
            phase >= min && phase <= max
        }
    }

    /// Java `WorldTimeResource` moon phase: the weekly phase swaps at midday.
    pub fn moon_phase(&self) -> i8 {
        let total = self.settings.total_moon_phases.max(1);
        let day = day_of_year(self.game_time.seconds);
        let week_day = (day - 1).rem_euclid(i64::from(total));
        let phase = if self.current_hour() < 12 {
            if week_day == 0 {
                i64::from(total) - 1
            } else {
                week_day - 1
            }
        } else {
            week_day
        };
        phase as i8
    }

    /// Java `WorldTimeResource.getSunlightFactor()`.
    ///
    /// The fixed 60/40 day split places sunrise at half of the night; the
    /// sunlight curve peaks at midday and bottoms out at night.
    pub fn sunlight_factor(&self) -> f32 {
        let day_progress = self.game_time.second_of_day() as f32;
        let half_night = (SECONDS_PER_DAY as f32 - DAYTIME_SECONDS as f32) * 0.5;
        let dawn_relative = (day_progress - half_night) / SECONDS_PER_DAY as f32;
        ((std::f32::consts::TAU * dawn_relative).sin() + 0.2).clamp(0.0, 1.0)
    }

    /// Snapshot for `UpdateTime`/`UpdateTimeSettings`.
    pub fn state(&self) -> WorldClockState {
        WorldClockState {
            game_time_seconds: self.game_time.seconds,
            game_time_nanos: self.game_time.nanos,
            daytime_duration_seconds: self.settings.daytime_duration_seconds,
            nighttime_duration_seconds: self.settings.nighttime_duration_seconds,
            total_moon_phases: self.settings.total_moon_phases,
            time_paused: self.paused,
        }
    }

    /// Advance the clock by `dt` **real** seconds.
    ///
    /// This is the Java `WorldTimeResource.tick` mapping: the day and night
    /// segments each consume a fixed share of the game day, so their rates
    /// differ when the configured durations are not a 60/40 split.
    pub fn tick(&mut self, dt: f64) {
        if self.paused || !dt.is_finite() || dt <= 0.0 {
            return;
        }

        let daytime = f64::from(self.settings.daytime_duration_seconds);
        let nighttime = f64::from(self.settings.nighttime_duration_seconds);
        let total = daytime + nighttime;
        let daytime_rate = f64::from(DAYTIME_SECONDS) / daytime;
        let nighttime_rate = f64::from(NIGHTTIME_SECONDS) / nighttime;

        let seconds_of_day = self.game_time.second_of_day();
        let x0 = if (SUNRISE_SECONDS..SUNRISE_SECONDS + DAYTIME_SECONDS).contains(&seconds_of_day) {
            f64::from(seconds_of_day - SUNRISE_SECONDS) / daytime_rate
        } else {
            let since_day_end = i32::rem_euclid(
                seconds_of_day - SUNRISE_SECONDS - DAYTIME_SECONDS,
                SECONDS_PER_DAY as i32,
            );
            daytime + f64::from(since_day_end) / nighttime_rate
        };
        let x1 = x0 + dt;

        // Whole day/night cycles consumed, plus the position within the
        // current cycle.
        let whole = (x1 / total).floor() - (x0 / total).floor();
        let m0 = x0.rem_euclid(total);
        let m1 = x1.rem_euclid(total);

        // Cumulative game seconds within a cycle.
        let game_seconds = |m: f64| {
            if m <= daytime {
                daytime_rate * m
            } else {
                f64::from(DAYTIME_SECONDS) + nighttime_rate * (m - daytime)
            }
        };
        let advance = whole * SECONDS_PER_DAY as f64 + (game_seconds(m1) - game_seconds(m0));
        let nanos = (advance * NANOS_PER_SECOND as f64) as i128;

        let old = self.game_time;
        let mut next = old.plus_nanos(nanos);
        let zero = GameInstant::ZERO_YEAR.total_nanos();
        let max = GameInstant::MAX_TIME.total_nanos();
        let next_total = next.total_nanos();
        if next_total < zero {
            // Java wraps through the opposite end of the supported range.
            next = GameInstant::from_total_nanos(max + old.total_nanos() - zero);
        } else if next_total > max {
            next = GameInstant::from_total_nanos(zero + max - old.total_nanos());
        }
        self.game_time = next;
    }
}

/// Java `MathUtil.lerp` (clamped to `0..1`).
fn lerp(from: f64, to: f64, t: f64) -> f64 {
    from + t.clamp(0.0, 1.0) * (to - from)
}

/// Java `MathUtil.within`, including a wrapped `min > max` range.
fn within_range(value: f64, min: f64, max: f64) -> bool {
    if min > max {
        (value >= min && value <= 1.0) || (value >= 0.0 && value <= max)
    } else {
        value >= min && value <= max
    }
}

fn clamp_instant(value: GameInstant) -> GameInstant {
    let zero = GameInstant::ZERO_YEAR.total_nanos();
    let max = GameInstant::MAX_TIME.total_nanos();
    let total = value.total_nanos().clamp(zero, max);
    GameInstant::from_total_nanos(total)
}

fn default_game_time() -> GameInstant {
    GameInstant::from_total_nanos(
        GameInstant::ZERO_YEAR.total_nanos()
            + i128::from(INITIAL_DAY_START_HOUR * 3_600 + INITIAL_DAY_START_MINUTE * 60)
                * NANOS_PER_SECOND,
    )
}

/// Parse a Java `Instant.toString()` value (`YYYY-MM-DDTHH:MM:SS[.fff][Z]`).
///
/// A trailing `Z`, `+HH:MM` or `-HH:MM` offset is accepted. The year must be
/// four digits (the range Java saves in `WorldConfig.GameTime`).
pub fn parse_iso8601(text: &str) -> Option<GameInstant> {
    let text = text.trim();
    let (date, time_and_offset) = text.split_once('T')?;
    let (date_parts, time_offset): (Vec<&str>, &str) = (date.split('-').collect(), time_and_offset);
    if date_parts.len() != 3 {
        return None;
    }
    let year: i64 = date_parts[0].parse().ok()?;
    let month: i64 = date_parts[1].parse().ok()?;
    let day: i64 = date_parts[2].parse().ok()?;

    let (clock, offset_seconds) = if let Some(value) = time_offset.strip_suffix('Z') {
        (value, 0i64)
    } else if let Some((value, offset)) = time_offset.rsplit_once('+') {
        (value, parse_offset(offset)?)
    } else if let Some((value, offset)) = time_offset.rsplit_once('-') {
        (value, -parse_offset(offset)?)
    } else {
        (time_offset, 0)
    };

    let mut clock_parts = clock.splitn(2, '.');
    let hms = clock_parts.next()?;
    let (hour, minute, second) = {
        let mut parts = hms.split(':');
        let hour: i64 = parts.next()?.parse().ok()?;
        let minute: i64 = parts.next()?.parse().ok()?;
        let second: i64 = parts.next()?.parse().ok()?;
        if parts.next().is_some() {
            return None;
        }
        (hour, minute, second)
    };
    if !(0..24).contains(&hour) || !(0..60).contains(&minute) || !(0..60).contains(&second) {
        return None;
    }
    let nanos = match clock_parts.next() {
        Some(fraction) => {
            if fraction.is_empty()
                || fraction.len() > 9
                || !fraction.bytes().all(|b| b.is_ascii_digit())
            {
                return None;
            }
            let mut value: i64 = fraction.parse().ok()?;
            for _ in fraction.len()..9 {
                value *= 10;
            }
            value
        }
        None => 0,
    };

    let days = days_from_civil(year, month, day);
    let seconds = days * SECONDS_PER_DAY + hour * 3_600 + minute * 60 + second - offset_seconds;
    Some(GameInstant::new(seconds, nanos as i32))
}

fn parse_offset(text: &str) -> Option<i64> {
    let (hour, minute) = text.split_once(':')?;
    let hour: i64 = hour.parse().ok()?;
    let minute: i64 = minute.parse().ok()?;
    if !(0..24).contains(&hour) || !(0..60).contains(&minute) {
        return None;
    }
    Some(hour * 3_600 + minute * 60)
}

/// Render an instant the way Java writes it in `WorldConfig.GameTime`.
pub fn format_iso8601(instant: GameInstant) -> String {
    let days = instant.seconds.div_euclid(SECONDS_PER_DAY);
    let seconds_of_day = instant.seconds.rem_euclid(SECONDS_PER_DAY);
    let (year, month, day) = civil_from_days(days);
    let hour = seconds_of_day / 3_600;
    let minute = (seconds_of_day % 3_600) / 60;
    let second = seconds_of_day % 60;

    let mut text = format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}");
    if instant.nanos != 0 {
        if instant.nanos % 1_000_000 == 0 {
            text.push_str(&format!(".{:03}", instant.nanos / 1_000_000));
        } else if instant.nanos % 1_000 == 0 {
            text.push_str(&format!(".{:06}", instant.nanos / 1_000));
        } else {
            text.push_str(&format!(".{:09}", instant.nanos));
        }
    }
    text.push('Z');
    text
}

/// Days since 1970-01-01 for a proleptic Gregorian date (Howard Hinnant).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let day_of_year = (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

/// Inverse of [`days_from_civil`].
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let days = days + 719_468;
    let era = if days >= 0 { days } else { days - 146_096 } / 146_097;
    let day_of_era = days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = if month_prime < 10 {
        month_prime + 3
    } else {
        month_prime - 9
    };
    if month <= 2 {
        year += 1;
    }
    (year, month, day)
}

/// Day of the year (`1..=366`) for a Unix timestamp.
fn day_of_year(seconds: i64) -> i64 {
    let days = seconds.div_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
    const MONTH_DAYS: [i64; 12] = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let mut ordinal = day;
    for (index, count) in MONTH_DAYS.iter().enumerate() {
        if index + 1 >= month as usize {
            break;
        }
        ordinal += count + i64::from(leap && index == 1);
    }
    ordinal
}

#[cfg(test)]
mod tests {
    use super::*;

    fn instant(text: &str) -> GameInstant {
        parse_iso8601(text).expect("parse instant")
    }

    #[test]
    fn default_time_is_java_initial_game_day() {
        let clock = WorldClock::default();
        assert_eq!(format_iso8601(clock.game_time()), "0001-01-01T05:30:00Z");
        assert_eq!(clock.state().daytime_duration_seconds, 1_728);
        assert_eq!(clock.state().nighttime_duration_seconds, 1_152);
        assert_eq!(clock.state().total_moon_phases, 5);
        assert!(!clock.state().time_paused);
    }

    #[test]
    fn iso_round_trip_matches_java_instant() {
        for text in [
            "0001-01-01T00:00:00Z",
            "0001-01-01T05:30:00Z",
            "2026-10-24T12:34:56Z",
            "2026-10-24T12:34:56.123Z",
            "2026-10-24T12:34:56.123456Z",
            "2026-10-24T12:34:56.123456789Z",
            "2026-10-24T12:34:56-05:00",
        ] {
            let parsed = instant(text);
            assert_eq!(
                parse_iso8601(&format_iso8601(parsed)),
                Some(parsed),
                "{text}"
            );
        }
        assert_eq!(
            instant("2026-10-24T12:34:56-05:00"),
            instant("2026-10-24T17:34:56Z")
        );
    }

    #[test]
    fn zero_year_is_midnight_utc() {
        assert_eq!(GameInstant::ZERO_YEAR.second_of_day(), 0);
        assert_eq!(instant("0001-01-01T05:30:00Z").second_of_day(), 19_800);
    }

    fn clock_at(hour: f64) -> WorldClock {
        let seconds = ZERO_YEAR_SECONDS + (hour * 3_600.0) as i64;
        WorldClock::from_game_time(
            Some(GameInstant::new(seconds, 0)),
            WorldClockSettings {
                total_moon_phases: 5,
                ..WorldClockSettings::default()
            },
            false,
        )
    }

    #[test]
    fn day_progress_and_scaled_time_match_java() {
        let midnight = clock_at(0.0);
        assert_eq!(midnight.day_progress(), 0.0);
        assert_eq!(midnight.scaled_day_time(), 0.0);

        // Java `halfNight` = 4:48, sunrise stays at scaled 0.25.
        let sunrise = clock_at(4.8);
        assert!((sunrise.scaled_day_time() - 0.25).abs() < 1e-9);
        let noon = clock_at(12.0);
        assert_eq!(noon.day_progress(), 0.5);
        assert!((noon.scaled_day_time() - 0.5).abs() < 1e-9);
        // The fixed 60/40 split puts sunset at 19:12, not 18:00.
        let sunset = clock_at(19.2);
        assert!((sunset.scaled_day_time() - 0.75).abs() < 1e-9);
    }

    #[test]
    fn day_time_and_moon_ranges_follow_java_wrapping() {
        let clock = clock_at(12.0);
        assert!(clock.is_day_time_within_range(6.0 / 24.0, 18.0 / 24.0));
        assert!(!clock.is_day_time_within_range(19.0 / 24.0, 5.0 / 24.0));
        assert!(clock_at(22.0).is_day_time_within_range(19.0 / 24.0, 5.0 / 24.0));
        assert!(!clock.is_day_time_within_range(0.0, 6.0 / 24.0));
        assert!(clock.is_scaled_day_time_within_range(0.25, 0.75));

        let phase = i32::from(clock.moon_phase());
        assert!(clock.is_moon_phase_within_range(phase, phase));
        assert!(clock.is_moon_phase_within_range(5, 0) || clock.is_moon_phase_within_range(0, 5));
        assert!(!clock.is_moon_phase_within_range(phase + 1, phase + 1));
    }

    #[test]
    fn current_hour_and_moon_phase_swap_at_midday() {
        let mut clock = WorldClock::from_game_time(
            Some(GameInstant::new(ZERO_YEAR_SECONDS + 3_600, 0)),
            WorldClockSettings {
                total_moon_phases: 5,
                ..WorldClockSettings::default()
            },
            false,
        );
        assert_eq!(clock.current_hour(), 1);
        let morning = clock.moon_phase();
        clock.set_game_time(GameInstant::new(ZERO_YEAR_SECONDS + 12 * 3_600, 0));
        assert_eq!(clock.current_hour(), 12);
        let noon = clock.moon_phase();
        // Day 1 at midday is phase 0; before midday it is the previous phase.
        assert_eq!(noon, 0);
        assert_eq!(morning, 4);
    }

    #[test]
    fn default_durations_advance_thirty_game_seconds_per_real_second() {
        let mut clock = WorldClock::default();
        let start = clock.game_time();
        clock.tick(1.0);
        assert_eq!(
            clock.game_time().total_nanos() - start.total_nanos(),
            i128::from(30) * NANOS_PER_SECOND
        );
        clock.tick(119.0);
        assert_eq!(format_iso8601(clock.game_time()), "0001-01-01T06:30:00Z");
    }

    #[test]
    fn a_full_real_day_returns_to_the_same_time_of_day() {
        let mut clock = WorldClock::default();
        let start = clock.game_time();
        clock.tick(2_880.0);
        assert_eq!(clock.game_time(), start.plus_nanos(NANOS_PER_DAY));
    }

    #[test]
    fn hour_rollover_crosses_midnight_cycle() {
        let mut clock = WorldClock::from_game_time(
            Some(instant("0001-01-01T05:30:00Z")),
            WorldClockSettings::default(),
            false,
        );
        // 120 real seconds = one game hour at the default rate.
        clock.tick(120.0);
        assert_eq!(format_iso8601(clock.game_time()), "0001-01-01T06:30:00Z");
        // 23 more game hours: one full day later, still 05:30.
        clock.tick(23.0 * 120.0);
        assert_eq!(format_iso8601(clock.game_time()), "0001-01-02T05:30:00Z");
    }

    #[test]
    fn pause_stops_and_resume_continues() {
        let mut clock = WorldClock::default();
        let paused_at = clock.game_time();
        clock.set_paused(true);
        clock.tick(600.0);
        assert_eq!(clock.game_time(), paused_at);
        assert!(clock.state().time_paused);

        clock.set_paused(false);
        clock.tick(1.0);
        assert_eq!(
            clock.game_time().total_nanos() - paused_at.total_nanos(),
            i128::from(30) * NANOS_PER_SECOND
        );
        assert!(!clock.state().time_paused);
    }

    #[test]
    fn duration_overrides_change_the_rate_but_not_the_day_length() {
        let mut clock = WorldClock::from_game_time(
            Some(instant("0001-01-01T00:00:00Z")),
            WorldClockSettings {
                daytime_duration_seconds: 100,
                nighttime_duration_seconds: 100,
                total_moon_phases: 5,
            },
            false,
        );
        // 00:00 is inside the night segment (sunset 19:12 .. sunrise 04:48).
        let start = clock.game_time();
        clock.tick(100.0);
        assert!(
            clock.game_time() > start,
            "time must move during the night segment"
        );
        // A full 200-real-second cycle is exactly one game day.
        let mut cycle = WorldClock::from_game_time(
            Some(instant("0001-01-01T00:00:00Z")),
            WorldClockSettings {
                daytime_duration_seconds: 100,
                nighttime_duration_seconds: 100,
                total_moon_phases: 5,
            },
            false,
        );
        cycle.tick(200.0);
        assert_eq!(
            cycle.game_time().total_nanos() - start.total_nanos(),
            NANOS_PER_DAY
        );
    }

    #[test]
    fn non_positive_durations_fall_back_to_defaults() {
        let settings = WorldClockSettings {
            daytime_duration_seconds: 0,
            nighttime_duration_seconds: -5,
            total_moon_phases: 0,
        }
        .sanitized();
        assert_eq!(settings, WorldClockSettings::default());
    }

    #[test]
    fn time_wraps_at_the_java_range_boundaries() {
        let max = GameInstant::MAX_TIME;
        let mut clock = WorldClock::from_game_time(Some(max), WorldClockSettings::default(), false);
        // 1 game second would cross MAX_TIME, so Java wraps to ZERO_YEAR plus
        // the tiny elapsed delta.
        clock.tick(1.0 / 30.0);
        assert!(clock.game_time() >= GameInstant::ZERO_YEAR);
        assert!(
            clock.game_time() < GameInstant::ZERO_YEAR.plus_nanos(NANOS_PER_SECOND),
            "unexpected wrap target: {}",
            clock.game_time()
        );
    }
}
