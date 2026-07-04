//! Runtime scheduling window (Phase 5).
//!
//! An optional window during which downloads may auto-start. When no window is
//! configured the scheduler runs at all times.

use chrono::{NaiveTime, Weekday};

#[derive(Debug, Clone)]
pub struct ScheduleWindow {
    pub start: NaiveTime,
    pub end: NaiveTime,
    /// Allowed weekdays; empty means every day.
    pub days: Vec<Weekday>,
}

/// Is `now` (time + weekday) inside the window? Handles overnight windows
/// (e.g. 22:00–06:00) by treating `start > end` as wrapping past midnight.
pub fn is_within(time: NaiveTime, weekday: Weekday, win: &ScheduleWindow) -> bool {
    if !win.days.is_empty() && !win.days.contains(&weekday) {
        return false;
    }
    if win.start <= win.end {
        time >= win.start && time < win.end
    } else {
        time >= win.start || time < win.end
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Weekday::*;

    fn t(h: u32, m: u32) -> NaiveTime {
        NaiveTime::from_hms_opt(h, m, 0).unwrap()
    }

    #[test]
    fn same_day_window() {
        let w = ScheduleWindow {
            start: t(2, 0),
            end: t(6, 0),
            days: vec![],
        };
        assert!(is_within(t(3, 0), Mon, &w));
        assert!(!is_within(t(1, 0), Mon, &w));
        assert!(!is_within(t(6, 0), Mon, &w)); // end exclusive
    }

    #[test]
    fn overnight_window() {
        let w = ScheduleWindow {
            start: t(22, 0),
            end: t(6, 0),
            days: vec![],
        };
        assert!(is_within(t(23, 30), Fri, &w));
        assert!(is_within(t(5, 0), Fri, &w));
        assert!(!is_within(t(12, 0), Fri, &w));
    }

    #[test]
    fn day_restriction() {
        let w = ScheduleWindow {
            start: t(0, 0),
            end: t(23, 59),
            days: vec![Sat, Sun],
        };
        assert!(is_within(t(10, 0), Sat, &w));
        assert!(!is_within(t(10, 0), Mon, &w));
    }
}
