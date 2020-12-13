use crate::{cache::Cache, db::Statements};
use actix_web::web::Bytes;
use common::pg::{PgConnector, PgHolder};
use std::sync::Arc;

pub struct Global {
    pub shows: Cache<Bytes>,
    pub pg_connector: PgConnector,
}

pub struct State {
    pub global: Arc<Global>,
    pub pg: Arc<PgHolder<Statements>>,
}
