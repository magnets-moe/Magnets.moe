use anyhow::{anyhow, Context, Result};
use postgres_types::Type;
use serde::{de::Error, Deserialize, Deserializer, Serialize, Serializer};
use tokio_postgres::Transaction;

#[derive(Serialize, Deserialize, PartialEq, Eq)]
pub struct Schema {
    pub tables: Vec<Table>,
    pub sequences: Vec<Sequence>,
}

#[derive(Serialize, Deserialize, PartialEq, Eq)]
pub struct Table {
    pub name: String,
    pub columns: Vec<Column>,
}

#[derive(Serialize, Deserialize, PartialEq, Eq)]
pub struct Sequence {
    pub name: String,
    #[serde(serialize_with = "serialize_type")]
    #[serde(deserialize_with = "deserialize_type")]
    pub ty: Type,
}

#[derive(Serialize, Deserialize, PartialEq, Eq)]
pub struct Column {
    pub name: String,
    #[serde(serialize_with = "serialize_type")]
    #[serde(deserialize_with = "deserialize_type")]
    pub ty: Type,
}

fn serialize_type<S>(t: &Type, s: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    t.oid().serialize(s)
}

fn deserialize_type<'de, D>(d: D) -> Result<Type, D::Error>
where
    D: Deserializer<'de>,
{
    let d = u32::deserialize(d)?;
    Type::from_oid(d).ok_or_else(|| D::Error::custom("unknown oid"))
}

pub async fn get_schema(tran: &Transaction<'_>) -> Result<Schema> {
    get_schema_(tran).await.context(anyhow!("cannot load schema"))
}

async fn get_schema_(tran: &Transaction<'_>) -> Result<Schema> {
    // language=sql
    let tables_ = tran
        .query(
            "select tablename from pg_catalog.pg_tables where schemaname = 'magnets' order by tablename",
            &[],
        )
        .await?;
    let mut tables = vec![];
    for table in tables_ {
        let name: String = table.get(0);
        let columns = get_columns(tran, &name).await?;
        tables.push(Table { name, columns })
    }
    // language=sql
    let sequences_ = tran
        .query(
            "select sequencename, data_type::oid from pg_sequences order by sequencename",
            &[],
        )
        .await?;
    let mut sequences = vec![];
    for sequence in sequences_ {
        let name: String = sequence.get(0);
        let ty = Type::from_oid(sequence.get(1)).unwrap();
        sequences.push(Sequence { name, ty })
    }
    Ok(Schema { tables, sequences })
}

async fn get_columns(tran: &Transaction<'_>, table: &str) -> Result<Vec<Column>> {
    // language=sql
    const STMT: &str = "
        select col.attname, col.atttypid
        from pg_namespace schm
        join pg_class tbl on schm.oid = tbl.relnamespace
        join pg_attribute col on tbl.oid = col.attrelid
        where schm.nspname = 'magnets' and tbl.relname = $1 and tbl.relkind = 'r' and col.attnum > 0
        order by col.attnum";
    let res = tran
        .query(STMT, &[&table])
        .await?
        .iter()
        .map(|r| Column {
            name: r.get(0),
            ty: Type::from_oid(r.get(1)).unwrap(),
        })
        .collect();
    Ok(res)
}
