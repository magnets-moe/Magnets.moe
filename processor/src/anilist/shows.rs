use crate::{
    anilist::{
        client::{AnilistClient, PageInfo},
        wait_for_grace_period,
    },
    db_state::LAST_SHOWS_UPDATE,
    scheduled::Scheduled,
    state::State,
};
use anyhow::Result;
use common::{pg, pg::PgClient, time::MINUTE, Format, Season, ShowNameType, YearSeason};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use unicode_normalization::UnicodeNormalization;

/// Refreshes our copy of the anilist shows database once a day
pub async fn load_shows(state: &State<'_>) {
    wait_for_grace_period(state).await;
    let scheduled = Scheduled::new(
        state,
        LAST_SHOWS_UPDATE,
        state.config.anilist.shows_poll_interval,
    );
    loop {
        scheduled.wait(&state.db_watcher.last_shows_update).await;
        log::info!("loading the shows");
        if let Err(e) = load_shows_now(state).await {
            log::error!("loading the shows failed: {:#}", e);
            tokio::time::delay_for(5 * MINUTE).await;
        } else {
            scheduled.update().await;
            // Refresh the show db so that the analyzer has access to the new data.
            if let Err(e) = state.show_db.refresh().await {
                log::error!("refreshing shows db failed: {:#}", e);
            }
        }
    }
}

/// Refreshes our copy of the anilist shows database
pub async fn load_shows_now(state: &State<'_>) -> Result<()> {
    let mut con = state.pg_connector.connect().await?;
    let shows = load_shows_from_db(&mut con).await?;
    log::info!("loaded {} existing shows", shows.len());
    for i in 1.. {
        // Note that we load the pages in increasing order of anilist's ids. This means
        // that we should not miss any shows unless an older show gets deleted while
        // we are traversing the pages.
        let has_next =
            load_shows_page(&mut con, &shows, &state.anilist_client, i).await?;
        if !has_next {
            break;
        }
    }
    Ok(())
}

// language=sql
common::create_statement!(LoadAllShows, show_id, show_format, season, anilist_id;
                          "select show_id, show_format, season, anilist_id from magnets.show");

// language=sql
common::create_statement!(LoadAllShowNames, show_name_id, show_id, name, show_name_type;
                          "select show_name_id, show_id, name, show_name_type from magnets.show_name");

struct Show {
    show_id: i64,
    anilist_id: i64,
    format: Format,
    season: Option<YearSeason>,
    names: Vec<Name>,
}

struct Name {
    show_name_id: i64,
    name: String,
    show_name_type: i32,
}

/// Loads our copy of the anilist shows database
async fn load_shows_from_db(con: &mut PgClient) -> Result<HashMap<i64, Show>> {
    let tran = pg::transaction(con).await?;
    let load = LoadAllShows::new(&tran).await?;
    let mut shows = HashMap::new();
    let rows = tran.query(&load.stmt, &[]).await?;
    for row in rows {
        let season = match row.get(load.season) {
            Some(s) => Some(YearSeason::from_db(s)?),
            _ => None,
        };
        let show = Show {
            show_id: row.get(load.show_id),
            anilist_id: row.get(load.anilist_id),
            format: Format::from_db(row.get(load.show_format))?,
            season,
            names: vec![],
        };
        shows.insert(show.show_id, show);
    }
    let load = LoadAllShowNames::new(&tran).await?;
    let rows = tran.query(&load.stmt, &[]).await?;
    for row in rows {
        let name = Name {
            show_name_id: row.get(load.show_name_id),
            name: row.get(load.name),
            show_name_type: row.get(load.show_name_type),
        };
        shows
            .get_mut(&row.get(load.show_id))
            .unwrap()
            .names
            .push(name);
    }
    Ok(shows.into_iter().map(|(_, v)| (v.anilist_id, v)).collect())
}

