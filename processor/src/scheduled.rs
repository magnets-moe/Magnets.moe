use crate::state::State;
use anyhow::Result;
use chrono::{DateTime, Utc};
use common::time::{sleep_until, DurationFmt, StdDuration, MINUTE};
use futures::{
    future::{select, Either},
    pin_mut,
};
use std::time::SystemTime;
use tokio::sync::Notify;
use tokio_postgres::types::Json;

pub struct Scheduled<'a> {
    state: &'a State<'a>,
    key: &'static str,
    period: StdDuration,
}

impl<'a> Scheduled<'a> {
    pub fn new(state: &'a State, key: &'static str, period: StdDuration) -> Self {
        Self { state, key, period }
    }

    pub async fn wait(&self, n: &Notify) {
        while let Err(e) = self.wait_(n).await {
            log::error!(
                "cannot retrieve schedule information for key {}: {:#}",
                self.key,
                e
            );
            log::info!("sleeping for 5 minutes");
            tokio::time::delay_for(5 * MINUTE).await;
        }
    }

    async fn wait_(&self, n: &Notify) -> Result<()> {
        loop {
            let con = self.state.pg.borrow().await?;
            let last = {
                // language=sql
                let row = con
                    .query_one(
                        "select value from magnets.state where key = $1",
                        &[&self.key],
                    )
                    .await?;
                let last: Json<DateTime<Utc>> = row.get(0);
                SystemTime::from(last.0)
            };
            let notified = n.notified();
            let sleep = sleep_until(last + self.period);
            pin_mut!(notified, sleep);
            if let Either::Right(_) = select(notified, sleep).await {
                break;
            }
        }
        Ok(())
    }

    pub async fn update(&self) {
        if let Err(e) = self.update_().await {
            log::error!("cannot update schedule of {}: {:#}", self.key, e);
            log::info!("manually sleeping for {}", DurationFmt(self.period));
            tokio::time::delay_for(self.period).await;
        }
    }

    async fn update_(&self) -> Result<()> {
        let now = Utc::now();
        let pg = self.state.pg.borrow().await?;
        // language=sql
        pg.execute(
            "update magnets.state set value = $1 where key = $2",
            &[&Json(now), &self.key],
        )
        .await?;
        Ok(())
    }
}
