use chrono::{Datelike, NaiveTime, Utc, Weekday};
use chrono_tz::America::New_York;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

use crate::config::SchedulesConfig;

/// Events emitted by the scheduler.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScheduleEvent {
    /// Time to generate a report. `report_type` matches config section names.
    GenerateReport {
        report_type: String,
        delivery_time_ms: i64,
    },
}

/// The scheduler: sleeps until the next scheduled event, then emits it.
pub struct Scheduler {
    config: SchedulesConfig,
}

impl Scheduler {
    pub fn new(config: SchedulesConfig) -> Self {
        Self { config }
    }

    /// Start the scheduler loop. Returns a receiver for schedule events.
    ///
    /// The scheduler runs in a spawned task and sends events on the channel
    /// when it's time to start generating a report (delivery_time - buffer).
    pub fn start(self) -> mpsc::Receiver<ScheduleEvent> {
        let (tx, rx) = mpsc::channel(16);
        tokio::spawn(async move {
            self.run_loop(tx).await;
        });
        rx
    }

    async fn run_loop(self, tx: mpsc::Sender<ScheduleEvent>) {
        loop {
            let now = Utc::now().with_timezone(&New_York);

            let next = match self.next_event(now) {
                Some(evt) => evt,
                None => {
                    warn!("scheduler: no upcoming events found, sleeping 60s");
                    tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                    continue;
                }
            };

            let delay = next.fire_at_utc - Utc::now();
            if delay.num_milliseconds() > 0 {
                debug!(
                    report_type = %next.report_type,
                    delay_seconds = delay.num_seconds(),
                    "scheduler: sleeping until next event"
                );
                tokio::time::sleep(delay.to_std().unwrap_or(std::time::Duration::from_secs(1)))
                    .await;
            }

            info!(report_type = %next.report_type, "scheduler: firing event");

            let event = ScheduleEvent::GenerateReport {
                report_type: next.report_type,
                delivery_time_ms: next.delivery_at_utc.timestamp_millis(),
            };

            if tx.send(event).await.is_err() {
                info!("scheduler: receiver dropped, shutting down");
                break;
            }
        }
    }

    /// Find the next event to fire, given the current time in ET.
    fn next_event(&self, now: chrono::DateTime<chrono_tz::Tz>) -> Option<PendingEvent> {
        let mut candidates = Vec::new();

        let today = now.date_naive();
        let weekday_name = weekday_to_string(now.weekday());
        let is_skip_day = self
            .config
            .weekly
            .skip_daily_on
            .iter()
            .any(|d| d.eq_ignore_ascii_case(&weekday_name));

        // Daily reports (skip on configured days)
        if !is_skip_day {
            for (name, schedule) in [
                ("morning", &self.config.morning),
                ("midday", &self.config.midday),
                ("evening", &self.config.evening),
            ] {
                if let Some(evt) = self.make_daily_event(name, schedule, today) {
                    candidates.push(evt);
                }
            }
        }

        // Also check tomorrow's daily reports (in case all today's are past)
        let tomorrow = today + chrono::Duration::days(1);
        let tomorrow_weekday = (now + chrono::Duration::days(1)).weekday();
        let tomorrow_skip = self
            .config
            .weekly
            .skip_daily_on
            .iter()
            .any(|d| d.eq_ignore_ascii_case(&weekday_to_string(tomorrow_weekday)));

        if !tomorrow_skip {
            for (name, schedule) in [
                ("morning", &self.config.morning),
                ("midday", &self.config.midday),
                ("evening", &self.config.evening),
            ] {
                if let Some(evt) = self.make_daily_event(name, schedule, tomorrow) {
                    candidates.push(evt);
                }
            }
        }

        // Weekly report
        if let Some(evt) = self.make_weekly_event(now) {
            candidates.push(evt);
        }

        // Return the earliest event that hasn't passed yet
        let now_utc = now.with_timezone(&Utc);
        candidates
            .into_iter()
            .filter(|e| e.fire_at_utc > now_utc)
            .min_by_key(|e| e.fire_at_utc)
    }

    fn make_daily_event(
        &self,
        name: &str,
        schedule: &crate::config::ReportSchedule,
        date: chrono::NaiveDate,
    ) -> Option<PendingEvent> {
        let time = parse_time(&schedule.delivery_time)?;
        let delivery_dt = date.and_time(time);
        let delivery_et = delivery_dt.and_local_timezone(New_York).earliest()?;
        let delivery_utc = delivery_et.with_timezone(&Utc);

        let fire_utc =
            delivery_utc - chrono::Duration::minutes(schedule.generation_buffer_minutes as i64);

        Some(PendingEvent {
            report_type: name.to_string(),
            fire_at_utc: fire_utc,
            delivery_at_utc: delivery_utc,
        })
    }

