use crate::text::searchable_text;
use common::ShowNameType;
use isnt::std_1::string::IsntStringExt;
use serde::Serialize;
use std::{borrow::Cow, collections::HashMap, mem};
use tokio_postgres::Row;
use unicode_normalization::UnicodeNormalization;

#[derive(Serialize)]
pub struct Show {
    pub letter: char,
    pub show_id: i64,
    pub display_name: String,
    pub display_name_is_romaji: bool,
    pub add_name: Option<String>,
}

#[derive(Serialize)]
struct JsonShow {
    element_id: i64,
    names: Vec<String>,
}

#[derive(Serialize)]
struct JsonLetter {
    name: char,
    elements: Vec<JsonShow>,
}

#[derive(Serialize)]
pub struct Letter {
    pub name: char,
    pub shows: Vec<Show>,
}

pub struct ShowList {
    pub letters: Vec<Letter>,
    pub json: String,
}

fn map_rows(
    rows: &[Row],
    show_id_idx: usize,
    name_idx: usize,
    show_name_type_idx: usize,
) -> HashMap<i64, Show> {
    let mut shows = HashMap::new();
    for row in rows {
        let show_id: i64 = row.get(show_id_idx);
        let name: String = row.get(name_idx);
        let ty: i32 = row.get(show_name_type_idx);
        let show = shows.entry(show_id).or_insert(Show {
            show_id,
            display_name: String::new(),
            display_name_is_romaji: false,
            add_name: None,
            letter: ' ',
        });
        if !show.display_name_is_romaji {
            let add_name = mem::replace(&mut show.display_name, name);
            if add_name.is_not_empty() {
                show.add_name = Some(add_name);
            }
            show.display_name_is_romaji = ty == ShowNameType::ROMAJI;
        } else {
            show.add_name = Some(name);
        }
    }
    shows
}

macro_rules! show_list_from_rows {
    ($stmt:expr, $rows:expr) => {
        show_list_from_rows($rows, $stmt.show_id, $stmt.name, $stmt.show_name_type)
    };
}

pub fn show_list_from_rows(
    rows: &[Row],
    show_id_idx: usize,
    name_idx: usize,
    show_name_type_idx: usize,
) -> ShowList {
    let shows = map_rows(rows, show_id_idx, name_idx, show_name_type_idx);
    let mut letters = HashMap::new();
    for (_, mut show) in shows {
        let display_name = &show.display_name;
        let display_name = if unicode_normalization::is_nfkd(display_name) {
            Cow::Borrowed(display_name)
        } else {
            Cow::Owned(display_name.nfkd().collect())
        };
        let letter = display_name.chars().next().unwrap().to_ascii_lowercase();
        show.letter = letter;
        letters.entry(letter).or_insert(vec![]).push(show);
    }
    let mut letters: Vec<_> = letters
        .into_iter()
        .map(|(k, v)| Letter {
            name: k.to_ascii_uppercase(),
            shows: v,
        })
        .collect();
    letters.sort_by(|v1, v2| v1.name.cmp(&v2.name));
    for letter in &mut letters {
        letter
            .shows
            .sort_by(|s1, s2| s1.display_name.cmp(&s2.display_name));
    }
    let json_letters: Vec<_> = letters
        .iter()
        .map(|l| JsonLetter {
            name: l.name,
            elements: l
                .shows
                .iter()
                .map(|s| JsonShow {
                    element_id: s.show_id,
                    names: {
                        let mut res = vec![searchable_text(&s.display_name)];
                        if let Some(ref n) = s.add_name {
                            res.push(searchable_text(n));
                        }
                        res
                    },
                })
                .collect(),
        })
        .collect();
    let json = serde_json::to_string(&json_letters).unwrap();
    ShowList { letters, json }
}
