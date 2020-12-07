use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use common::pg::{MessageHandler, PgClient};
use paste::paste;
use serde::{Deserialize, Serialize};
use std::{fmt::Debug, sync::Arc};
use tokio::sync::Notify;
use tokio_postgres::{types::Json, GenericClient};

macro_rules! states {
    ($($id:ident,)*) => {
        paste! {
            $(pub const [<$id:upper>]: &'static str = stringify!($id);)*
        }
    }
}

macro_rules! w {
    ($($id:ident,)*) => {
        pub struct DbWatcher {
            $(pub $id: Notify,)*
        }

        impl DbWatcher {
            pub fn new() -> Arc<Self> {
                Arc::new(Self {
                    $($id: Notify::new(),)*
                })
            }

            pub fn notify_all(&self) {
                $(self.$id.notify();)*
            }

            pub fn handle_str(&self, s: &str) {
                let n = match s {
                    $(paste!([<$id:upper>]) => &self.$id,)*
                    _ => {
                        log::warn!("received unknown state change: {}", s);
                        return;
                    }
                };
                log::info!("received state change of row {}", s);
                n.notify();
            }
        }
    }
}

states! {
    max_nyaa_si_id,
    rematch_unmatched,
    last_shows_update,
    last_schedule_update,
    initial_setup,
}

w! {
    max_nyaa_si_id,
    rematch_unmatched,
    last_shows_update,
    last_schedule_update,
}

impl DbWatcher {
    pub fn message_handler(self: &Arc<Self>) -> WatchMessageHandler {
        WatchMessageHandler {
            watcher: self.clone(),
        }
    }
}

/// Sets a value in `magnets.state`
pub async fn set<P: GenericClient, T: Serialize + Debug + Sync + Clone>(
    p: &P,
    key: &str,
    value: T,
) -> Result<()> {
    p.execute(
        // language=sql
        "update magnets.state set value = $1 where key = $2",
        &[&Json(value.clone()), &key],
    )
    .await
    .with_context(|| anyhow!("cannot set database state of {} to {:?}", key, value))?;
    Ok(())
}

/// Retrieves a value from `magnets.state`
pub async fn get<P: GenericClient, T: for<'a> Deserialize<'a>>(
    p: &P,
    key: &str,
) -> Result<T> {
    let res: Json<T> = p
        // language=sql
        .query_one("select value from magnets.state where key = $1", &[&key])
        .await
        .with_context(|| anyhow!("cannot retrieve database state of {}", key))?
        .get(0);
    Ok(res.0)
}

#[derive(Clone)]
pub struct WatchMessageHandler {
    watcher: Arc<DbWatcher>,
}

#[async_trait]
impl MessageHandler for WatchMessageHandler {
    async fn listen(&self, client: &PgClient) -> Result<()> {
        client
            .simple_query("listen state_change")
            .await
            .context("could not execute `listen state_change`")?;
        self.watcher.notify_all();
        Ok(())
    }

    fn handle(&self, channel: &str, payload: &str) {
        assert_eq!(channel, "state_change");
        self.watcher.handle_str(payload);
    }
}
