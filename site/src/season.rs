use crate::{
    show_list::{show_list_from_rows, Letter},
    state::State,
};
use actix_web::{web, web::Data, HttpResponse, Responder};
use anyhow::Result;
use askama::Template;
use common::YearSeason;

#[actix_web::get("/season/{name}")]
pub async fn get(state: Data<State>, name: web::Path<(String,)>) -> impl Responder {
    let season = match YearSeason::from_url_str(&name.0.0) {
        Ok(s) => s,
        _ => return HttpResponse::NotFound().finish(),
    };
    match season_(state, season).await {
        Ok(b) => HttpResponse::Ok().content_type("text/html").body(b),
        Err(e) => {
            log::error!(
                "An error occurred while trying to retrieve season {}: {:#?}",
                season.display_name(),
                e
            );
            HttpResponse::InternalServerError().finish()
        }
    }
}

#[derive(Template)]
#[template(path = "season.html")]
struct Tpl<'a> {
    letters: &'a [Letter],
    json: &'a str,
    season_name: String,
    prev_season_link: String,
    next_season_link: String,
    prev_season_name: String,
    next_season_name: String,
}

async fn season_(state: Data<State>, season: YearSeason) -> Result<String> {
    let db = state.pg.borrow().await?;
    let rows = db.query(&db.t.season.stmt, &[&season.to_db()]).await?;
    let show_list = show_list_from_rows!(db.t.season, &rows);
    let tpl = Tpl {
        letters: &show_list.letters,
        json: &show_list.json,
        season_name: season.display_name(),
        prev_season_link: season.prev().to_url_str(),
        next_season_link: season.next().to_url_str(),
        prev_season_name: season.prev().display_name(),
        next_season_name: season.next().display_name(),
    };
    Ok(tpl.render()?)
}
