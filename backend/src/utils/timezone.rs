use chrono::{DateTime, Utc};
use chrono_tz::{Asia::Bangkok, Tz};

/// Bangkok timezone constant
pub const BANGKOK_TZ: Tz = Bangkok;

/// Get current time in Bangkok timezone
pub fn bangkok_now() -> DateTime<Tz> {
    Utc::now().with_timezone(&BANGKOK_TZ)
}

/// Get current time in Bangkok timezone as RFC3339 string
pub fn bangkok_now_rfc3339() -> String {
    bangkok_now().to_rfc3339()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Offset;

    #[test]
    fn test_bangkok_timezone() {
        let bangkok_time = bangkok_now();
        let _utc_time = Utc::now();

        // Bangkok should be 7 hours ahead of UTC
        let diff = bangkok_time.offset().fix().local_minus_utc();
        assert_eq!(diff, 7 * 3600); // 7 hours in seconds
    }

    #[test]
    fn test_rfc3339_format() {
        let rfc3339_string = bangkok_now_rfc3339();
        // Should be a valid RFC3339 string with timezone
        assert!(rfc3339_string.contains("+07:00"));
    }
}
