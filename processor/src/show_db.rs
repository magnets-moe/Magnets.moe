use crate::{
    heap::AsciiHeap,
    strings::{ArcString, StringLists},
};
use anyhow::Result;
use common::{pg, pg::PgConnector, Format, YearSeason};
use smallvec::{smallvec, SmallVec};
use std::{
    collections::HashMap,
    hash::{Hash, Hasher},
    ops::Range,
    sync::Arc,
};
use tokio::sync::Mutex;
use tokio_postgres::Transaction;

/// A large number suitable for ensuring that allocations occur via mmap
pub const LARGE_NUMBER: usize = 10_000;

pub struct Show {
    pub show_id: i64,
    pub names: usize,
    pub seasons: SmallVec<[u32; 1]>,
    pub years: SmallVec<[u32; 1]>,
    pub formats: SmallVec<[Format; 1]>,
}

impl Eq for Show {
}

impl PartialEq<Show> for Show {
    fn eq(&self, other: &Show) -> bool {
        self.show_id == other.show_id
    }
}

impl Hash for Show {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.show_id.hash(state)
    }
}

pub struct ShowDb {
    pub shows: Box<[Show]>,
    pub names: StringLists,
    pub map: HashMap<ArcString, SmallVec<[usize; 1]>>,
    pub heap: AsciiHeap<usize>,
}

// language=sql
common::create_statement!(LoadAllShows, show_id, show_format, season;
                          "select show_id, show_format, season from magnets.show");

async fn load_shows(
    tran: &Transaction<'_>,
) -> Result<(HashMap<i64, usize>, Box<[Show]>)> {
    let load_all_shows = LoadAllShows::new(tran).await?;

    let mut shows = Vec::with_capacity(LARGE_NUMBER);
    let mut shows_map = HashMap::with_capacity(LARGE_NUMBER);

    let rows = tran.query(&load_all_shows.stmt, &[]).await?;
    for row in rows {
        let mut show = Show {
            show_id: row.get(load_all_shows.show_id),
            names: 0,
            seasons: smallvec![],
            years: smallvec![],
            formats: smallvec![Format::from_db(row.get(load_all_shows.show_format),)?],
        };
        if let Some(season) = row.get::<_, Option<i32>>(load_all_shows.season) {
            show.years.push(YearSeason::from_db(season)?.year as u32);
        }
        shows_map.insert(show.show_id, shows.len());
        shows.push(show);
    }
    Ok((shows_map, shows.into_boxed_slice()))
}

// language=sql
common::create_statement!(LoadAllShowNames, show_id, name;
                          "select show_id, name from magnets.show_name order by show_id");

async fn load_names(
    tran: &Transaction<'_>,
) -> Result<(StringLists, Vec<(i64, usize)>, usize)> {
    let s = LoadAllShowNames::new(tran).await?;

    let mut string_lists = StringLists::new();
    let rows = tran.query(&s.stmt, &[]).await?;
    let mut res = Vec::with_capacity(LARGE_NUMBER);
    let mut last_show_id = None;
    for row in &rows {
        let show_id = row.get(s.show_id);
        let name = row.get(s.name);
        if let Some(last_show_id) = last_show_id {
            if last_show_id != show_id {
                res.push((last_show_id, string_lists.finish_list()));
            }
        }
        string_lists.push_str(name);
        last_show_id = Some(show_id);
    }
    if let Some(last_show_id) = last_show_id {
        res.push((last_show_id, string_lists.finish_list()));
    }
    string_lists.shrink_to_fit();
    Ok((string_lists, res, rows.len()))
}

fn build_db(
    shows: (HashMap<i64, usize>, Box<[Show]>),
    names: (StringLists, Vec<(i64, usize)>, usize),
) -> ShowDb {
    let (shows_map, mut shows) = shows;
    let (show_names, names, total_names) = names;
    let mut names_map = HashMap::new();
    let mut search_name_buf = String::with_capacity(LARGE_NUMBER);
    let mut search_name_ranges = Vec::with_capacity(total_names);
    for (show_id, names) in names {
        let show_idx = shows_map[&show_id];
        let show = &mut shows[show_idx];
        show.names = names;
        for name in show_names.iter(names) {
            let mut name = name.to_ascii_lowercase();
            if let Some((range, year)) = find_year(&name) {
                if show.years.iter().all(|&y| y != year) {
                    show.years.push(year);
                }
                name.drain(range);
            }
            if let Some((range, format)) = find_format(&name) {
                if show.formats.iter().all(|&f| f != format) {
                    show.formats.push(format);
                }
                name.drain(range);
            }
            if let Some((range, season)) = find_season(&name) {
                show.seasons.push(season);
                name.drain(range);
            }
            {
                let start = search_name_buf.len();
                search_name_buf.push_str(&search_name(&name));
                let end = search_name_buf.len();
                search_name_ranges.push((start..end, show_idx));
            }
        }
    }
    let arc_string = ArcString::new(search_name_buf);
    for (range, show_idx) in search_name_ranges {
        names_map
            .entry(arc_string.substring(range))
            .or_insert(smallvec![])
            .push(show_idx);
    }
    let heap = AsciiHeap::new(names_map.iter().flat_map(|(search_name, show_idxs)| {
        show_idxs
            .iter()
            .map(move |&show_idx| (&**search_name, show_idx))
    }));
    log::info!("total show/name combos: {}", total_names);
    names_map.shrink_to_fit();
    ShowDb {
        names: show_names,
        shows,
        map: names_map,
        heap,
    }
}

