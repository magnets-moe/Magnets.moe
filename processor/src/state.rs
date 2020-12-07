use crate::{
    anilist_client::AnilistClient,
    db_state::{DbWatcher, WatchMessageHandler},
    show_db::ShowDbHolder,
};
use common::pg::{Dummy, PgHolder};
use std::sync::Arc;
use tokio::time::Instant;

pub struct State<'a> {
    pub pg: Arc<PgHolder<Dummy, WatchMessageHandler>>,
    pub show_db: ShowDbHolder,
    pub web_client: &'a reqwest::Client,
    pub anilist_client: AnilistClient<'a>,
    pub db_watcher: Arc<DbWatcher>,
    pub startup_time: Instant,
}
