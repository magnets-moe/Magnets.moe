use crate::{
    db_state, db_state::REMATCH_UNMATCHED, show_db::Show, state::State, title_analyzer,
};
use anyhow::Result;
use common::pg;
use tokio_postgres::Transaction;

#[derive(Copy, Clone, Eq, PartialEq)]
enum RematchMode {
    None,
    Unmatched,
    All,
}

pub async fn match_unmatched(state: &State<'_>) {
    loop {
        state.db_watcher.rematch_unmatched.notified().await;
        let rematch = match get_rematch_unmatched(state).await {
            Ok(r) => r,
            Err(e) => {
                log::error!("could not get rematch_unmatched, assuming None: {:#}", e);
                RematchMode::None
            }
        };
        if rematch != RematchMode::None {
            log::info!("rematching torrents");
            if let Err(e) = match_unmatched_(state, rematch).await {
                log::error!("matching unmatched torrents failed: {:#}", e);
            }
        }
    }
}

async fn get_rematch_unmatched(state: &State<'_>) -> Result<RematchMode> {
    let con = state.pg.borrow().await?;
    let res: i32 = db_state::get(&**con, REMATCH_UNMATCHED).await?;
    Ok(match res {
        0 => RematchMode::None,
        1 => RematchMode::Unmatched,
        2 => RematchMode::All,
        val => {
            log::error!("database contains unknown rematch mode {}", val);
            RematchMode::None
        }
    })
}

// language=sql
common::create_statement!(LoadAllUnmatchedTorrents, torrent_id, title;
                          "select * from magnets.torrent where not matched");

async fn match_unmatched_(state: &State<'_>, mode: RematchMode) -> Result<()> {
    let show_db = state.show_db.get().await?;
    let mut con = pg::connect().await?;
    let tran = pg::transaction(&mut con).await?;
    if mode == RematchMode::All {
        // language=sql
        tran.simple_query("truncate magnets.rel_torrent_show")
            .await?;
    }
    // language=sql
    tran.simple_query(
        "
        update magnets.torrent t
        set matched = exists (
            select *
            from magnets.rel_torrent_show rts
            where rts.torrent_id = t.torrent_id
        )",
    )
    .await?;
    let load = LoadAllUnmatchedTorrents::new(&tran).await?;
    let rows = tran.query(&load.stmt, &[]).await?;
    let mut matched = 0;
    for row in &rows {
        let title = row.get(load.title);
        let torrent_id: i64 = row.get(load.torrent_id);
        if let Ok(s) = title_analyzer::find_show(&show_db, title) {
            insert_match(&tran, torrent_id, &s).await?;
            if mode != RematchMode::All {
                log::info!(
                    "matched previously unmatched torrent {} with show {}: {}",
                    torrent_id,
                    s.show_id,
                    title
                );
            }
            matched += 1;
        }
    }
    log::info!(
        "matched {} out of {} previously unmatched torrents",
        matched,
        rows.len()
    );
    db_state::set(&tran, REMATCH_UNMATCHED, 0).await?;
    tran.commit().await?;
    Ok(())
}

pub async fn insert_match(
    tran: &Transaction<'_>,
    torrent_id: i64,
    s: &Show,
) -> Result<()> {
    // language=sql
    tran.execute(
        "insert into magnets.rel_torrent_show (show_id, torrent_id, nyaa_id)
        select $1, $2, nyaa_id
        from magnets.torrent where torrent_id = $2",
        &[&s.show_id, &torrent_id],
    )
    .await?;
    // language=sql
    tran.execute(
        "update magnets.torrent set matched = true where torrent_id = $1",
        &[&torrent_id],
    )
    .await?;
    Ok(())
}