    fn make_weekly_event(&self, now: chrono::DateTime<chrono_tz::Tz>) -> Option<PendingEvent> {
        let weekly = &self.config.weekly;
        let target_weekday = parse_weekday(&weekly.day)?;
        let time = parse_time(&weekly.delivery_time)?;

        let today = now.date_naive();
        let current_weekday = now.weekday();
        let days_ahead = (target_weekday.num_days_from_monday() as i64
            - current_weekday.num_days_from_monday() as i64
            + 7)
            % 7;

        // If it's the same day, check if the time has passed
        let target_date = if days_ahead == 0 {
            let delivery_dt = today.and_time(time);
            let delivery_et = delivery_dt.and_local_timezone(New_York).earliest()?;
            let fire_utc = delivery_et.with_timezone(&Utc)
                - chrono::Duration::minutes(weekly.generation_buffer_minutes as i64);
            if fire_utc > now.with_timezone(&Utc) {
                today
            } else {
                today + chrono::Duration::days(7)
            }
        } else {
            today + chrono::Duration::days(days_ahead)
        };

        let delivery_dt = target_date.and_time(time);
        let delivery_et = delivery_dt.and_local_timezone(New_York).earliest()?;
        let delivery_utc = delivery_et.with_timezone(&Utc);
        let fire_utc =
            delivery_utc - chrono::Duration::minutes(weekly.generation_buffer_minutes as i64);

        Some(PendingEvent {
            report_type: "weekly".to_string(),
            fire_at_utc: fire_utc,
            delivery_at_utc: delivery_utc,
        })
    }

    /// Check if the current time is within overnight hours (ET).
    pub fn is_overnight(&self) -> bool {
        let now = Utc::now().with_timezone(&New_York);
        let current_time = now.time();

        let start = match parse_time(&self.config.overnight.start) {
            Some(t) => t,
            None => return false,
        };
        let end = match parse_time(&self.config.overnight.end) {
            Some(t) => t,
            None => return false,
        };

        // Overnight wraps midnight: 22:00 → 08:00
        if start > end {
            current_time >= start || current_time < end
        } else {
            current_time >= start && current_time < end
        }
    }
}

#[derive(Debug)]
struct PendingEvent {
    report_type: String,
    fire_at_utc: chrono::DateTime<Utc>,
    delivery_at_utc: chrono::DateTime<Utc>,
}

fn parse_time(s: &str) -> Option<NaiveTime> {
    NaiveTime::parse_from_str(s, "%H:%M").ok()
}

fn parse_weekday(s: &str) -> Option<Weekday> {
    match s.to_lowercase().as_str() {
        "monday" | "mon" => Some(Weekday::Mon),
        "tuesday" | "tue" => Some(Weekday::Tue),
        "wednesday" | "wed" => Some(Weekday::Wed),
        "thursday" | "thu" => Some(Weekday::Thu),
        "friday" | "fri" => Some(Weekday::Fri),
        "saturday" | "sat" => Some(Weekday::Sat),
        "sunday" | "sun" => Some(Weekday::Sun),
        _ => None,
    }
}