pub fn find_year(s: &str) -> Option<(Range<usize>, u32)> {
    lazy_static::lazy_static! {
        static ref YEAR: regex::Regex = regex::Regex::new(r"\((\d{4})\)").unwrap();
    }
    let ca = YEAR.captures(s)?;
    let zero = ca.get(0).unwrap();
    let year = ca.get(1).unwrap().as_str().parse().unwrap();
    Some((zero.start()..zero.end(), year))
}

pub fn find_format(s: &str) -> Option<(Range<usize>, Format)> {
    lazy_static::lazy_static! {
        static ref FORMAT: regex::Regex = regex::Regex::new(r"\((tv|movie|ova|ona|oad)\)").unwrap();
    }
    let ca = FORMAT.captures(s)?;
    let zero = ca.get(0).unwrap();
    let format = match ca.get(1).unwrap().as_str() {
        "tv" => Format::Tv,
        "movie" => Format::Movie,
        "ova" | "oad" => Format::Ova,
        "ona" => Format::Ona,
        _ => unreachable!(),
    };
    Some((zero.start()..zero.end(), format))
}

pub fn find_season(s: &str) -> Option<(Range<usize>, u32)> {
    lazy_static::lazy_static! {
        static ref SEASON: regex::Regex =
            regex::Regex::new(r"(?x)
                    (^|\b)
                    (
                            (?P<season1>\d+)(st|nd|rd|th)\sseason        # 2nd season
                        |   season\s(?P<season2>\d{1,5})                 # season 2
                        |   s(?P<season3>\d+)                            # s2
                        |   (?P<season4>(first|second|third))\sseason    # first season
                    )
                    (\b|$)").unwrap();
    }
    let mut ca = SEASON.captures(s)?;
    let mut start = 0;
    loop {
        start += ca.get(0).unwrap().start();
        match SEASON.captures(&s[start + 1..]) {
            Some(ca2) => {
                ca = ca2;
                start += 1;
            }
            _ => break,
        };
    }
    let end = start + ca[0].len();
    for n in &["season1", "season2", "season3"] {
        if let Some(ca2) = ca.name(n) {
            return Some((start..end, ca2.as_str().parse().unwrap()));
        }
    }
    if let Some(ca2) = ca.name("season4") {
        let s = match ca2.as_str() {
            "first" => 1,
            "second" => 2,
            "third" => 3,
            _ => unreachable!(),
        };
        return Some((start..end, s));
    }
    None
}

pub fn search_name(s: &str) -> String {
    let mut search_name = String::new();
    for &b in s.as_bytes() {
        if matches!(b, b'a'..=b'z' | b'0'..=b'9') {
            search_name.push(b as char);
        }
    }
    search_name.shrink_to_fit();
    search_name
}

async fn load_db(connector: &PgConnector) -> Result<ShowDb> {
    log::info!("reloading the database");
    let mut con = connector.connect().await?;
    let tran = pg::transaction(&mut con).await?;
    let (shows, names) = futures::join!(load_shows(&tran), load_names(&tran));
    let db = build_db(shows?, names?);
    Ok(db)
}

pub struct ShowDbHolder {
    show_db: Mutex<Option<Arc<ShowDb>>>,
    connector: PgConnector,
}

impl ShowDbHolder {
    pub fn new(connector: &PgConnector) -> Self {
        Self {
            show_db: Mutex::new(None),
            connector: connector.clone(),
        }
    }

    pub async fn get(&self) -> Result<Arc<ShowDb>> {
        let mut show_db = self.show_db.lock().await;
        if show_db.is_none() {
            *show_db = Some(Arc::new(load_db(&self.connector).await?));
        }
        Ok(show_db.as_ref().unwrap().clone())
    }

    pub async fn refresh(&self) -> Result<()> {
        let new = Arc::new(load_db(&self.connector).await?);
        *self.show_db.lock().await = Some(new);
        Ok(())
    }
}
