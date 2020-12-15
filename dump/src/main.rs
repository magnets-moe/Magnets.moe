#![deny(unused_must_use)]

mod dump;
mod load;
mod pg;
mod schema;

use anyhow::Result;
use clap::{App, AppSettings, Arg, SubCommand};
use common::pg::PgConnector;

#[tokio::main(basic_scheduler)]
async fn main() -> Result<()> {
    let matches = App::new("magnets.moe dumper")
        .about("Dumps and loads the magnets.moe database")
        .arg(
            Arg::with_name("location")
                .short("l")
                .long("location")
                .value_name("LOCATION")
                .help("Sets the location of the dumped database")
                .takes_value(true)
                .default_value("data"),
        )
        .arg(
            Arg::with_name("connection_string")
                .short("c")
                .long("connection-string")
                .value_name("CONNECTION_STRING")
                .help("Sets the connection string")
                .required(true)
                .takes_value(true),
        )
        .setting(AppSettings::SubcommandRequiredElseHelp)
        .subcommand(SubCommand::with_name("dump").about("Dumps the database"))
        .subcommand(SubCommand::with_name("load").about("Loads the database"))
        .get_matches();
    let location = matches.value_of("location").unwrap();
    let connection_string = matches.value_of("connection_string").unwrap();
    let connector = PgConnector::new(connection_string.to_string());
    let mut con = connector.connect().await?;
    let tran = common::pg::transaction(&mut con).await?;
    match matches.subcommand() {
        ("dump", _) => dump::dump(location, &tran).await?,
        ("load", _) => load::load(location, &tran).await?,
        _ => unreachable!(),
    }
    tran.commit().await?;
    Ok(())
}
