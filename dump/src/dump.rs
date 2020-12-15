use crate::{pg, schema::get_schema};
use anyhow::{anyhow, Result, Context};
use std::{
    fs,
    fs::OpenOptions,
    io::{BufWriter},
    path::Path,
};
use tokio_postgres::Transaction;
use std::fs::File;
use tokio_postgres::binary_copy::BinaryCopyOutStream;
use crate::schema::{Schema, Table};
use futures::{pin_mut, StreamExt};
use postgres_types::{Oid, Type};

pub async fn dump(location: &str, tran: &Transaction<'_>) -> Result<()> {
    let path = Path::new(location);
    if path.exists() {
        return Err(anyhow!("error: {} already exists", location));
    }
    fs::create_dir_all(path)?;
    let schema = get_schema(tran).await?;
    dump_tables(path, &schema, &tran).await.context(anyhow!("cannot dump tables"))?;
    dump_sequences(path, &tran).await.context(anyhow!("cannot dump sequences"))?;
    dump_schema(path, &schema).await.context(anyhow!("cannot dump schema.json"))?;
    Ok(())
}

fn open_file(path: &Path) -> Result<BufWriter<File>> {
    Ok(BufWriter::new(OpenOptions::new().create(true).write(true).open(&path)?))
}

async fn dump_schema(path: &Path, schema: &Schema) -> Result<()> {
    let file = path.join("schema.json");
    let mut file = open_file(&file)?;
    serde_json::to_writer_pretty(&mut file, &schema)?;
    Ok(())
}

async fn dump_tables(path: &Path, schema: &Schema, tran: &Transaction<'_>) -> Result<()> {
    let root = path.join("tables");
    fs::create_dir(&root)?;
    for table in &schema.tables {
        dump_table(&root, table, &tran).await.with_context(|| anyhow!("cannot dump table {}", table.name))?;
    }
    Ok(())
}

async fn dump_sequences(path: &Path, tran: &Transaction<'_>) -> Result<()> {
    let path = path.join("sequences");
    fs::create_dir(&path)?;
    // language=sql
    let rows = tran
        .query(
            "
                select sequencename, data_type::oid, nextval('magnets.' || sequencename)
                from pg_catalog.pg_sequences
                where schemaname = 'magnets'",
            &[],
        )
        .await?;
    for row in rows {
        let sequencename: &str = row.get(0);
        let data_type: Oid = row.get(1);

        let file = path.join(sequencename);
        let mut file = open_file(&file)?;

        let serializer = pg::serializer(&Type::from_oid(data_type).unwrap());
        serializer.serialize(&mut file, &row, 2).with_context(|| anyhow!("cannot serialize sequence {}", sequencename))?;
    }
    Ok(())
}

async fn dump_table(path: &Path, table: &Table, tran: &Transaction<'_>) -> Result<()> {
    let stmt = format!("copy magnets.{} to stdout binary", table.name);
    let stream = tran.copy_out(&*stmt).await?;
    let types: Vec<_> = table.columns.iter().map(|c| c.ty.clone()).collect();
    let reader = BinaryCopyOutStream::new(stream, &types);
    pin_mut!(reader);

    let serializers: Vec<_> = table.columns.iter().map(|c| pg::serializer(&c.ty)).collect();

    let path = path.join(&table.name);
    fs::create_dir(&path)?;

    while let Some(row) = reader.next().await {
        let row = row?;
        let file = serializers[0].create_file(&path, &row)?;
        let mut file = open_file(&file)?;
        for (idx, serializer) in serializers.iter().enumerate() {
            serializer.serialize(&mut file, &row, idx)?;
        }
    }
    Ok(())
}
