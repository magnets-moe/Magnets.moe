#![allow(clippy::eval_order_dependence)] // https://github.com/rust-lang/rust-clippy/issues/5684

#[macro_use]
mod torrent_list;
#[macro_use]
mod show_list;
mod cache;
mod config;
mod db;
mod faq;
mod index;
mod new;
mod schedule;
mod season;
mod show;
mod shows;
mod state;
mod text;
mod torrent;
mod unmatched;

use crate::{
    cache::Cache,
    config::{AddrType, Config},
    state::{Global, State},
};
use actix_files as fs;
use actix_web::{
    web::{PathConfig, QueryConfig},
    App, HttpServer,
};
use anyhow::Result;
use common::{
    pg::{PgConnector, PgHolder},
    time::MINUTE,
};
use std::sync::Arc;

#[actix_web::main]
async fn main() -> Result<()> {
    common::env::configure_logger();

    let config: Config = common::config::load()?;

    let pg_connector = PgConnector::new(config.db.connection_string.clone());

    let global = Arc::new(Global {
        shows: Cache::new(10 * MINUTE),
        pg_connector: pg_connector.clone(),
    });

    let mut server = HttpServer::new(move || {
        let state = State {
            global: global.clone(),
            pg: PgHolder::new(&pg_connector),
        };
        App::new()
            .data(state)
            .app_data(
                QueryConfig::default()
                    .error_handler(|_, _| actix_web::error::ErrorNotFound("")),
            )
            .app_data(
                PathConfig::default()
                    .error_handler(|_, _| actix_web::error::ErrorNotFound("")),
            )
            .service(fs::Files::new("/static", "static"))
            .service(schedule::get)
            .service(index::get)
            .service(shows::get)
            .service(season::get)
            .service(show::get)
            .service(unmatched::get)
            .service(torrent::get)
            .service(faq::get)
            .service(new::get)
    });
    for addr in &config.http.listen_addr {
        log::info!("binding to {}", addr);
        server = match addr {
            AddrType::Ip(addr) => server.bind(addr)?,
            #[cfg(unix)]
            AddrType::Uds(addr) => server.bind_uds(addr)?,
            #[cfg(not(unix))]
            AddrType::Uds(_) => log::warn!("skipping uds address"),
        };
    }
    server.run().await?;
    Ok(())
}
