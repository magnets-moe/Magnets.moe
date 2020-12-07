use chrono::{
    format::{Item, StrftimeItems},
    DateTime, Utc,
};
use percent_encoding::{utf8_percent_encode, AsciiSet, PercentEncode, CONTROLS};
use serde::export::Formatter;
use std::{fmt, fmt::Display, mem::MaybeUninit};
use tokio_postgres::types::ToSql;

pub fn searchable_text(s: &str) -> String {
    let mut res = String::new();
    for mut c in s.chars() {
        c = c.to_ascii_lowercase();
        if c as u32 > 127 || matches!(c, 'a'..='z' | '0'..='9') {
            res.push(c);
        }
    }
    res
}

pub const TEXT_HTML: &str = "text/html; charset=utf-8";

pub fn query_encode(input: &str) -> PercentEncode {
    const QUERY: AsciiSet = CONTROLS
        .add(b' ')
        .add(b'#')
        .add(b'"')
        .add(b'\'')
        .add(b'<')
        .add(b'>');
    utf8_percent_encode(input, &QUERY)
}

pub const TRACKERS: [&str; 5] = [
    "http://nyaa.tracker.wf:7777/announce",
    "udp://open.stealth.si:80/announce",
    "udp://tracker.opentrackr.org:1337/announce",
    "udp://tracker.coppersurfer.tk:6969/announce",
    "udp://exodus.desync.com:6969/announce",
];

pub fn format_day(time: &DateTime<Utc>) -> Result<impl Display, askama::Error> {
    lazy_static::lazy_static! {
        static ref F: Vec<Item<'static>> = StrftimeItems::new("%F").collect();
    }
    Ok(time.date().format_with_items(F.iter()))
}

pub fn format_time(time: &DateTime<Utc>) -> Result<impl Display, askama::Error> {
    lazy_static::lazy_static! {
        static ref F: Vec<Item<'static>> = StrftimeItems::new("%R").collect();
    }
    Ok(time.time().format_with_items(F.iter()))
}

pub fn format_full_time(time: &DateTime<Utc>) -> Result<impl Display, askama::Error> {
    lazy_static::lazy_static! {
        static ref F: Vec<Item<'static>> = StrftimeItems::new("%F %R").collect();
    }
    Ok(time.format_with_items(F.iter()))
}

pub fn format_size(time: &i64) -> Result<impl Display, askama::Error> {
    Ok(bytesize::ByteSize::b(*time as u64).to_string_as(true))
}

pub struct HexFormatter<'a>(pub &'a [u8]);

impl<'a> Display for HexFormatter<'a> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        const HEX_CHARS_LOWER: [u8; 16] = *b"0123456789abcdef";
        unsafe {
            #[allow(clippy::uninit_assumed_init)]
            let mut buf: [u8; 40] = MaybeUninit::uninit().assume_init();
            let mut buf_pos = 0;
            for &byte in self.0 {
                if buf_pos == buf.len() {
                    f.write_str(std::str::from_utf8_unchecked(&buf))?;
                    buf_pos = 0;
                }
                *buf.get_unchecked_mut(buf_pos) = HEX_CHARS_LOWER[(byte >> 4) as usize];
                *buf.get_unchecked_mut(buf_pos + 1) =
                    HEX_CHARS_LOWER[(byte & 0xf) as usize];
                buf_pos += 2;
            }
            if buf_pos > 0 {
                f.write_str(std::str::from_utf8_unchecked(&buf[..buf_pos]))?;
            }
        }
        Ok(())
    }
}

pub struct MagnetFormatter<'a>(pub &'a str, pub &'a [u8]);

impl<'a> Display for MagnetFormatter<'a> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "magnet:?xt=urn:btih:{}&dn={}",
            HexFormatter(self.1),
            query_encode(self.0)
        )?;
        for tracker in &TRACKERS {
            write!(f, "&tr={}", query_encode(tracker))?;
        }
        Ok(())
    }
}

pub type SqlParams<'a> = &'a [&'a (dyn ToSql + Sync)];

#[derive(thiserror::Error, Debug)]
#[error("Not found")]
pub struct NotFound;