fn weekday_to_string(w: Weekday) -> String {
    match w {
        Weekday::Mon => "monday",
        Weekday::Tue => "tuesday",
        Weekday::Wed => "wednesday",
        Weekday::Thu => "thursday",
        Weekday::Fri => "friday",
        Weekday::Sat => "saturday",
        Weekday::Sun => "sunday",
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{OvernightConfig, ReportSchedule, WeeklySchedule};
    use chrono::{NaiveDate, Timelike};

    fn test_config() -> SchedulesConfig {
        SchedulesConfig {
            morning: ReportSchedule {
                delivery_time: "08:30".into(),
                generation_buffer_minutes: 10,
                description: "Morning".into(),
            },
            midday: ReportSchedule {
                delivery_time: "12:00".into(),
                generation_buffer_minutes: 10,
                description: "Midday".into(),
            },
            evening: ReportSchedule {
                delivery_time: "20:00".into(),
                generation_buffer_minutes: 10,
                description: "Evening".into(),
            },
            weekly: WeeklySchedule {
                delivery_time: "17:00".into(),
                day: "sunday".into(),
                generation_buffer_minutes: 15,
                description: "Weekly".into(),
                skip_daily_on: vec!["sunday".into()],
            },
            overnight: OvernightConfig {
                start: "22:00".into(),
                end: "08:00".into(),
            },
        }
    }

    #[test]
    fn parse_time_valid() {
        assert_eq!(
            parse_time("08:30"),
            Some(NaiveTime::from_hms_opt(8, 30, 0).unwrap())
        );
        assert_eq!(
            parse_time("20:00"),
            Some(NaiveTime::from_hms_opt(20, 0, 0).unwrap())
        );
    }

    #[test]
    fn parse_time_invalid() {
        assert_eq!(parse_time("invalid"), None);
    }

    #[test]
    fn parse_weekday_valid() {
        assert_eq!(parse_weekday("sunday"), Some(Weekday::Sun));
        assert_eq!(parse_weekday("Monday"), Some(Weekday::Mon));
        assert_eq!(parse_weekday("fri"), Some(Weekday::Fri));
    }

    #[test]
    fn next_event_returns_morning_on_weekday() {
        let scheduler = Scheduler::new(test_config());

        // Monday at 06:00 ET → morning should be next (fire at 08:20 ET)
        let monday = NaiveDate::from_ymd_opt(2025, 1, 6).unwrap();
        let dt = monday
            .and_hms_opt(6, 0, 0)
            .unwrap()
            .and_local_timezone(New_York)
            .earliest()
            .unwrap();

        let event = scheduler.next_event(dt).unwrap();
        assert_eq!(event.report_type, "morning");
    }

    #[test]
    fn next_event_skips_daily_on_sunday() {
        let scheduler = Scheduler::new(test_config());

        // Sunday at 06:00 ET → daily skipped, weekly at 16:45 is next
        let sunday = NaiveDate::from_ymd_opt(2025, 1, 5).unwrap();
        let dt = sunday
            .and_hms_opt(6, 0, 0)
            .unwrap()
            .and_local_timezone(New_York)
            .earliest()
            .unwrap();

        let event = scheduler.next_event(dt).unwrap();
        assert_eq!(event.report_type, "weekly");
    }

    #[test]
    fn next_event_after_evening_goes_to_tomorrow() {
        let scheduler = Scheduler::new(test_config());

        // Tuesday at 21:00 ET → all daily past, next is Wednesday morning
        let tuesday = NaiveDate::from_ymd_opt(2025, 1, 7).unwrap();
        let dt = tuesday
            .and_hms_opt(21, 0, 0)
            .unwrap()
            .and_local_timezone(New_York)
            .earliest()
            .unwrap();

        let event = scheduler.next_event(dt).unwrap();
        assert_eq!(event.report_type, "morning");
    }

    #[test]
    fn next_event_midday_after_morning() {
        let scheduler = Scheduler::new(test_config());

        // Wednesday at 09:00 ET → morning past, midday next
        let wednesday = NaiveDate::from_ymd_opt(2025, 1, 8).unwrap();
        let dt = wednesday
            .and_hms_opt(9, 0, 0)
            .unwrap()
            .and_local_timezone(New_York)
            .earliest()
            .unwrap();

        let event = scheduler.next_event(dt).unwrap();
        assert_eq!(event.report_type, "midday");
    }

    #[test]
    fn delivery_time_includes_buffer() {
        let scheduler = Scheduler::new(test_config());

        let monday = NaiveDate::from_ymd_opt(2025, 1, 6).unwrap();
        let dt = monday
            .and_hms_opt(6, 0, 0)
            .unwrap()
            .and_local_timezone(New_York)
            .earliest()
            .unwrap();

        let event = scheduler.next_event(dt).unwrap();
        let delivery_et = event.delivery_at_utc.with_timezone(&New_York);
        let fire_et = event.fire_at_utc.with_timezone(&New_York);
        assert_eq!(delivery_et.hour(), 8);
        assert_eq!(delivery_et.minute(), 30);
        assert_eq!(fire_et.hour(), 8);
        assert_eq!(fire_et.minute(), 20);
    }

    #[test]
    fn weekday_to_string_roundtrip() {
        for wd in [
            Weekday::Mon,
            Weekday::Tue,
            Weekday::Wed,
            Weekday::Thu,
            Weekday::Fri,
            Weekday::Sat,
            Weekday::Sun,
        ] {
            let s = weekday_to_string(wd);
            assert_eq!(parse_weekday(&s), Some(wd));
        }
    }
}
