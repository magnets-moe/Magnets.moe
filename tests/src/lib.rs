use anyhow::Result;

use common::pg::PgConnector;
use std::collections::HashMap;
use testcontainers::{
    clients::Cli, core::Port, Container, Docker, Image, WaitForMessage,
};

#[derive(Debug)]
struct Postgres {
    arguments: PostgresArgs,
    env_vars: HashMap<String, String>,
    ports: Option<Vec<Port>>,
    version: u8,
}

#[derive(Default, Debug, Clone)]
struct PostgresArgs {}

impl IntoIterator for PostgresArgs {
    type Item = String;
    type IntoIter = ::std::vec::IntoIter<String>;

    fn into_iter(self) -> Self::IntoIter {
        vec![].into_iter()
    }
}

impl Default for Postgres {
    fn default() -> Self {
        let mut env_vars = HashMap::new();
        env_vars.insert("POSTGRES_DB".to_owned(), "postgres".to_owned());
        env_vars.insert("POSTGRES_HOST_AUTH_METHOD".into(), "trust".into());

        Self {
            arguments: PostgresArgs::default(),
            env_vars,
            ports: None,
            version: 13,
        }
    }
}

impl Image for Postgres {
    type Args = PostgresArgs;
    type EnvVars = HashMap<String, String>;
    type Volumes = HashMap<String, String>;
    type EntryPoint = std::convert::Infallible;

    fn descriptor(&self) -> String {
        format!("postgres:{}-alpine", self.version)
    }

    fn wait_until_ready<D: Docker>(&self, container: &Container<'_, D, Self>) {
        container
            .logs()
            .stderr
            .wait_for_message("database system is ready to accept connections")
            .unwrap();
    }

    fn args(&self) -> Self::Args {
        self.arguments.clone()
    }

    fn env_vars(&self) -> Self::EnvVars {
        self.env_vars.clone()
    }

    fn volumes(&self) -> Self::Volumes {
        HashMap::new()
    }

    fn ports(&self) -> Option<Vec<Port>> {
        self.ports.clone()
    }

    fn with_args(self, arguments: Self::Args) -> Self {
        Self { arguments, ..self }
    }
}

pub struct Testdb<'a> {
    _container: Container<'a, Cli, Postgres>,
    pub connector: PgConnector,
}

impl<'a> Testdb<'a> {
    pub async fn new(docker: &'a Cli) -> Result<Testdb<'a>> {
        let container = docker.run(Postgres::default());
        let connection_string = format!(
            "dbname=postgres user=postgres host=localhost port={}",
            container.get_host_port(5432).unwrap()
        );
        let res = Testdb {
            _container: container,
            connector: PgConnector::new(connection_string),
        };
        let client = res.connector.connect().await?;
        client
            .simple_query(include_str!("../../sql/init.sql"))
            .await?;
        Ok(res)
    }
}

#[tokio::test]
async fn f() -> Result<()> {
    common::env::configure_logger();
    // let docker = Cli::default();
    // let testdb = Testdb::new(&docker).await?;
    // loop {}
    Ok(())
}
