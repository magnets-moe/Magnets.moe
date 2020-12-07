use crate::show_db::{find_format, find_season, find_year, search_name, Show, ShowDb};
use anyhow::{anyhow, Result};
use common::Format;
use isnt::std_1::vec::IsntVecExt;
use itertools::Itertools;
use regex::Regex;
use serde::export::Formatter;
use std::{borrow::Cow, fmt, fmt::Display};

pub fn find_show<'a>(db: &'a ShowDb, title: &str) -> Result<&'a Show> {
    let normalized_title = normalize_title(title, find_separator(title));
    let blocks = parse_blocks(&normalized_title);
    let name_range = find_name_range(&blocks);
    if name_range.is_empty() {
        return Err(anyhow!("name range is empty"));
    }
    let (ep, season, plain_digits) = find_episode(&name_range);
    let pre_episode_range = truncate_blocks(&name_range, ep);
    let res = handle_pre_episode_range(db, &normalized_title, &pre_episode_range, season);
    if res.is_err() && plain_digits {
        // e.g. Mob Psycho 100
        return handle_pre_episode_range(db, &normalized_title, &name_range, season);
    }
    res
}

fn blocks_to_string(s: &str, blocks: &[Block]) -> String {
    let last = blocks.last().unwrap();
    s[blocks[0].start..last.start + last.val.len()].to_string()
}

fn handle_pre_episode_range<'a>(
    db: &'a ShowDb,
    normalized_title: &str,
    pre_episode_range: &[Block],
    season: Option<u32>,
) -> Result<&'a Show> {
    if pre_episode_range.is_empty() {
        return Err(anyhow!("pre episode range is empty"));
    }
    let mut pre_episode_title = blocks_to_string(normalized_title, pre_episode_range);
    let mut metadata = extract_title_metadata(normalized_title, &mut pre_episode_title);
    if season.is_some() {
        metadata.0 = season;
    }
    let res = search(db, &pre_episode_title, metadata);
    if let x @ Ok(_) = res {
        return x;
    }
    if let Some(pos) = pre_episode_title.find('|') {
        if let x @ Ok(_) = search(db, &pre_episode_title[..pos], metadata) {
            return x;
        }
    }
    if pre_episode_range.len() > 1
        && pre_episode_range.last().unwrap().delimiter.is_some()
    {
        let mut pre_episode_title = blocks_to_string(
            normalized_title,
            &pre_episode_range[..pre_episode_range.len() - 1],
        );
        extract_title_metadata(normalized_title, &mut pre_episode_title);
        return search(db, &pre_episode_title, metadata);
    }
    res
}

type TitleMetadata = (Option<u32>, Option<u32>, Option<Format>);

fn search<'a>(
    db: &'a ShowDb,
    pre_episode_title: &str,
    (season, year, _format): TitleMetadata,
    // ) -> Result<Rc<Show>> {
) -> Result<&'a Show> {
    let search_name = search_name(pre_episode_title);
    let shows = db.map.get(&*search_name);
    if shows.is_none() {
        let idx = db.heap.find(&search_name);
        let r: Vec<_> = db
            .heap
            .iter(idx)
            .copied()
            .unique()
            .map(|idx| &db.shows[idx])
            .take(10)
            .collect();
        if r.len() == 1 {
            return Ok(r[0]);
        }
        return Err(anyhow!(
            "found no perfect match. trie search returned {}+ results: {}",
            r.len(),
            Shows(db, &r),
        ));
    }
    let shows: Vec<_> = shows
        .unwrap()
        .iter()
        .copied()
        .unique()
        .map(|idx| &db.shows[idx])
        .collect();
    if shows.len() == 1 {
        return Ok(shows[0]);
        // return Ok(shows[0].clone());
    }
    let mut total_shows = vec![];
    let mut season_shows = vec![];
    let mut year_shows = vec![];
    for show in &shows {
        let season_matches = match season {
            None => show.seasons.is_empty(),
            Some(season) => show.seasons.iter().any(|s| s.eq(&season)),
        };
        let year_matches = match year {
            None => false,
            Some(year) => show.years.iter().any(|s| s.eq(&year)),
        };
        if season_matches {
            if year_matches {
                total_shows.push(*show);
            } else {
                season_shows.push(*show);
            }
        }
        if year_matches {
            year_shows.push(*show);
        }
    }
    let shows = if total_shows.is_empty() {
        if season_shows.len() == 1 {
            &season_shows
        } else if year_shows.len() == 1 {
            &year_shows
        } else if season_shows.is_empty() {
            &shows
        } else {
            &season_shows
        }
    } else {
        &total_shows
    };
    if shows.len() == 1 {
        return Ok(shows[0]);
    }
    Err(anyhow!(
        "found {} perfect matches: {}",
        shows.len(),
        Shows(db, &**shows)
    ))
}

