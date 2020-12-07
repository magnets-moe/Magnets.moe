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

#[derive(Deserialize)]
pub struct QueryParams {
    #[serde(rename = "a", default = "i64::max_value")]
    after: i64,
}

#[actix_web::get("/unmatched")]
pub async fn get(
    state: Data<State>,
    Query(params): Query<QueryParams>,
) -> impl Responder {
    match get_(params.after, state).await {
        Ok(s) => HttpResponse::Ok().content_type(TEXT_HTML).body(s),
        Err(e) => {
            log::error!(
                "An error occurred while trying to fetch unmatched torrents: {:?}",
                e
            );
            HttpResponse::InternalServerError().finish()
        }
    }
}

#[derive(Template)]
#[template(path = "unmatched.html")]
struct Days<'a> {
    days: &'a [Day<'a>],
    last: Option<i64>,
    first: bool,
}

mod filters {
    pub use crate::text::{format_day, format_time};
}

async fn get_(a: i64, state: Data<State>) -> Result<String> {
    let db = state.pg.borrow().await?;
    let rows = db.query(&db.t.unmatched.stmt, &[&a]).await?;
    let (last, days) = torrent_list_from_rows!(db.t.unmatched, &rows);
    let template = Days {
        last,
        days: &days,
        first: a == i64::MAX,
    };
    Ok(template.render()?)
}
