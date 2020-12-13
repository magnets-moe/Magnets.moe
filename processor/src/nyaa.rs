use crate::{
    db_state, db_state::MAX_NYAA_SI_ID, sleeper::Sleeper, state::State, title_analyzer,
};
use anyhow::{anyhow, Context, Result};
use common::pg;
use rust_decimal::{prelude::ToPrimitive, Decimal};
use scraper::{ElementRef, Html, Selector};
use selectors::Element;
use std::{ops::Not, time, time::SystemTime};
use time::Duration;
use tokio::time::timeout;
use tokio_postgres::Transaction;
use unicode_normalization::UnicodeNormalization;
use url::Url;

type LocalName = html5ever::LocalName;

lazy_static::lazy_static! {
    static ref ROWS: Selector = Selector::parse(".torrent-list > tbody > tr").unwrap();
    static ref TITLE_LINK: Selector = Selector::parse("td:nth-child(2) > a:not(.comments)").unwrap();
    static ref MAGNET_LINK: Selector = Selector::parse("a > i.fa-magnet").unwrap();
    static ref SIZE_FIELD: Selector = Selector::parse("td:nth-child(4)").unwrap();
    static ref TIMESTAMP_FIELD: Selector = Selector::parse("td:nth-child(5)").unwrap();
}

fn get_unique_element<'a>(
    e: &ElementRef<'a>,
    selector: &Selector,
) -> Result<ElementRef<'a>> {
    let mut iter = e.select(selector);
    let first = match iter.next() {
        Some(f) => f,
        _ => return Err(anyhow!("selector matches no element")),
    };
    if iter.next().is_some() {
        return Err(anyhow!("selector matches multiple elements"));
    }
    Ok(first)
}

pub async fn load_torrents(state: &State<'_>) {
    loop {
        let _ = timeout(
            state.config.nyaa.scrape_interval,
            state.db_watcher.max_nyaa_si_id.notified(),
        )
        .await;
        log::info!("scraping nyaa.si");
        if let Err(e) = load_torrents_(state).await {
            log::error!("could not load torrents: {:#}", e);
        }
    }
}

async fn load_torrents_(state: &State<'_>) -> Result<()> {
    let con = state.pg.borrow().await?;
    let max_nyaa_id: i64 = db_state::get(&**con, MAX_NYAA_SI_ID).await?;
    let mut torrents = vec![];
    let mut sleeper = Sleeper::new();
    for i in 1..=100 {
        if i > 1 {
            log::info!("loading page {}", i);
        }
        let mut new = vec![];
        scrape_page(&state.web_client, &mut new, i).await?;
        let saw_existing = new
            .iter()
            .any(|t| t.nyaa_id.saturating_add(74) <= max_nyaa_id);
        torrents.extend(new);
        if saw_existing {
            break;
        }
        sleeper.sleep(Duration::from_secs(1)).await;
    }
    if torrents.is_empty() {
        return Ok(());
    }
    // fetch show_db before opening the transaction so that all shows in the db are
    // visible to the transaction
    let show_db = state.show_db.get().await?;
    let mut con = state.pg_connector.connect().await?;
    let tran = pg::transaction(&mut con).await?;
    torrents.sort_by_key(|t| t.nyaa_id);
    for torrent in &mut torrents {
        insert_torrent(&tran, torrent).await?;
    }
    for torrent in &torrents {
        if let Some(torrent_id) = torrent.torrent_id {
            match title_analyzer::find_show(&show_db, &torrent.title) {
                Ok(s) => crate::matcher::insert_match(&tran, torrent_id, &s).await?,
                Err(e) => {
                    log::error!("could not match torrent {}: {:#}", torrent.title, e);
                }
            }
        }
    }
    // language=sql
    tran.execute(
        "
        update magnets.state set value = (
            select max(nyaa_id) from magnets.torrent
        )::text::jsonb where key = $1",
        &[&MAX_NYAA_SI_ID],
    )
    .await?;
    tran.commit().await?;
    Ok(())
}

async fn scrape_page(
    client: &reqwest::Client,
    torrents: &mut Vec<Torrent>,
    page_no: u32,
) -> Result<()> {
    let url = format!("https://nyaa.si/?f=0&c=1_2&p={}", page_no);
    let response = client
        .get(&url)
        .send()
        .await
        .context("cannot communicate with nyaa.si")?;
    if response.status().as_u16() != 200 {
        return Err(anyhow!("nyaa.si status code is {}", response.status()));
    }
    // if let Some(cache) = response.headers().get("x-proxy-cache") {
    //     log::info!("x-proxy-cache: {:?}", cache);
    // }
    // if let Some(cache) = response.headers().get("date") {
    //     log::info!("date: {:?}", cache);
    // }
    let content = response
        .text()
        .await
        .context("cannot read nyaa.si response")?;
    let html = Html::parse_document(&content);
    for (i, torrent) in html.select(&ROWS).enumerate() {
        let torrent = parse_row(&torrent).with_context(|| {
            format!("cannot parse torrent number {} on {}", i + 1, url)
        })?;
        torrents.push(torrent);
    }
    Ok(())
}

