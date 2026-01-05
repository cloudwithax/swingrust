//! Date and time utilities

use chrono::{DateTime, Datelike, Duration, Local, TimeZone, Utc};

/// Get Unix timestamp from N days ago
pub fn get_timestamp_days_ago(days: i64) -> i64 {
    let now = Utc::now();
    let past = now - Duration::days(days);
    past.timestamp()
}

/// Get Unix timestamp from N hours ago
pub fn get_timestamp_hours_ago(hours: i64) -> i64 {
    let now = Utc::now();
    let past = now - Duration::hours(hours);
    past.timestamp()
}

/// Get Unix timestamp from N minutes ago
pub fn get_timestamp_minutes_ago(minutes: i64) -> i64 {
    let now = Utc::now();
    let past = now - Duration::minutes(minutes);
    past.timestamp()
}

/// Format a timestamp as "YYYY-MM-DD HH:MM:SS"
pub fn format_datetime(timestamp: i64) -> String {
    let dt = DateTime::from_timestamp(timestamp, 0).unwrap_or_else(|| Utc::now());
    dt.format("%Y-%m-%d %H:%M:%S").to_string()
}

/// Convert timestamp to relative time string (e.g., "2 hours ago")
pub fn timestamp_to_relative(timestamp: i64) -> String {
    let dt = DateTime::from_timestamp(timestamp, 0).unwrap_or_else(|| Utc::now());
    chrono_humanize::HumanTime::from(dt).to_string()
}

/// Convert date string to relative time
pub fn date_to_relative(date_str: &str) -> String {
    if let Ok(dt) = DateTime::parse_from_str(date_str, "%Y-%m-%d %H:%M:%S") {
        chrono_humanize::HumanTime::from(dt.with_timezone(&Utc)).to_string()
    } else {
        date_str.to_string()
    }
}

/// Convert seconds to human-readable duration (e.g., "1 hr, 30 mins")
pub fn seconds_to_human_readable(seconds: i64) -> String {
    if seconds < 60 {
        return format!("{} sec", seconds);
    }

    let minutes = seconds / 60;
    if minutes < 60 {
        return format!("{} min", minutes);
    }

    let hours = minutes / 60;
    let remaining_minutes = minutes % 60;

    if hours < 24 {
        if remaining_minutes > 0 {
            format!("{} hr, {} min", hours, remaining_minutes)
        } else {
            format!("{} hr", hours)
        }
    } else {
        let days = hours / 24;
        let remaining_hours = hours % 24;
        if remaining_hours > 0 {
            format!("{} days, {} hr", days, remaining_hours)
        } else {
            format!("{} days", days)
        }
    }
}

/// Time period for statistics
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Period {
    Day,
    Week,
    Month,
    Year,
    AllTime,
}

impl Period {
    /// Get the start and end timestamps for this period
    pub fn get_range(&self) -> (i64, i64) {
        let now = Utc::now();
        let end = now.timestamp();

        let start = match self {
            Period::Day => (now - Duration::days(1)).timestamp(),
            Period::Week => (now - Duration::weeks(1)).timestamp(),
            Period::Month => (now - Duration::days(30)).timestamp(),
            Period::Year => (now - Duration::days(365)).timestamp(),
            Period::AllTime => 0,
        };

        (start, end)
    }

    /// Get seconds in this period
    pub fn seconds(&self) -> i64 {
        match self {
            Period::Day => 86400,
            Period::Week => 604800,
            Period::Month => 2592000, // 30 days
            Period::Year => 31536000, // 365 days
            Period::AllTime => i64::MAX,
        }
    }
}

/// Get the start of the current day
pub fn start_of_day() -> i64 {
    let now = Local::now();
    now.date_naive()
        .and_hms_opt(0, 0, 0)
        .map(|dt| Local.from_local_datetime(&dt).unwrap().timestamp())
        .unwrap_or(0)
}

/// Get the start of the current week (Monday)
pub fn start_of_week() -> i64 {
    let now = Local::now();
    let days_since_monday = now.weekday().num_days_from_monday() as i64;
    let monday = now - Duration::days(days_since_monday);

    monday
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .map(|dt| Local.from_local_datetime(&dt).unwrap().timestamp())
        .unwrap_or(0)
}

/// Get the start of the current month
pub fn start_of_month() -> i64 {
    let now = Local::now();
    now.date_naive()
        .with_day(1)
        .and_then(|d| d.and_hms_opt(0, 0, 0))
        .map(|dt| Local.from_local_datetime(&dt).unwrap().timestamp())
        .unwrap_or(0)
}

/// Get the start of the current year
pub fn start_of_year() -> i64 {
    let now = Local::now();
    now.date_naive()
        .with_month(1)
        .and_then(|d| d.with_day(1))
        .and_then(|d| d.and_hms_opt(0, 0, 0))
        .map(|dt| Local.from_local_datetime(&dt).unwrap().timestamp())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_seconds_to_human_readable() {
        assert_eq!(seconds_to_human_readable(30), "30 sec");
        assert_eq!(seconds_to_human_readable(120), "2 min");
        assert_eq!(seconds_to_human_readable(3600), "1 hr");
        assert_eq!(seconds_to_human_readable(5400), "1 hr, 30 min");
        assert_eq!(seconds_to_human_readable(86400), "1 days");
    }

    #[test]
    fn test_period_range() {
        let (start, end) = Period::Day.get_range();
        assert!(end > start);
        assert!(end - start <= 86400 + 1);
    }
}
