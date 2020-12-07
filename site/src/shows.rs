use crate::{
    cache::Cached,
    show_list::{show_list_from_rows, Letter},
    state::State,
    text::TEXT_HTML,
};
use actix_web::{
    http::header::{CacheControl, CacheDirective, CACHE_CONTROL},
    web::{Bytes, Data},
    HttpResponse, Responder,
};
use anyhow::Result;
use askama::Template;

#[actix_web::get("/shows")]
pub async fn get(state: Data<State>) -> impl Responder {
    match shows_(state).await {
        Ok(b) => {
            let bytes: Bytes = (*b).clone();
            let cc = CacheControl(vec![
                CacheDirective::MaxAge(b.max_age()),
                CacheDirective::Public,
            ]);
            HttpResponse::Ok()
                .header(CACHE_CONTROL, cc)
                .content_type(TEXT_HTML)
                .body(bytes)
        }
        Err(e) => {
            log::error!(
                "an error occurred while trying to retrieve all shows: {:#}",
                e
            );
            HttpResponse::InternalServerError().finish()
        }
    }
}

async fn shows_(state: Data<State>) -> Result<Cached<Bytes>> {
    state.global.shows.get(load_shows).await
}

#[derive(Template)]
#[template(path = "shows.html")]
struct Shows<'a> {
    letters: &'a [Letter],
    json: &'a str,
}

// language=sql
common::create_statement!(ShowsStmt, show_id, name, show_name_type; "
    select show_id, name, show_name_type
    from magnets.show_name
    where show_name_type in (1, 2)");

async fn load_shows() -> Result<Bytes> {
    let db = common::pg::connect().await?;
    let stmt = ShowsStmt::new(&db).await?;
    let rows = db.query(&stmt.stmt, &[]).await?;
    let show_list = show_list_from_rows!(stmt, &rows);
    let show = Shows {
        letters: &show_list.letters,
        json: &show_list.json,
    };
    Ok(show.render()?.into())
}