async fn insert_torrent(tran: &Transaction<'_>, torrent: &mut Torrent) -> Result<()> {
    // language=sql
    let have = tran
        .query_one(
            "select count(*) as count from magnets.torrent where nyaa_id = $1",
            &[&torrent.nyaa_id],
        )
        .await?
        .get::<_, i64>(0)
        > 0;
    if have {
        return Ok(());
    }
    log::info!("inserting new torrent {}", torrent.title);
    // language=sql
    let row = tran
        .query_one(
            "
                insert into magnets.torrent
                (nyaa_id, hash, hash_type, uploaded_at, title, size, trusted)
                values ($1, $2, $3, $4, $5, $6, $7)
                returning torrent_id",
            &[
                &torrent.nyaa_id,
                &torrent.hash,
                &1i32,
                &torrent.timestamp,
                &torrent.title,
                &torrent.size,
                &torrent.trusted,
            ],
        )
        .await?;
    torrent.torrent_id = Some(row.get(0));
    Ok(())
}

#[derive(Debug)]
struct Torrent {
    torrent_id: Option<i64>,
    title: String,
    hash: Vec<u8>,
    nyaa_id: i64,
    trusted: bool,
    size: i64,
    timestamp: SystemTime,
}

fn parse_row(torrent: &ElementRef) -> Result<Torrent> {
    let title_link =
        get_unique_element(&torrent, &TITLE_LINK).context("cannot extract title link")?;

    let trusted = torrent
        .value()
        .classes
        .contains(&LocalName::from("success"));

    let title: String = title_link.text().flat_map(|t| t.nfc()).collect();

    let nyaa_id = {
        const URL_PREFIX: &str = "/view/";
        let nyaa_url = title_link
            .value()
            .attr("href")
            .context("title link does not contain a href attribute")?
            .to_owned();
        if nyaa_url.starts_with(URL_PREFIX).not() {
            return Err(anyhow!(
                "nyaa link does not start with prefix: {}",
                nyaa_url
            ));
        }
        nyaa_url[URL_PREFIX.len()..]
            .parse()
            .with_context(|| format!("nyaa id is out of bounds: {}", nyaa_url))?
    };

    let hash = {
        const TOPIC_PREFIX: &str = "urn:btih:";
        let magnet_link = get_unique_element(&torrent, &MAGNET_LINK)
            .context("cannot extract magnet link")?;
        let magnet_link = magnet_link
            .parent_element()
            .unwrap()
            .value()
            .attr("href")
            .context("magnet link does not contain a href attribute")?;
        let url = Url::parse(magnet_link).with_context(|| {
            format!("magnet link is not a valid url: {}", magnet_link)
        })?;
        let topic = url
            .query_pairs()
            .find(|e| e.0 == "xt")
            .map(|e| e.1)
            .with_context(|| {
                format!(
                    "magnet link does not contain an xt parameter: {}",
                    magnet_link
                )
            })?;
        if topic.starts_with(TOPIC_PREFIX).not() {
            return Err(anyhow!(
                "topic does not start with bittorent prefix: {}",
                topic
            ));
        }
        let hash = &topic[TOPIC_PREFIX.len()..];
        hex::decode(hash).with_context(|| format!("hash is not hex: {}", hash))?
    };

    let size = {
        let size = get_unique_element(&torrent, &SIZE_FIELD)
            .context("cannot extract size field")?;
        let size: String = size.text().collect();
        parse_size(&size).context("cannot parse size")?
    };

    let timestamp = {
        let timestamp = get_unique_element(&torrent, &TIMESTAMP_FIELD)
            .context("cannot extract timestamp field")?;
        let timestamp = timestamp
            .value()
            .attr("data-timestamp")
            .context("timestamp field does not contain a data-timestamp attribute")?;
        let timestamp: u64 = timestamp
            .parse()
            .with_context(|| anyhow!("timestamp is invalid: {}", timestamp))?;
        SystemTime::UNIX_EPOCH
            .checked_add(Duration::from_secs(timestamp))
            .context("timestamp is out of bounds")?
    };

    Ok(Torrent {
        title,
        hash,
        nyaa_id,
        trusted,
        size,
        timestamp,
        torrent_id: None,
    })
}

fn parse_size(s: &str) -> Result<i64> {
    let (num, unit) = s.split_at(
        s.find(' ')
            .with_context(|| format!("missing unit: {}", s))?,
    );
    let num: Decimal = num.trim().parse()?;
    let multiplier: i64 = match &*unit.trim().to_ascii_lowercase() {
        "" | "b" => 1,
        "ki" | "kib" => 1024,
        "mi" | "mib" => 1024 * 1024,
        "gi" | "gib" => 1024 * 1024 * 1024,
        "ti" | "tib" => 1024 * 1024 * 1024 * 1024,
        "k" | "kb" => 1_000,
        "m" | "mb" => 1_000_000,
        "g" | "gb" => 1_000_000_000,
        "t" | "tb" => 1_000_000_000_000,
        _ => return Err(anyhow!("invalid unit: {}", s)),
    };
    let num = num * Decimal::from(multiplier);
    num.to_i64()
        .with_context(|| format!("out of bounds: {}", s))
}
