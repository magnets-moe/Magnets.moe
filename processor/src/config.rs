use common::time::StdDuration;
use serde::{de::Error, Deserialize, Deserializer};

#[derive(Debug, Deserialize)]
pub struct Config {
    pub db: Db,
    pub anilist: Anilist,
    pub nyaa: Nyaa,
    pub http: Http,
}

#[derive(Debug, Deserialize)]
pub struct Db {
    pub connection_string: String,
}

#[derive(Debug, Deserialize)]
pub struct Http {
    pub user_agent: String,
}

#[derive(Debug, Deserialize)]
pub struct Anilist {
    #[serde(deserialize_with = "deserialize_duration")]
    pub startup_grace_period: StdDuration,
    #[serde(deserialize_with = "deserialize_duration")]
    pub schedule_poll_interval: StdDuration,
    #[serde(deserialize_with = "deserialize_duration")]
    pub shows_poll_interval: StdDuration,
}

#[derive(Debug, Deserialize)]
pub struct Nyaa {
    #[serde(deserialize_with = "deserialize_duration")]
    pub scrape_interval: StdDuration,
}

fn deserialize_duration<'de, D>(d: D) -> Result<StdDuration, D::Error>
where
    D: Deserializer<'de>,
{
    let s: String = Deserialize::deserialize(d)?;
    match parse_duration::parse(&s) {
        Ok(d) => Ok(d),
        Err(e) => Err(D::Error::custom(format!(
            "cannot parse duration `{}`: {}",
            s, e
        ))),
    }
}
