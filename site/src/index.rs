use crate::text::TEXT_HTML;
use actix_web::{HttpResponse, Responder};
use askama::Template;
use common::YearSeason;

#[derive(Template)]
#[template(path = "index.html")]
struct Index {
    season_name: String,
    season_link: String,
}

#[actix_web::get("/")]
pub async fn get() -> impl Responder {
    let season = YearSeason::current();
    let index = Index {
        season_name: season.display_name(),
        season_link: season.to_url_str(),
    };
    let index = index.render().unwrap();
    HttpResponse::Ok().content_type(TEXT_HTML).body(index)
}
