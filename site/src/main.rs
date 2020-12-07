#![allow(clippy::eval_order_dependence)] // https://github.com/rust-lang/rust-clippy/issues/5684

#[macro_use]
mod torrent_list;
#[macro_use]
mod show_list;
mod cache;
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
    state::{Global, State},
};
use actix_files as fs;
use actix_web::{
    web::{PathConfig, QueryConfig},
    App, HttpServer,
};
use common::{pg::PgHolder, time::MINUTE};
use std::{io, sync::Arc};

#[actix_web::main]
async fn main() -> io::Result<()> {
    common::pg::set_name("site");
    common::env::configure_logger();

    let global = Arc::new(Global {
        shows: Cache::new(10 * MINUTE),
    });

    HttpServer::new(move || {
        let state = State {
            global: global.clone(),
            pg: PgHolder::new(),
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
    })
    .max_connections(1000)
    .bind("[::]:8080")?
    .run()
    .await
}
