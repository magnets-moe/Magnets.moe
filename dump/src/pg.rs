use anyhow::{anyhow, Result};
use bytes::BufMut;
use common::time::StdDuration;
use memchr::memchr3;
use postgres_types::{accepts, to_sql_checked};
use std::{
    convert::TryInto,
    error::Error,
    fmt::Display,
    io::{Read, Write},
    marker::PhantomData,
    path::{Path, PathBuf},
    str::FromStr,
    time::SystemTime,
};
use tokio_postgres::{types::{private::BytesMut, FromSql, IsNull, ToSql, Type}, Row};
use tokio_postgres::binary_copy::BinaryCopyOutRow;

pub trait GenericRow {
    fn get<'a, T: FromSql<'a>>(&'a self, idx: usize) -> T;
}

impl GenericRow for Row {
    fn get<'a, T: FromSql<'a>>(&'a self, idx: usize) -> T {
        self.get(idx)
    }
}

impl GenericRow for BinaryCopyOutRow {
    fn get<'a, T: FromSql<'a>>(&'a self, idx: usize) -> T {
        self.get(idx)
    }
}

pub trait Serializer<W, G> {
    fn serialize(&self, w: &mut W, row: &G, idx: usize) -> Result<()>;

    fn create_file(&self, _root: &Path, _row: &G) -> Result<PathBuf> {
        unimplemented!();
    }
}

pub trait Deserializer {
    fn read(&self, line: &str) -> Result<Box<dyn ToSql + Sync>>;
}

macro_rules! map {
    ($($pgty:ident => $rt:ident,)*) => {
        pub fn serializer<W: Write, G: GenericRow>(t: &Type) -> &'static dyn Serializer<W, G> {
            match *t {
                $(Type::$pgty => &$rt,)*
                ref t => unreachable!("cannot serialize type {}", t),
            }
        }

        pub fn deserializer(t: &Type) -> &'static dyn Deserializer {
            match *t {
                $(Type::$pgty => &$rt,)*
                ref t => unreachable!("cannot deserialize type {}", t),
            }
        }
    }
}

map! {
    TEXT => Text,
    INT4 => Int4,
    INT8 => Int8,
    TIMESTAMPTZ => Timestamptz,
    JSONB => Json,
    BYTEA => Bytea,
    BOOL => Bool,
}

trait Modifier<T, U> {
    fn modify(t: T) -> U;
}

trait Formatter<W, T> {
    fn write(w: &mut W, t: T) -> Result<()>;
}

fn plain3<'a, W, T, F, G>(w: &mut W, r: &'a G, idx: usize) -> Result<()>
where
    W: Write,
    T: FromSql<'a>,
    F: Formatter<W, T>,
    G: GenericRow
{
    match r.get::<Option<T>>(idx) {
        Some(v) => F::write(w, v)?,
        _ => writeln!(w, "null")?,
    }
    Ok(())
}

fn plain2<W, T, U, M, G>(w: &mut W, r: &G, idx: usize) -> Result<()>
where
    W: Write,
    T: for<'c> FromSql<'c>,
    U: Display,
    M: Modifier<T, U>,
    G: GenericRow,
{
    struct F<U, M> {
        _d: PhantomData<(U, M)>,
    }
    impl<W: Write, T, U: Display, M: Modifier<T, U>> Formatter<W, T> for F<U, M> {
        fn write(w: &mut W, t: T) -> Result<()> {
            writeln!(w, "{}", M::modify(t))?;
            Ok(())
        }
    }
    plain3::<W, T, F<U, M>, G>(w, r, idx)
}

fn plain<W, T, G>(w: &mut W, r: &G, idx: usize) -> Result<()>
where
    W: Write,
    T: for<'c> FromSql<'c> + Display,
    G: GenericRow,
{
    struct Id;
    impl<T> Modifier<T, T> for Id {
        fn modify(t: T) -> T {
            t
        }
    }
    plain2::<W, T, T, Id, G>(w, r, idx)
}

macro_rules! null_check {
    ($l:expr) => {
        if $l == "null" {
            return Ok(Box::new(Null));
        }
    };
}

pub struct Int4;
pub struct Int8;