struct Shows<'a>(&'a ShowDb, &'a [&'a Show]);

impl<'a> Display for Shows<'a> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        for show in self.1 {
            write!(f, "{}[", show.show_id)?;
            for name in self.0.names.iter(show.names) {
                // for name in &*show.names {
                write!(f, "{}, ", name)?;
            }
            write!(f, "], ")?;
        }
        Ok(())
    }
}

fn extract_title_metadata(
    original_title: &str,
    pre_episode_title: &mut String,
) -> TitleMetadata {
    macro_rules! e {
        ($f:expr) => {
            match $f(&pre_episode_title) {
                Some((range, n)) => {
                    pre_episode_title.drain(range);
                    Some(n)
                }
                _ => $f(original_title).map(|(_, b)| b),
            }
        };
    }
    (e!(find_season), e!(find_year), e!(find_format))
}

fn truncate_blocks<'a>(
    blocks: &'a [Block<'a>],
    end: Option<(usize, usize)>, // block_idx, start position
) -> Cow<'a, [Block<'a>]> {
    let (block_idx, pos) = match end {
        None => {
            // [HorribleSubs] Shigatsu wa kimi no uso [750p]
            //                ^^^^^^^^^^^^^^^^^^^^^^^
            return Cow::Borrowed(blocks);
        }
        Some((end, block_end)) => (end, block_end),
    };
    let block = &blocks[block_idx];
    if block.delimiter.is_some() {
        // [HorribleSubs] Shigatsu wa kimi no uso (750p S01E02)
        //                ^^^^^^^^^^^^^^^^^^^^^^^ return value (delimited block removed)
        //                ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ episode range
        return Cow::Borrowed(&blocks[..block_idx]);
    }
    let mut last = blocks[block_idx];
    last.val = &last.val[..pos];
    if is_not_relevant(last.val) {
        // [HorribleSubs] Shigatsu wa kimi no uso (Cleaned) S01E02
        //                ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ return value (empty block removed)
        //                ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ episode range
        return Cow::Borrowed(&blocks[..block_idx]);
    }
    let mut blocks = blocks[..block_idx].to_vec();
    blocks.push(last);
    // [HorribleSubs] Shigatsu wa kimi no uso Cleaned S01E02
    //                ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ return value
    //                ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ episode range
    Cow::Owned(blocks)
}

fn find_separator(title: &str) -> char {
    let mut num_underscore = 0;
    let mut num_dot = 0;
    for c in title.chars() {
        match c {
            ' ' => return ' ',
            '_' => num_underscore += 1,
            '.' => num_dot += 1,
            _ => {}
        }
    }
    if num_underscore == 0 && num_dot == 0 {
        return ' ';
    }
    if num_underscore >= num_dot { '_' } else { '.' }
}

fn normalize_title(title: &str, separator: char) -> String {
    let mut last_was_space = true;
    let mut res = String::new();
    for c in title.chars() {
        if c == separator {
            if !last_was_space {
                res.push(' ');
                last_was_space = true;
            }
        } else {
            res.push(c.to_ascii_lowercase());
            last_was_space = false;
        }
    }
    if last_was_space {
        res.pop();
    }
    res
}

fn parse_blocks(title: &str) -> Vec<Block> {
    let mut blocks = vec![];
    let mut pos = 0;
    let mut block = Block::new(pos);
    macro_rules! push_block {
        ($pos:expr) => {{
            let stop = $pos;
            if block.start < stop {
                block.val = &title[block.start..stop];
                blocks.push(block);
                block = Block::new({
                    pos += 1;
                    pos
                });
                block.start = stop;
            }
        }};
    }
    let mut paren_depth = 0;
    for (pos, c) in title.char_indices() {
        match c {
            '[' | '(' => {
                if block.delimiter == None {
                    push_block!(pos);
                    block.delimiter = Some(c);
                }
                if block.delimiter == Some(c) {
                    paren_depth += 1;
                }
            }
            ']' | ')' if block.delimiter == Some(opening_pair(c)) => {
                paren_depth -= 1;
                if paren_depth == 0 {
                    push_block!(pos + 1);
                }
            }
            _ => {}
        }
    }
    if let Some(last) = blocks.pop() {
        if last.delimiter == None {
            block = last;
        } else {
            blocks.push(last);
        }
    }
    push_block!(title.len());
    if blocks.iter().any(|b| is_not_relevant(b.val)) {
        return blocks.into_iter().filter(|b| is_relevant(b.val)).collect();
    }
    blocks
}

