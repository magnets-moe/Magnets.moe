#![allow(clippy::eval_order_dependence)] // https://github.com/rust-lang/rust-clippy/issues/5684

#[cfg(target_os = "linux")]
mod allocator;
mod anilist;
mod config;
mod db_state;
mod diff;
mod heap;
mod http;
mod matcher;
mod nyaa;
mod scheduled;
mod show_db;
mod sleeper;
mod state;
mod strings;
mod title_analyzer;
mod trie;

use crate::{
    anilist::{
        client::AnilistClient,
        schedule::load_schedule,
        shows::{load_shows, load_shows_now},
    },
    config::Config,
    db_state::{DbWatcher, INITIAL_SETUP, LAST_SHOWS_UPDATE},
    matcher::match_unmatched,
    nyaa::load_torrents,
    show_db::ShowDbHolder,
    state::State,
};
use anyhow::Result;
use chrono::Utc;
use common::pg::{PgConnector, PgHolder};
use tokio::time::Instant;

pub fn processor() -> Result<()> {
    common::env::configure_logger();

    // Running our application in a thread reduces memory usage (glibc)
    std::thread::spawn(processor_in_thread).join().unwrap()?;
    Ok(())
}

pub fn diff() -> Result<()> {
    diff::diff()
}

fn processor_in_thread() -> Result<()> {
    tokio::runtime::Builder::new()
        .basic_scheduler()
        .enable_all()
        .build()
        .unwrap()
        .block_on(process())?;
    Ok(())
}

async fn process() -> Result<()> {
    let config: Config = common::config::load()?;
    let db_watcher = DbWatcher::new();
    let web_client = http::reqwest_client(&config.http.user_agent);
    let pg_connector = PgConnector::new(config.db.connection_string.clone());
    let state = State {
        pg: PgHolder::with_message_handler(
            db_watcher.message_handler(),
            true,
            &pg_connector,
        ),
        show_db: ShowDbHolder::new(&pg_connector),
        web_client: &web_client,
        anilist_client: AnilistClient::new(&web_client),
        db_watcher,
        startup_time: Instant::now(),
        pg_connector,
        config: &config,
    };
    initial_setup(&state).await?;
    let analyze_unmatched = match_unmatched(&state);
    let load_schedule = load_schedule(&state);
    let load_torrents = load_torrents(&state);
    let load_shows = load_shows(&state);
    futures::join!(analyze_unmatched, load_schedule, load_torrents, load_shows,);
    Ok(())
}

async fn initial_setup(state: &State<'_>) -> Result<()> {
    let pg = state.pg.borrow().await?;
    let initial_setup: bool = db_state::get(&**pg, INITIAL_SETUP).await?;
    if initial_setup {
        load_shows_now(state).await?;
        db_state::set(&**pg, LAST_SHOWS_UPDATE, Utc::now()).await?;
        db_state::set(&**pg, INITIAL_SETUP, false).await?;
    }
    Ok(())
}

#[cfg(test)]
mod test {}