macro_rules! int {
    ($pt:ty, $rt:ty) => {
        impl<W: Write, G: GenericRow> Serializer<W, G> for $pt {
            fn serialize(&self, w: &mut W, row: &G, idx: usize) -> Result<()> {
                plain::<_, $rt, G>(w, row, idx)
            }

            fn create_file(&self, root: &Path, row: &G) -> Result<PathBuf> {
                let key: $rt = row.get(0);
                let mut dir = root.join(format!("{}", key / 1000));
                std::fs::create_dir_all(&dir)?;
                dir.push(key.to_string());
                Ok(dir)
            }
        }

        impl Deserializer for $pt {
            fn read(&self, line: &str) -> Result<Box<dyn ToSql + Sync>> {
                null_check!(line);
                let s: $rt = line.parse()?;
                Ok(Box::new(s))
            }
        }
    };
}

int!(Int4, i32);
int!(Int8, i64);

#[derive(Debug)]
pub struct Null;

impl ToSql for Null {
    fn to_sql(
        &self,
        _ty: &Type,
        _out: &mut BytesMut,
    ) -> Result<IsNull, Box<dyn Error + Sync + Send>>
    where
        Self: Sized,
    {
        Ok(IsNull::Yes)
    }

    fn accepts(_: &Type) -> bool
    where
        Self: Sized,
    {
        true
    }

    to_sql_checked!();
}

pub struct Text;

impl<W: Write, G: GenericRow> Serializer<W, G> for Text {
    fn serialize(&self, w: &mut W, row: &G, idx: usize) -> Result<()> {
        plain3::<_, &str, EscapedWriter<IdMapper<&str>>, G>(w, row, idx)
    }

    fn create_file(&self, root: &Path, row: &G) -> Result<PathBuf> {
        Ok(root.join(row.get::<&str>(0)))
    }
}

impl Deserializer for Text {
    fn read(&self, line: &str) -> Result<Box<dyn ToSql + Sync>> {
        null_check!(line);
        Ok(Box::new(read_text(line)?))
    }
}

fn read_text(mut line: &str) -> Result<String> {
    if line.len() < 2 {
        return Err(anyhow!("text line is too small"));
    }
    if line.as_bytes()[0] != b'"' || line.as_bytes()[line.len() - 1] != b'"' {
        return Err(anyhow!("text line is not delimited by \""));
    }
    line = &line[1..line.len() - 1];
    let mut res = String::new();
    loop {
        let p = match memchr::memchr(b'\\', line.as_bytes()) {
            Some(p) => p,
            None => {
                res.push_str(line);
                return Ok(res);
            }
        };
        res.push_str(&line[..p]);
        if p + 1 >= line.len() {
            return Err(anyhow!("text line contains a trailing \\"));
        }
        match line.as_bytes()[p + 1] {
            b'n' => res.push('\n'),
            c => res.push(c as char),
        }
        line = &line[p + 2..];
    }
}

struct IdMapper<T> {
    _d: PhantomData<T>,
}

impl<T> Modifier<T, T> for IdMapper<T> {
    fn modify(t: T) -> T {
        t
    }
}

struct EscapedWriter<M> {
    _d: PhantomData<M>,
}

impl<'a, W: Write, T, M: Modifier<T, &'a str>> Formatter<W, T> for EscapedWriter<M> {
    fn write(w: &mut W, t: T) -> Result<()> {
        let mut t = M::modify(t);
        write!(w, "\"")?;
        loop {
            let pos = match memchr3(b'"', b'\\', b'\n', t.as_bytes()) {
                Some(p) => p,
                _ => {
                    writeln!(w, "{}\"", t)?;
                    return Ok(());
                }
            };
            write!(w, "{}\\", &t[..pos])?;
            match t.as_bytes()[pos] {
                c @ b'"' | c @ b'\\' => w.write_all(&[c])?,
                b'n' => w.write_all(&[b'n'])?,
                _ => unreachable!(),
            }
            t = &t[pos + 1..];
        }
    }
}

pub struct Timestamptz;

