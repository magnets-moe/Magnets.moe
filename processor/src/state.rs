use crate::{
    anilist_client::AnilistClient,
    config::Config,
    db_state::{DbWatcher, WatchMessageHandler},
    show_db::ShowDbHolder,
};
use common::pg::{Dummy, PgConnector, PgHolder};
use std::sync::Arc;
use tokio::time::Instant;

pub struct State<'a> {
    pub pg: Arc<PgHolder<Dummy, WatchMessageHandler>>,
    pub show_db: ShowDbHolder,
    pub web_client: &'a reqwest::Client,
    pub anilist_client: AnilistClient<'a>,
    pub db_watcher: Arc<DbWatcher>,
    pub startup_time: Instant,
    pub pg_connector: PgConnector,
    pub config: &'a Config,
}
