use crate::{
    state::State,
    text::{NotFound, SqlParams, TEXT_HTML},
    torrent_list::{torrent_list_from_rows, Day},
};
use actix_web::{
    web,
    web::{Data, Query},
    HttpResponse, Responder,
};
use anyhow::Result;
use askama::Template;
use common::{Format, ShowNameType, YearSeason};
use serde::Deserialize;
use std::ops::Deref;
use tokio_postgres::types::Json;

#[actix_web::get("/show/{show_id}")]
pub async fn get(
    state: Data<State>,
    id: web::Path<(String,)>,
    Query(query): Query<QueryParams>,
) -> impl Responder {
    match process(&state, &id.0.0, query).await {
        Ok(data) => HttpResponse::Ok().content_type(TEXT_HTML).body(data),
        Err(e) => {
            if e.is::<NotFound>() {
                HttpResponse::NotFound().finish()
            } else {
                log::error!(
                    "An error occurred while trying to retrieve show {}: {:#}",
                    id.0.0,
                    e
                );
                HttpResponse::InternalServerError().finish()
            }
        }
    }
}

#[derive(Deserialize)]
struct Name {
    name: String,
    show_name_type: i32,
}

#[derive(Template)]
#[template(path = "show.html")]
struct Show<'a> {
    show_id: i64,
    anilist_id: i64,
    romaji: &'a str,
    english: Option<&'a str>,
    format: &'static str,
    season: Option<(String, String)>,
    days: &'a [Day<'a>],
    last: Option<i64>,
    first: bool,
}

mod filters {
    pub use crate::text::{format_day, format_time};
}

#[derive(Deserialize)]
pub struct QueryParams {
    #[serde(rename = "a", default = "i64::max_value")]
    after: i64,
}

async fn process(state: &State, id: &str, query: QueryParams) -> Result<String> {
    let show_id: i64 = match id.parse() {
        Ok(i) => i,
        _ => return Err(NotFound.into()),
    };
    let db = state.pg.borrow().await?;
    let (show_info_row, show_torrents_rows) = {
        let p1: SqlParams = &[&show_id];
        let a = db.query_opt(&db.t.show_info.stmt, p1);
        let p2: SqlParams = &[&show_id, &query.after];
        let b = db.query(&db.t.show_torrents.stmt, p2);
        futures::join!(a, b)
    };
    let show_info_row = match show_info_row? {
        Some(r) => r,
        _ => return Err(NotFound.into()),
    };
    let show_torrents_rows = show_torrents_rows?;
    let (last, days) = torrent_list_from_rows!(db.t.show_torrents, &show_torrents_rows);
    let names: Json<Vec<Name>> = show_info_row.get(db.t.show_info.names);
    let mut romaji = "";
    let mut english = None;
    for name in &names.0 {
        if name.show_name_type == ShowNameType::ROMAJI {
            romaji = &name.name;
        } else if name.show_name_type == ShowNameType::ENGLISH {
            english = Some(name.name.deref());
        }
    }
    let show = Show {
        show_id,
        anilist_id: show_info_row.get(db.t.show_info.anilist_id),
        romaji,
        english,
        format: Format::from_db(show_info_row.get(db.t.show_info.show_format))?.as_str(),
        season: {
            match show_info_row.get(db.t.show_info.season) {
                None => None,
                Some(ys) => {
                    let season = YearSeason::from_db(ys)?;
                    Some((season.display_name(), season.to_url_str()))
                }
            }
        },
        days: &days,
        last,
        first: query.after == i64::MAX,
    };
    Ok(show.render()?)
}
