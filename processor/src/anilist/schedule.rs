use crate::{
    anilist::{client::PageInfo, wait_for_grace_period},
    db_state::LAST_SCHEDULE_UPDATE,
    scheduled::Scheduled,
    state::State,
};
use anyhow::Result;
use chrono::{DateTime, Duration, TimeZone, Utc};
use common::{pg, time::MINUTE};
use serde::{Deserialize, Serialize};
use std::{cmp::Ordering, ops::Not};
use tokio_postgres::Transaction;

/// Loads the schedule once per hour
pub async fn load_schedule(state: &State<'_>) {
    wait_for_grace_period(state).await;
    let scheduled = Scheduled::new(
        state,
        LAST_SCHEDULE_UPDATE,
        state.config.anilist.schedule_poll_interval,
    );
    loop {
        scheduled.wait(&state.db_watcher.last_schedule_update).await;
        log::info!("loading the schedule");
        if let Err(e) = load_schedule_(state).await {
            log::error!("loading the schedule failed: {:#}", e);
            tokio::time::delay_for(5 * MINUTE).await;
        } else {
            scheduled.update().await;
        }
    }
}

#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct Item {
    airs_at: DateTime<Utc>,
    anilist_id: i64,
    episode: i32,
}

struct ExistingItem {
    item: Item,
    schedule_id: i64,
}

enum Diff {
    Add(Item),
    Del(ExistingItem),
}

/// Loads the schedule
pub async fn load_schedule_(state: &State<'_>) -> Result<()> {
    let mut pg = state.pg_connector.connect().await?;
    let tran = pg::transaction(&mut pg).await?;

    let existing = load_existing_items(&tran).await?;
    let new = load_new_items(state).await?;

    let diff = compute_diff(existing, new);

    log::info!("found {} schedule changes", diff.len());

    for diff in diff {
        match diff {
            Diff::Del(e) => {
                // language=sql
                tran.execute(
                    "delete from magnets.schedule where schedule_id = $1",
                    &[&e.schedule_id],
                )
                .await?;
            }
            Diff::Add(n) => {
                // language=sql
                tran.execute(
                    "
                    insert into magnets.schedule (show_id, episode, airs_at)
                    select show_id, $2, $3
                    from magnets.show
                    where anilist_id = $1",
                    &[&n.anilist_id, &n.episode, &n.airs_at],
                )
                .await?;
            }
        }
    }

    Ok(tran.commit().await?)
}

fn compute_diff(mut existing: Vec<ExistingItem>, mut new: Vec<Item>) -> Vec<Diff> {
    existing.sort_by(|e1, e2| e2.item.cmp(&e1.item));
    new.sort_by(|n1, n2| n2.cmp(n1));

    let mut res = vec![];

    while let Some(e) = existing.pop() {
        let n = match new.pop() {
            Some(n) => n,
            _ => {
                existing.push(e);
                break;
            }
        };
        match e.item.cmp(&n) {
            Ordering::Less => {
                res.push(Diff::Del(e));
                new.push(n);
            }
            Ordering::Greater => {
                res.push(Diff::Add(n));
                existing.push(e);
            }
            _ => {}
        }
    }
    res.extend(new.into_iter().map(Diff::Add).rev());
    res.extend(existing.into_iter().map(Diff::Del).rev());

    res
}

// language=sql
common::create_statement!(LoadScheduleItems, schedule_id, show_id, episode, airs_at, anilist_id; "
    select sch.schedule_id, sch.show_id, sch.episode, sch.airs_at, sho.anilist_id
    from magnets.schedule sch
    join magnets.show sho using (show_id)");

async fn load_existing_items(tran: &Transaction<'_>) -> Result<Vec<ExistingItem>> {
    let stmt = LoadScheduleItems::new(tran).await?;
    let rows = tran.query(&stmt.stmt, &[]).await?;
    let mut res = vec![];
    for row in rows {
        res.push(ExistingItem {
            item: Item {
                airs_at: row.get(stmt.airs_at),
                anilist_id: row.get(stmt.anilist_id),
                episode: row.get(stmt.episode),
            },
            schedule_id: row.get(stmt.schedule_id),
        })
    }
    Ok(res)
}

/// Loads the schedule
async fn load_new_items(state: &State<'_>) -> Result<Vec<Item>> {
    const QUERY: &str = r#"
query ($start: Int, $stop: Int, $page: Int) {
  page: Page(perPage: 50, page: $page) {
    page_info: pageInfo {
      total
      per_page: perPage
      current_page: currentPage
      last_page: lastPage
      has_next_page: hasNextPage
    }
    airing_schedule: airingSchedules(airingAt_greater: $start, airingAt_lesser: $stop) {
      airing_at: airingAt
      episode
      media_id: mediaId
    }
  }
}"#;

    #[derive(Serialize)]
    struct Variables {
        start: i64,
        stop: i64,
        page: i32,
    }

    #[derive(Deserialize, Debug)]
    struct AiringSchedule {
        airing_at: i64,
        episode: i32,
        media_id: i64,
    }

    #[derive(Deserialize, Debug)]
    struct Page {
        page_info: PageInfo,
        airing_schedule: Vec<AiringSchedule>,
    }

    #[derive(Deserialize, Debug)]
    struct Data {
        page: Page,
    }

    impl From<AiringSchedule> for Item {
        fn from(a: AiringSchedule) -> Self {
            Self {
                airs_at: Utc.timestamp(a.airing_at, 0),
                anilist_id: a.media_id,
                episode: a.episode,
            }
        }
    }

    // On magnets.moe, we only display the schedule from yesterday to six days from now
    // (7 days total). Therefore it makes sense to only retrieve a similar number of
    // days from anilist. Note however that we load one more day into the future to cover
    // the time between midnight and the next reload of the schedule.
    let today = Utc::today().and_hms(0, 0, 0);
    let yesterday = (today - Duration::days(1)).timestamp();
    let next_week = (today + Duration::days(7)).timestamp();

    let mut scheds = vec![];
    for page in 1.. {
        log::info!("loading schedule page {}", page);
        let variables = Variables {
            start: yesterday,
            stop: next_week,
            page,
        };
        let data: Data = state.anilist_client.request(QUERY, &variables).await;
        scheds.extend(data.page.airing_schedule.into_iter().map(Item::from));
        if data.page.page_info.has_next_page.not() {
            break;
        }
    }
    Ok(scheds)
}
