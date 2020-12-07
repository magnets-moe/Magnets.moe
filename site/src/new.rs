use crate::{
    state::State,
    text::TEXT_HTML,
    torrent_list::{torrent_list_from_rows, Day},
};
use actix_web::{
    web::{Data, Query},
    HttpResponse, Responder,
};
use anyhow::Result;
use askama::Template;
use serde::Deserialize;

#[actix_web::get("/new")]
pub async fn get(state: Data<State>, Query(query): Query<QueryParams>) -> impl Responder {
    match process(&state, query).await {
        Ok(data) => HttpResponse::Ok().content_type(TEXT_HTML).body(data),
        Err(e) => {
            log::error!("an error occurred while trying to load new shows: {:#}", e);
            HttpResponse::InternalServerError().finish()
        }
    }
}

#[derive(Template)]
#[template(path = "new.html")]
struct Days<'a> {
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

async fn process(state: &State, query: QueryParams) -> Result<String> {
    let db = state.pg.borrow().await?;
    let rows = db.query(&db.t.new.stmt, &[&query.after]).await?;
    let (last, days) = torrent_list_from_rows!(db.t.new, &rows);
    let days = Days {
        days: &days,
        last,
        first: query.after == i64::MAX,
    };
    Ok(days.render()?)
}
