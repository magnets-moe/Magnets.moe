use anyhow::Result;
use isnt::std_1::vec::IsntVecExt;
use std::collections::HashMap;

/// Calculates the diff between the current state of `magnets.rel_torrent_show` and the
/// result of rematching all torrents.
///
/// The result will be printed to stdout.
pub fn diff() -> Result<()> {
    tokio::runtime::Builder::new()
        .basic_scheduler()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async_diff())?;
    Ok(())
}

enum Diff {
    Add,
    Sub,
}

async fn async_diff() -> Result<()> {
    let current = load_current().await?;
    let show_names = load_show_names().await?;
    let show_db = crate::show_db::ShowDbHolder::new().get().await?;
    let pg = common::pg::connect().await?;
    // language=sql
    let torrents = pg
        .query("select title, nyaa_id from magnets.torrent", &[])
        .await?;
    for torrent in torrents {
        let title = torrent.get("title");
        let nyaa_id = torrent.get("nyaa_id");
        let current = current.get(&nyaa_id).map(|v| &**v).unwrap_or(&[]);
        let mut diff = vec![];
        match crate::title_analyzer::find_show(&show_db, title) {
            Ok(show) => {
                if current.is_empty() {
                    diff.push((Diff::Add, show.show_id));
                } else if current.len() == 1 {
                    let old_show = current[0];
                    if show.show_id != old_show {
                        diff.push((Diff::Sub, old_show));
                        diff.push((Diff::Add, show.show_id));
                    }
                } else {
                    let mut have_new = false;
                    for &old_show in current {
                        if old_show == show.show_id {
                            have_new = true;
                        } else {
                            diff.push((Diff::Sub, old_show));
                        }
                    }
                    if !have_new {
                        diff.push((Diff::Add, show.show_id));
                    }
                }
            }
            _ => {
                for &old_show in current {
                    diff.push((Diff::Sub, old_show));
                }
            }
        }
        if diff.is_not_empty() {
            println!("{}", title);
            for (diff, show_id) in &diff {
                match diff {
                    Diff::Sub => println!("- {} -> {}", nyaa_id, show_id),
                    Diff::Add => println!("+ {} -> {}", nyaa_id, show_id),
                }
                for name in show_names.get(show_id).unwrap() {
                    println!("    {}", name);
                }
            }
            println!();
        }
    }
    Ok(())
}

async fn load_current() -> Result<HashMap<i64, Vec<i64>>> {
    let pg = common::pg::connect().await?;
    // language=sql
    let rows = pg
        .query("select nyaa_id, show_id from magnets.rel_torrent_show", &[])
        .await?;
    let mut res = HashMap::new();
    for row in rows {
        res.entry(row.get("nyaa_id"))
            .or_insert(vec![])
            .push(row.get("show_id"));
    }
    Ok(res)
}

async fn load_show_names() -> Result<HashMap<i64, Vec<String>>> {
    let pg = common::pg::connect().await?;
    // language=sql
    let rows = pg
        .query("select show_id, name from magnets.show_name", &[])
        .await?;
    let mut res = HashMap::new();
    for row in rows {
        res.entry(row.get("show_id"))
            .or_insert(vec![])
            .push(row.get("name"));
    }
    Ok(res)
}
