//! Session expiration logic, 4:00 AM calculation, and background cleanup scheduler.

use std::time::Duration;

use chrono::{DateTime, Local, TimeZone};

use crate::db::Database;
use crate::error::Result;

/// Calculates the previous 4:00 AM (local time) relative to `now`.
///
/// In Alhalte specification:
/// Every day at 4:00 AM local time, all sessions that had no heartbeat since the
/// previous 4:00 AM are deleted.
///
/// If `now` is at or around 4:00 AM (within a 5-minute execution window up to 04:05:00),
/// the current 4:00 AM is being serviced, so the *previous* 4:00 AM is yesterday's 4:00 AM.
/// If `now` is later in the day (e.g. 14:00), the previous 4:00 AM was earlier today at 04:00:00.
#[must_use]
pub fn previous_4am_local(now: DateTime<Local>) -> DateTime<Local> {
    let naive = now.naive_local();
    let four_am_today = naive.date().and_hms_opt(4, 0, 0).unwrap_or(naive);
    let four_five_am_today = naive.date().and_hms_opt(4, 5, 0).unwrap_or(naive);

    let cutoff_date = if naive <= four_five_am_today {
        naive.date() - chrono::Duration::days(1)
    } else {
        naive.date()
    };

    let cutoff_naive = cutoff_date.and_hms_opt(4, 0, 0).unwrap_or(four_am_today);
    Local
        .from_local_datetime(&cutoff_naive)
        .single()
        .unwrap_or(now)
}

/// Deletes all sessions in `db` whose `last_heartbeat` was strictly before `cutoff`.
///
/// Returns the number of expired sessions removed.
pub fn cleanup_sessions_with_cutoff(db: &Database, cutoff: DateTime<Local>) -> Result<usize> {
    db.cleanup_sessions_before(cutoff.timestamp())
}

/// Deterministically evaluates and performs session cleanup for reference timestamp `now`.
///
/// Computes `cutoff = previous_4am_local(now)` and deletes sessions older than the cutoff.
pub fn cleanup_sessions_at(db: &Database, now: DateTime<Local>) -> Result<usize> {
    let cutoff = previous_4am_local(now);
    cleanup_sessions_with_cutoff(db, cutoff)
}

/// Calculates the duration to wait until the next occurrence of 4:00:00 AM local time.
#[must_use]
pub fn duration_until_next_4am(now: DateTime<Local>) -> Duration {
    let naive = now.naive_local();
    let today_4am = naive.date().and_hms_opt(4, 0, 0).unwrap_or(naive);

    let target_naive = if naive < today_4am {
        today_4am
    } else {
        let tomorrow = naive.date() + chrono::Duration::days(1);
        tomorrow.and_hms_opt(4, 0, 0).unwrap_or(today_4am)
    };

    let target_dt = Local
        .from_local_datetime(&target_naive)
        .single()
        .unwrap_or(now);
    let diff = target_dt.signed_duration_since(now);
    if diff.num_seconds() > 0 {
        Duration::from_secs(diff.num_seconds() as u64)
    } else {
        Duration::from_secs(1)
    }
}

/// Starts the background task that triggers session cleanup every day at 4:00 AM local time.
pub fn spawn_session_cleanup_task(db: Database) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            let wait_dur = duration_until_next_4am(Local::now());
            tracing::info!("Next 4:00 AM session cleanup scheduled in {:?}", wait_dur);
            tokio::time::sleep(wait_dur).await;

            let now = Local::now();
            match cleanup_sessions_at(&db, now) {
                Ok(count) => {
                    tracing::info!(
                        "4:00 AM session cleanup completed: deleted {count} expired sessions"
                    );
                }
                Err(e) => {
                    tracing::error!("Error during 4:00 AM session cleanup: {e:?}");
                }
            }

            // Sleep a small duration to avoid re-triggering within the same second
            tokio::time::sleep(Duration::from_secs(60)).await;
        }
    })
}
