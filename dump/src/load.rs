use crate::{
    pg,
    schema::{get_schema, Schema, Sequence, Table},
};
use anyhow::{anyhow, Context, Result};
use futures::pin_mut;
use postgres_types::ToSql;
use std::{
    fs::File,
    io::{BufRead, BufReader},
    path::Path,
    pin::Pin,
};
use tokio_postgres::{binary_copy::BinaryCopyInWriter, Transaction};
use walkdir::{DirEntry, WalkDir};

pub async fn load(root: &str, tran: &Transaction<'_>) -> Result<()> {
    let root = Path::new(root);
    let created_schema = get_schema(tran).await?;
    let data_schema =
        read_schema_json(root).context(anyhow!("cannot deserialize schema.json"))?;
    if created_schema != data_schema {
        return Err(anyhow!("schema.json is different from actual schema"));
    }
    load_tables(root, &data_schema, &tran)
        .await
        .context(anyhow!("cannot load tables"))?;
    load_sequences(root, &data_schema, &tran)
        .await
        .context(anyhow!("cannot load sequences"))?;
    Ok(())
}

fn read_schema_json(root: &Path) -> Result<Schema> {
    Ok(serde_json::from_str(&std::fs::read_to_string(
        root.join("schema.json"),
    )?)?)
}

async fn load_sequences(
    root: &Path,
    schema: &Schema,
    tran: &Transaction<'_>,
) -> Result<()> {
    let root = root.join("sequences");
    for sequence in &schema.sequences {
        load_sequence(&root, sequence, tran).await?;
    }
    Ok(())
}

async fn load_sequence(
    root: &Path,
    sequence: &Sequence,
    tran: &Transaction<'_>,
) -> Result<()> {
    let path = root.join(&sequence.name);
    let value = std::fs::read_to_string(&path)?;
    let value = pg::deserializer(&sequence.ty).read(&value.trim())?;
    // language=sql
    let sql = format!("select setval('magnets.{}', $1, false)", sequence.name);
    tran.execute(&*sql, &[&*value]).await?;
    Ok(())
}

async fn load_tables(root: &Path, schema: &Schema, tran: &Transaction<'_>) -> Result<()> {
    let root = root.join("tables");
    for table in &schema.tables {
        check_table_empty(table, tran).await?;
        load_table(&root, table, tran)
            .await
            .with_context(|| anyhow!("cannot load table {}", table.name))?;
    }
    Ok(())
}

async fn check_table_empty(table: &Table, tran: &Transaction<'_>) -> Result<()> {
    let sql = format!("select count(*) from magnets.{}", table.name);
    let row = tran.query_one(&*sql, &[]).await?;
    if row.get::<_, i64>(0) > 0 {
        return Err(anyhow!("table {} is not empty", table.name));
    }
    Ok(())
}

async fn load_table(dir: &Path, table: &Table, tran: &Transaction<'_>) -> Result<()> {
    let stmt = format!("copy magnets.{} from stdin binary", table.name);
    let sink = tran.copy_in(&*stmt).await?;
    let types: Vec<_> = table.columns.iter().map(|c| c.ty.clone()).collect();
    let writer = BinaryCopyInWriter::new(sink, &types);
    pin_mut!(writer);

    let dir = dir.join(&table.name);
    for entry in WalkDir::new(&dir) {
        let entry = entry?;
        load_table_row(&table, &entry, writer.as_mut())
            .await
            .with_context(|| anyhow!("cannot load row {}", entry.path().display()))?;
    }

    writer.finish().await?;
    Ok(())
}

async fn load_table_row(
    table: &Table,
    entry: &DirEntry,
    writer: Pin<&mut BinaryCopyInWriter>,
) -> Result<()> {
    if !entry.file_type().is_file() {
        return Ok(());
    }
    let mut columns = vec![];
    let reader = BufReader::new(File::open(entry.path())?);
    for (idx, line) in reader.lines().enumerate() {
        if idx >= table.columns.len() {
            return Err(anyhow!("too many columns"));
        }
        let ty = pg::deserializer(&table.columns[idx].ty);
        columns.push(ty.read(&line?)?);
    }
    if columns.len() < table.columns.len() {
        return Err(anyhow!("too few columns"));
    }
    writer
        .write_raw(columns.iter().map(|v| {
            let val: &dyn ToSql = &**v;
            val
        }))
        .await?;
    Ok(())
}
