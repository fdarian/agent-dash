use chrono::{Local, TimeZone};

pub fn format_relative_time(now_epoch: i64, then_epoch: i64) -> String {
    let d = (now_epoch - then_epoch).max(0);
    if d < 60 {
        format!("{d}s")
    } else if d < 3600 {
        format!("{}m", d / 60)
    } else if d < 86400 {
        format!("{}h", d / 3600)
    } else if d < 604800 {
        format!("{}d", d / 86400)
    } else {
        format!("{}w", d / 604800)
    }
}

#[derive(PartialEq, Eq)]
pub enum ActivityBucket {
    Now,
    LastHour,
    Today,
    Yesterday,
    ThisWeek,
    Older,
}

impl ActivityBucket {
    pub fn label(&self) -> &'static str {
        match self {
            ActivityBucket::Now => "Now",
            ActivityBucket::LastHour => "Last hour",
            ActivityBucket::Today => "Today",
            ActivityBucket::Yesterday => "Yesterday",
            ActivityBucket::ThisWeek => "This week",
            ActivityBucket::Older => "Older",
        }
    }
}

fn local_date(epoch: i64) -> Option<chrono::NaiveDate> {
    Local
        .timestamp_opt(epoch, 0)
        .single()
        .map(|dt| dt.date_naive())
}

pub fn activity_bucket(now_epoch: i64, then_epoch: i64) -> ActivityBucket {
    let d = now_epoch - then_epoch;
    if d < 300 {
        ActivityBucket::Now
    } else if d < 3600 {
        ActivityBucket::LastHour
    } else {
        let now_date = match local_date(now_epoch) {
            Some(date) => date,
            None => return ActivityBucket::Older,
        };
        let then_date = match local_date(then_epoch) {
            Some(date) => date,
            None => return ActivityBucket::Older,
        };
        if then_date == now_date {
            ActivityBucket::Today
        } else if then_date == now_date.pred_opt().unwrap_or(now_date) {
            ActivityBucket::Yesterday
        } else if d < 604800 {
            ActivityBucket::ThisWeek
        } else {
            ActivityBucket::Older
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_relative_time_seconds() {
        assert_eq!(format_relative_time(1000, 950), "50s");
    }

    #[test]
    fn format_relative_time_minutes() {
        assert_eq!(format_relative_time(3600, 3000), "10m");
    }

    #[test]
    fn format_relative_time_hours() {
        assert_eq!(format_relative_time(10_000, 2800), "2h");
    }

    #[test]
    fn format_relative_time_days() {
        assert_eq!(format_relative_time(200_000, 86_400), "1d");
    }

    #[test]
    fn format_relative_time_weeks() {
        assert_eq!(format_relative_time(700_000, 0), "1w");
    }

    #[test]
    fn format_relative_time_clamps_negative_delta() {
        assert_eq!(format_relative_time(100, 200), "0s");
    }

    #[test]
    fn activity_bucket_now() {
        assert!(matches!(
            activity_bucket(10_000, 9_800),
            ActivityBucket::Now
        ));
    }

    #[test]
    fn activity_bucket_last_hour() {
        assert!(matches!(
            activity_bucket(10_000, 9_000),
            ActivityBucket::LastHour
        ));
    }

    #[test]
    fn activity_bucket_today_same_calendar_day() {
        let now = Local
            .with_ymd_and_hms(2026, 6, 6, 15, 0, 0)
            .unwrap()
            .timestamp();
        let then = Local
            .with_ymd_and_hms(2026, 6, 6, 8, 0, 0)
            .unwrap()
            .timestamp();
        assert!(matches!(
            activity_bucket(now, then),
            ActivityBucket::Today
        ));
    }

    #[test]
    fn activity_bucket_yesterday_boundary() {
        let now = Local
            .with_ymd_and_hms(2026, 6, 6, 1, 0, 0)
            .unwrap()
            .timestamp();
        let then = Local
            .with_ymd_and_hms(2026, 6, 5, 23, 0, 0)
            .unwrap()
            .timestamp();
        assert!(matches!(
            activity_bucket(now, then),
            ActivityBucket::Yesterday
        ));
    }

    #[test]
    fn activity_bucket_this_week() {
        let now = Local
            .with_ymd_and_hms(2026, 6, 8, 12, 0, 0)
            .unwrap()
            .timestamp();
        let then = Local
            .with_ymd_and_hms(2026, 6, 3, 12, 0, 0)
            .unwrap()
            .timestamp();
        assert!(matches!(
            activity_bucket(now, then),
            ActivityBucket::ThisWeek
        ));
    }

    #[test]
    fn activity_bucket_older() {
        let now = Local
            .with_ymd_and_hms(2026, 6, 8, 12, 0, 0)
            .unwrap()
            .timestamp();
        let then = Local
            .with_ymd_and_hms(2026, 5, 1, 12, 0, 0)
            .unwrap()
            .timestamp();
        assert!(matches!(activity_bucket(now, then), ActivityBucket::Older));
    }

    #[test]
    fn activity_bucket_labels() {
        assert_eq!(ActivityBucket::Now.label(), "Now");
        assert_eq!(ActivityBucket::LastHour.label(), "Last hour");
        assert_eq!(ActivityBucket::Today.label(), "Today");
        assert_eq!(ActivityBucket::Yesterday.label(), "Yesterday");
        assert_eq!(ActivityBucket::ThisWeek.label(), "This week");
        assert_eq!(ActivityBucket::Older.label(), "Older");
    }
}