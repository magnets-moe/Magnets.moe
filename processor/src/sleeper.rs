use std::time::{Duration, Instant};

pub struct Sleeper(Instant);

impl Sleeper {
    pub fn new() -> Self {
        Self(Instant::now())
    }

    pub async fn sleep(&mut self, duration: Duration) {
        let end = self.0 + duration;
        let now = Instant::now();
        self.0 = if now < end {
            tokio::time::delay_until(end.into()).await;
            end
        } else {
            now
        };
    }

    pub fn set_now(&mut self) {
        self.0 = Instant::now();
    }
}