impl<W: Write, G: GenericRow> Serializer<W, G> for Timestamptz {
    fn serialize(&self, w: &mut W, row: &G, idx: usize) -> Result<()> {
        struct X;
        impl Modifier<SystemTime, i128> for X {
            fn modify(s: SystemTime) -> i128 {
                s.duration_since(SystemTime::UNIX_EPOCH)
                    .map(|d| d.as_micros() as i128)
                    .unwrap_or_else(|d| -(d.duration().as_micros() as i128))
            }
        }
        plain2::<_, _, _, X, G>(w, row, idx)
    }
}

impl Deserializer for Timestamptz {
    fn read(&self, line: &str) -> Result<Box<dyn ToSql + Sync>> {
        null_check!(line);
        let ms: i128 = line.parse()?;
        let duration = StdDuration::from_micros(ms.try_into()?);
        Ok(Box::new(
            SystemTime::UNIX_EPOCH
                .checked_add(duration)
                .ok_or_else(|| anyhow!("duration {:?} is out of bounds", duration))?,
        ))
    }
}

pub struct Json;

impl<W: Write, G: GenericRow> Serializer<W, G> for Json {
    fn serialize(&self, w: &mut W, row: &G, idx: usize) -> Result<()> {
        plain3::<_, _, EscapedWriter<JsonTextToStrModifier>, G>(w, row, idx)
    }
}

impl Deserializer for Json {
    fn read(&self, line: &str) -> Result<Box<dyn ToSql + Sync>> {
        null_check!(line);
        Ok(Box::new(JsonString(read_text(line)?)))
    }
}

struct JsonTextToStrModifier;

impl<'a> Modifier<JsonStr<'a>, &'a str> for JsonTextToStrModifier {
    fn modify(t: JsonStr<'a>) -> &'a str {
        t.0
    }
}

struct JsonStr<'a>(&'a str);

#[derive(Debug)]
struct JsonString(String);

impl<'a> FromSql<'a> for JsonStr<'a> {
    fn from_sql(
        ty: &Type,
        mut raw: &'a [u8],
    ) -> Result<Self, Box<dyn Error + Sync + Send>> {
        if *ty == Type::JSONB {
            let mut b = [0; 1];
            raw.read_exact(&mut b)?;
            // We only support version 1 of the jsonb binary format
            if b[0] != 1 {
                return Err("unsupported JSONB encoding version".into());
            }
        }
        Ok(JsonStr(std::str::from_utf8(raw)?))
    }

    accepts!(JSON, JSONB);
}

impl ToSql for JsonString {
    fn to_sql(
        &self,
        ty: &Type,
        out: &mut BytesMut,
    ) -> Result<IsNull, Box<dyn Error + Sync + Send>>
    where
        Self: Sized,
    {
        if *ty == Type::JSONB {
            out.put_u8(1);
        }
        out.put(self.0.as_bytes());
        Ok(IsNull::No)
    }

    accepts!(JSON, JSONB);

    to_sql_checked!();
}

struct Bytea;

impl<W: Write, G: GenericRow> Serializer<W, G> for Bytea {
    fn serialize(&self, w: &mut W, row: &G, idx: usize) -> Result<()> {
        let b: &[u8] = match row.get(idx) {
            Some(b) => b,
            _ => return Ok(write!(w, "null")?),
        };
        writeln!(w, "\"{}\"", hex::encode(b))?;
        Ok(())
    }
}

impl Deserializer for Bytea {
    fn read(&self, line: &str) -> Result<Box<dyn ToSql + Sync>> {
        null_check!(line);
        let text = read_text(line)?;
        Ok(Box::new(hex::decode(text)?))
    }
}

struct Bool;

impl<W: Write, G: GenericRow> Serializer<W, G> for Bool {
    fn serialize(&self, w: &mut W, row: &G, idx: usize) -> Result<()> {
        match row.get::<Option<bool>>(idx) {
            Some(b) => writeln!(w, "{}", b)?,
            _ => writeln!(w, "null")?,
        };
        Ok(())
    }
}

impl Deserializer for Bool {
    fn read(&self, line: &str) -> Result<Box<dyn ToSql + Sync>> {
        null_check!(line);
        Ok(Box::new(bool::from_str(line)?))
    }
}