/// Loads one page of the anilist shows database
async fn load_shows_page(
    con: &mut PgClient,
    existing: &HashMap<i64, Show>,
    client: &AnilistClient<'_>,
    page: i32,
) -> Result<bool> {
    log::info!("loading anilist shows page {}", page);

    const QUERY: &str = r#"
query ($page: Int) {
  page: Page(perPage: 50, page: $page) {
    page_info: pageInfo {
      total
      per_page: perPage
      current_page: currentPage
      last_page: lastPage
      has_next_page: hasNextPage
    }
    media(sort: ID, format_in: [TV, TV_SHORT, MOVIE, SPECIAL, OVA, ONA]) {
      id
      title {
        romaji
        english
      }
      season_year: seasonYear
      season
      format
    }
  }
}"#;

    #[derive(Serialize)]
    struct Variables {
        page: i32,
    }

    #[derive(Deserialize, Debug)]
    struct Title {
        // We assume that the romaji name is always set. This holds true as of this
        // comment. Both the frontend and the backend rely on having a romaji name.
        romaji: String,
        english: Option<String>,
    }

    #[derive(Deserialize, Debug)]
    struct Media {
        id: i64,
        title: Title,
        season_year: Option<u16>,
        season: Option<String>,
        format: String,
    }

    #[derive(Deserialize, Debug)]
    struct Page {
        page_info: PageInfo,
        media: Vec<Media>,
    }

    #[derive(Deserialize, Debug)]
    struct Data {
        page: Page,
    }

    let data: Data = client.request(QUERY, &Variables { page }).await;

    // We are transactional on a per-page basis. Note that we HAVE to calculate a diff to
    // preserve the foreign key constraints. This is also more efficient because the
    // upstream database changes very little.
    let tran = pg::transaction(con).await?;

    for x in &data.page.media {
        let format = match Format::from_anilist(&x.format) {
            Ok(f) => f,
            Err(_) => {
                log::warn!("cannot parse format of anilist show: {}", x.format);
                // Note that we do not abort the operation if we cannot deal with the
                // response. I assume that any parsing problem will require manual
                // intervention. No point in aborting and retrying later. Instead skip
                // to the next result.
                continue;
            }
        };
        let season = match (x.season_year, &x.season) {
            (Some(season_year), Some(season)) => {
                let season = match Season::from_anilist_str(season) {
                    Ok(s) => s,
                    Err(_) => {
                        log::warn!("cannot parse anilist season: {}", season);
                        continue;
                    }
                };
                Some(YearSeason {
                    year: season_year,
                    season,
                })
            }
            _ => None,
        };
        // We store everything in NFC form
        let romaji = x.title.romaji.nfc().collect();
        let mut names = vec![];
        if let Some(n) = &x.title.english {
            let name: String = n.nfc().collect();
            if name != romaji {
                names.push(Name {
                    show_name_id: -1,
                    name,
                    show_name_type: ShowNameType::ENGLISH,
                });
            }
        }
        names.push(Name {
            show_name_id: -1,
            name: romaji,
            show_name_type: ShowNameType::ROMAJI,
        });
        if let Some(existing) = existing.get(&x.id) {
            if existing.format != format {
                log::info!(
                    "updating format of show {} from {} to {}",
                    existing.show_id,
                    existing.format.as_str(),
                    format
                );
                // language=sql
                tran.execute(
                    "update magnets.show set show_format = $1 where show_id = $2",
                    &[&format.to_db(), &existing.show_id],
                )
                .await?;
            }
            if existing.season != season {
                log::info!(
                    "updating season of show {} from {:?} to {:?}",
                    existing.show_id,
                    existing.season,
                    season
                );
                // language=sql
                tran.execute(
                    "update magnets.show set season = $1 where show_id = $2",
                    &[&season.map(|s| s.to_db()), &existing.show_id],
                )
                .await?;
            }
            for name in names {
                match existing
                    .names
                    .iter()
                    .find(|e| e.show_name_type == name.show_name_type)
                {
                    Some(old) => {
                        if name.name != old.name {
                            log::info!(
                                "updating name ({}) of show {} from {} to {}",
                                old.show_name_type,
                                existing.show_id,
                                old.name,
                                name.name
                            );
                            // language=sql
                            tran.execute("update magnets.show_name set name = $1 where show_name_id = $2",
                                         &[&name.name, &old.show_name_id]).await?;
                        }
                    }
                    _ => {
                        log::info!(
                            "adding new name ({}) to show {}: {}",
                            name.show_name_type,
                            existing.show_id,
                            name.name
                        );
                        // language=sql
                        tran.execute("insert into magnets.show_name (show_id, show_name_type, name) values ($1, $2, $3)",
                                     &[&existing.show_id, &name.show_name_type, &name.name]).await?;
                    }
                }
            }
            continue;
        }
        log::info!("adding new show {}", x.title.romaji);
        // language=sql
        let row = tran
            .query_one(
                "insert into magnets.show (anilist_id, show_format, season) values ($1, $2, $3) returning show_id",
                &[&x.id, &format.to_db(), &season.map(|s| s.to_db())],
            )
            .await?;
        let show_id: i64 = row.get("show_id");
        for name in names {
            // language=sql
            tran.execute("insert into magnets.show_name (show_id, show_name_type, name) values ($1, $2, $3)",
                         &[&show_id, &name.show_name_type, &name.name]).await?;
        }
    }

    tran.commit().await?;

    Ok(data.page.page_info.has_next_page)
}
