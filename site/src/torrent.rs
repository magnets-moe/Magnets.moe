use crate::{
    state::State,
    text::{HexFormatter, MagnetFormatter, NotFound, TEXT_HTML},
};
use actix_web::{web, web::Data, HttpResponse, Responder};
use anyhow::Result;
use askama::Template;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use tokio_postgres::types::Json;

#[actix_web::get("/torrent/{torrent_id}")]
pub async fn get(state: Data<State>, id: web::Path<(i64,)>) -> impl Responder {
    match process(&state, id.0.0).await {
        Ok(data) => HttpResponse::Ok().content_type(TEXT_HTML).body(data),
        Err(e) => {
            if e.is::<NotFound>() {
                HttpResponse::NotFound().finish()
            } else {
                log::error!(
                    "An error occurred while trying to retrieve torrent {}: {:#}",
                    id.0.0,
                    e
                );
                HttpResponse::InternalServerError().finish()
            }
        }
    }
}

#[derive(Template)]
#[template(path = "torrent.html")]
struct Torrent<'a> {
    torrent_id: i64,
    nyaa_id: i64,
    title: &'a str,
    trusted: bool,
    date: DateTime<Utc>,
    magnet_link: MagnetFormatter<'a>,
    hash: HexFormatter<'a>,
    shows: Vec<Show>,
    size: i64,
}

mod filters {
    pub use crate::text::{format_full_time, format_size};
}

#[derive(Deserialize)]
struct Show {
    show_id: i64,
    name: String,
}

async fn process(state: &State, torrent_id: i64) -> Result<String> {
    // language=sql
    const QUERY: &str = r"
        select *,
               (
                   select coalesce(json_agg(x), '[]'::json)
                   from (
                       select rts.show_id, sn.name
                       from magnets.rel_torrent_show rts
                       join magnets.show_name sn using (show_id)
                       where rts.torrent_id = $1 and sn.show_name_type = 1
                   ) x
               ) as shows
        from magnets.torrent
        where torrent_id = $1;
    ";

    let db = state.pg.borrow().await?;
    let row = db.query_opt(QUERY, &[&torrent_id]).await?;
    let row = match row {
        Some(r) => r,
        _ => return Err(NotFound.into()),
    };
    let shows: Json<Vec<Show>> = row.get("shows");
    let title = row.get("title");
    let hash = row.get("hash");
    let torrent = Torrent {
        torrent_id,
        title,
        nyaa_id: row.get("nyaa_id"),
        trusted: row.get("trusted"),
        date: row.get("uploaded_at"),
        magnet_link: MagnetFormatter(title, hash),
        hash: HexFormatter(hash),
        shows: shows.0,
        size: row.get("size"),
    };
    Ok(torrent.render()?)
}
