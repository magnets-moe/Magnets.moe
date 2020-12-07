use crate::{
    db::Statements,
    state::State,
    text::{searchable_text, TEXT_HTML},
};
use actix_web::{
    http::header::{CacheControl, CacheDirective, CACHE_CONTROL},
    web::Data,
    HttpResponse, Responder,
};
use anyhow::Result;
use askama::Template;
use chrono::{DateTime, Datelike, Duration, Timelike, Utc};
use common::pg::Pg;
use serde::{Deserialize, Serialize};
use tokio_postgres::types::Json;

#[derive(Deserialize)]
struct Name {
    name: String,
    show_name_type: i32,
}

#[derive(Serialize)]
struct ScheduleItem {
    schedule_id: i64,
    show_id: i64,
    episode: i32,
    name: String,
}

#[derive(Serialize)]
struct HtmlEntry {
    timestamp: i64,
    air_time: String,
    showing_data: Option<ScheduleItem>,
}

#[derive(Serialize)]
struct ShowingJson {
    element_id: i64,
    names: Vec<String>,
}

#[derive(Serialize)]
struct Day<T> {
    name: &'static str,
    elements: Vec<T>,
    always_visible: bool,
}

impl<T> Day<T> {
    pub fn new(name: &'static str) -> Self {
        Self {
            name,
            elements: vec![],
            always_visible: false,
        }
    }
}

struct TimeRange {
    now: DateTime<Utc>,
    now_ts: i64,
    num_days_from_monday: usize,
    yesterday: DateTime<Utc>,
    end_of_week: DateTime<Utc>,
}

impl TimeRange {
    pub fn new() -> Self {
        let now = Utc::now();
        let now_ts = now.timestamp();
        let today = now.date().and_hms(0, 0, 0);
        let yesterday = today - Duration::days(1);
        let end_of_week = today + Duration::days(6);
        Self {
            now,
            now_ts,
            num_days_from_monday: now.date().weekday().num_days_from_monday() as usize,
            yesterday,
            end_of_week,
        }
    }
}

const WEEKDAYS: &[&str] = &[
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
    "Sunday",
];

#[actix_web::get("/schedule")]
pub async fn get(state: Data<State>) -> impl Responder {
    match get_(state).await {
        Ok(b) => {
            let cc = CacheControl(vec![
                CacheDirective::MaxAge(10 * 60),
                CacheDirective::Public,
            ]);
            HttpResponse::Ok()
                .header(CACHE_CONTROL, cc)
                .content_type(TEXT_HTML)
                .body(b)
        }
        Err(e) => {
            log::error!(
                "An error occurred while trying to retrieve the schedule: {:#?}",
                e
            );
            HttpResponse::InternalServerError().finish()
        }
    }
}

#[derive(Template)]
#[template(path = "schedule.html")]
struct Tpl<'a> {
    days: &'a [&'a Day<HtmlEntry>],
    json: &'a str,
}

async fn get_(state: Data<State>) -> anyhow::Result<String> {
    let client = state.pg.borrow().await?;

    let time_range = TimeRange::new();

    let (mut html_days, json_days) = collect_shedules(&client, &time_range).await?;

    insert_current_time(&mut html_days, &time_range);
    let arranged_days = arrange_days(&html_days, &time_range);

    let json = serde_json::to_string(&json_days)?;

    let tpl = Tpl {
        days: &arranged_days,
        json: &json,
    };

    Ok(tpl.render()?)
}

async fn collect_shedules(
    pg: &Pg<Statements>,
    times: &TimeRange,
) -> Result<(Vec<Day<HtmlEntry>>, Vec<Day<ShowingJson>>)> {
    let mut html_days: Vec<_> = WEEKDAYS.iter().copied().map(Day::new).collect();
    let mut json_days: Vec<_> = WEEKDAYS.iter().copied().map(Day::new).collect();

    json_days[times.num_days_from_monday].always_visible = true;

    let rows = pg
        .query(&pg.t.schedule.stmt, &[&times.yesterday, &times.end_of_week])
        .await?;
    for row in rows {
        let names: Json<Vec<Name>> = row.get(pg.t.schedule.names);
        let time: DateTime<Utc> = row.get(pg.t.schedule.airs_at);
        let schedule_id = row.get(pg.t.schedule.schedule_id);
        let item = HtmlEntry {
            timestamp: time.timestamp(),
            air_time: format_time(&time),
            showing_data: Some(ScheduleItem {
                schedule_id,
                show_id: row.get(pg.t.schedule.show_id),
                episode: row.get(pg.t.schedule.episode),
                name: names
                    .0
                    .iter()
                    .find(|n| n.show_name_type == 1)
                    .unwrap()
                    .name
                    .clone(),
            }),
        };
        let json_item = ShowingJson {
            element_id: schedule_id,
            names: names.0.iter().map(|n| searchable_text(&n.name)).collect(),
        };
        let date = time.date().weekday().num_days_from_monday() as usize;
        html_days[date].elements.push(item);
        json_days[date].elements.push(json_item);
    }

    Ok((html_days, json_days))
}

fn insert_current_time(html: &mut Vec<Day<HtmlEntry>>, times: &TimeRange) {
    let today_el = &mut html[times.num_days_from_monday];
    let mut idx = today_el.elements.len();
    for (i, entry) in today_el.elements.iter().enumerate() {
        if entry.timestamp >= times.now_ts {
            idx = i;
            break;
        }
    }
    today_el.elements.insert(
        idx,
        HtmlEntry {
            timestamp: times.now_ts,
            air_time: format_time(&times.now),
            showing_data: None,
        },
    )
}

fn arrange_days<'a>(
    html: &'a [Day<HtmlEntry>],
    times: &TimeRange,
) -> Vec<&'a Day<HtmlEntry>> {
    let start_idx = times.now.date().weekday().pred().num_days_from_monday() as usize;
    html[start_idx..].iter().chain(&html[..start_idx]).collect()
}

fn format_time(t: &DateTime<Utc>) -> String {
    let time = t.time();
    format!("{:02}:{:02}", time.hour(), time.minute())
}
