use chrono::Duration;
use std::{
    fmt,
    fmt::{Display, Formatter},
    time::SystemTime,
};

pub type StdDuration = std::time::Duration;

pub const MINUTE: StdDuration = StdDuration::from_secs(60);

pub const HOUR: StdDuration = StdDuration::from_secs(60 * 60);

/// Sleeps until `time`
pub async fn sleep_until(time: SystemTime) {
    if let Ok(t) = time.duration_since(SystemTime::now()) {
        tokio::time::delay_for(t).await;
    }
}

/// Formats a stdlib Duration like a chrono Duration
pub struct DurationFmt(pub StdDuration);

impl Display for DurationFmt {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match Duration::from_std(self.0) {
            Ok(d) => Display::fmt(&d, f),
            Err(_) => write!(f, "?"),
        }
    }
}
