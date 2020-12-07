use crate::text::MagnetFormatter;
use chrono::{DateTime, Utc};
use itertools::Itertools;
use std::collections::HashMap;
use tokio_postgres::Row;

pub struct Day<'a> {
    pub date: DateTime<Utc>,
    pub torrents: Vec<Torrent<'a>>,
}

pub struct Torrent<'a> {
    pub torrent_id: i64,
    pub title: &'a str,
    pub trusted: bool,
    pub date: DateTime<Utc>,
    pub magnet_link: MagnetFormatter<'a>,
}

macro_rules! torrent_list_from_rows {
    ($stmt:expr, $rows:expr) => {
        torrent_list_from_rows(
            $rows,
            $stmt.title,
            $stmt.nyaa_id,
            $stmt.torrent_id,
            $stmt.trusted,
            $stmt.hash,
            $stmt.uploaded_at,
        )
    };
}

pub fn torrent_list_from_rows(
    mut rows: &[Row],
    title_idx: usize,
    nyaa_id_idx: usize,
    torrent_id_idx: usize,
    trusted_idx: usize,
    hash_idx: usize,
    uploaded_at_idx: usize,
) -> (Option<i64>, Vec<Day>) {
    let last = match rows.len() {
        101 => {
            rows = &rows[..100];
            Some(rows.last().unwrap().get(nyaa_id_idx))
        }
        _ => None,
    };
    let mut days = HashMap::new();
    for row in rows {
        let title = row.get(title_idx);
        let uploaded_at: DateTime<Utc> = row.get(uploaded_at_idx);
        let day = days.entry(uploaded_at.date()).or_insert_with(|| Day {
            date: uploaded_at,
            torrents: vec![],
        });
        day.torrents.push(Torrent {
            torrent_id: row.get(torrent_id_idx),
            title,
            trusted: row.get(trusted_idx),
            date: uploaded_at,
            magnet_link: MagnetFormatter(title, row.get(hash_idx)),
        });
    }
    let days: Vec<_> = days
        .into_iter()
        .map(|(_, b)| b)
        .sorted_by(|a, b| b.date.cmp(&a.date))
        .collect();
    (last, days)
}