fn is_relevant(s: &str) -> bool {
    for &b in s.as_bytes() {
        if matches!(b, b'a'..=b'z' | b'0'..=b'9') {
            return true;
        }
    }
    false
}

fn is_not_relevant(s: &str) -> bool {
    !is_relevant(s)
}

#[derive(Copy, Clone, Debug)]
struct Block<'a> {
    pos: usize,
    delimiter: Option<char>,
    start: usize,
    val: &'a str,
}

impl<'a> Block<'a> {
    fn new(pos: usize) -> Self {
        Block {
            pos,
            delimiter: None,
            start: 0,
            val: "",
        }
    }
}

fn opening_pair(c: char) -> char {
    match c {
        ']' => '[',
        ')' => '(',
        _ => unreachable!(),
    }
}

fn find_name_range<'a>(blocks: &[Block<'a>]) -> Vec<Block<'a>> {
    lazy_static::lazy_static! {
        static ref FILE_INFO: Regex = Regex::new(r"(?x)\b
            (
                    \.mkv
                |   mkv
                |   720p
                |   360p
                |   1080p
                |   multiple subtitle
                |   480p
                |   aac
                |   hevc
                |   english dub
                |   multi-?subs?
                |   540p
                |   10bit
                |   10-bit
                |   x265
                |   av1
                |   60pfs
                |   dual-audio
                |   x264
            )
            \b
        ").unwrap();
    }
    let mut matched = vec![];
    let mut last_len = None;
    for block in blocks.iter() {
        if matched.is_empty() && block.delimiter.is_some() {
            continue;
        }
        if let Some(ca) = FILE_INFO.captures(block.val) {
            let fi_start = ca.get(0).unwrap().start();
            if fi_start > 0 && block.delimiter == None {
                last_len = Some(fi_start);
                matched.push(*block);
            }
            if matched.is_not_empty() {
                break;
            }
        } else {
            matched.push(*block);
        }
    }
    while let Some(last) = matched.last() {
        if last.delimiter == Some('[') {
            matched.pop();
        } else {
            break;
        }
    }
    if let Some(last) = matched.last_mut() {
        last.val = &last.val[..last_len.unwrap_or_else(|| last.val.len())];
    }
    matched
}

fn find_episode(blocks: &[Block]) -> (Option<(usize, usize)>, Option<u32>, bool) {
    lazy_static::lazy_static! {
        static ref R1: regex::Regex = regex::Regex::new(r"(?x)
            (^|[^a-z0-9])
            (ep(\.|isodes?)?\s*)?
            (
                    \d+\s*~\s*\d+
                |   -\s*\d+\s*-\s*\d+
                |   \d+-\d+
                |   0\d+\s*-\s*\d+
                |   (s(?P<season>\d+)e|-\s*|(?P<plain>\d))\d+(\.\d)?\s*(v\d)?
            )
            \s*(end|final|oad)?
            [^a-z0-9]*
            $
            ").unwrap();
        static ref R2: regex::Regex = regex::Regex::new(r"(?x)
            (^|[^a-z0-9])
            ep(\.|isodes?)?\s*
            \d+(\s*(~|-)\s*\d+)?
            \s*(end|final|oad)?
            [^a-z0-9]*
            $
            ").unwrap();
        static ref R3: regex::Regex = regex::Regex::new(r"(?x)
            ^
            [^a-z0-9]*
            \d+(\s*-\s*\d+)?
            [^a-z0-9]*
            $
            ").unwrap();
    }
    for (pos, block) in blocks.iter().enumerate().rev() {
        for i in &[&*R1, &*R2, &*R3] {
            if let Some(ca) = i.captures(block.val) {
                let season = ca.name("season").map(|s| s.as_str().parse().unwrap());
                let offset = ca.get(0).unwrap().start();
                let plain = ca.name("plain").is_some();
                return (Some((pos, offset)), season, plain);
            }
        }
    }
    (None, None, false)
}
