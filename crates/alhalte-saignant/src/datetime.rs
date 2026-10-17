//! Timestamp formatting and calendar conversion utilities.

/// Formats supported for log timestamps.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TimestampFormat {
    /// Machine-readable and standard ISO 8601 representation (e.g. `1970-01-01T00:00:00Z`).
    #[default]
    Iso8601,
    /// Japanese localized human-readable representation (e.g. `1970年1月1日0時0分0秒`).
    Japanese,
    /// Standard space-separated date and time representation (e.g. `1970-01-01 00:00:00`).
    Standard,
}

/// Converts UNIX timestamp seconds into Gregorian civil calendar date and time:
/// `(year, month, day, hour, minute, second)` in UTC.
///
/// Implements Howard Hinnant's algorithm for proleptic Gregorian calendar calculations.
pub fn unix_secs_to_datetime(secs: u64) -> (i64, u8, u8, u8, u8, u8) {
    let second = (secs % 60) as u8;
    let total_minutes = secs / 60;
    let minute = (total_minutes % 60) as u8;
    let total_hours = total_minutes / 60;
    let hour = (total_hours % 24) as u8;
    let days = (total_hours / 24) as i64;

    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = (yoe as i64) + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if m <= 2 { y + 1 } else { y };

    (year, m as u8, d as u8, hour, minute, second)
}

/// Formats UNIX timestamp seconds as an ISO 8601 string in UTC (e.g. `1970-01-01T00:00:00Z`).
pub fn format_iso8601(secs: u64) -> String {
    let (y, m, d, hh, mm, ss) = unix_secs_to_datetime(secs);
    format!("{y:04}-{m:02}-{d:02}T{hh:02}:{mm:02}:{ss:02}Z")
}

/// Formats UNIX timestamp seconds in Japanese notation (e.g. `1970年1月1日0時0分0秒`).
pub fn format_japanese(secs: u64) -> String {
    let (y, m, d, hh, mm, ss) = unix_secs_to_datetime(secs);
    format!("{y}年{m}月{d}日{hh}時{mm}分{ss}秒")
}

/// Formats UNIX timestamp seconds as standard space-separated date and time (e.g. `1970-01-01 00:00:00`).
pub fn format_standard(secs: u64) -> String {
    let (y, m, d, hh, mm, ss) = unix_secs_to_datetime(secs);
    format!("{y:04}-{m:02}-{d:02} {hh:02}:{mm:02}:{ss:02}")
}

/// Formats UNIX timestamp seconds according to the specified [`TimestampFormat`].
pub fn format_timestamp(secs: u64, format: TimestampFormat) -> String {
    match format {
        TimestampFormat::Iso8601 => format_iso8601(secs),
        TimestampFormat::Japanese => format_japanese(secs),
        TimestampFormat::Standard => format_standard(secs),
    }
}
