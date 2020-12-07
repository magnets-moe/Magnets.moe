use crate::text::TEXT_HTML;
use actix_web::{HttpResponse, Responder};
use askama::Template;

#[derive(Template)]
#[template(path = "faq.html")]
struct Faq;

#[actix_web::get("/faq")]
pub async fn get() -> impl Responder {
    let faq = Faq.render().unwrap();
    HttpResponse::Ok().content_type(TEXT_HTML).body(faq)
}
