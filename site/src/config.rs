use serde::{de::Error, Deserialize, Deserializer};
use std::{
    fmt,
    fmt::Display,
    net::{SocketAddr, ToSocketAddrs},
    path::PathBuf,
};

#[derive(Debug, Deserialize)]
pub struct Config {
    pub db: Db,
    pub http: Http,
}

#[derive(Debug, Deserialize)]
pub struct Db {
    pub connection_string: String,
}

#[derive(Debug, Deserialize)]
pub struct Http {
    #[serde(deserialize_with = "parse_addr_type")]
    pub listen_addr: Vec<AddrType>,
}

#[derive(Debug)]
pub enum AddrType {
    Ip(SocketAddr),
    Uds(PathBuf),
}

impl Display for AddrType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AddrType::Ip(addr) => Display::fmt(addr, f),
            AddrType::Uds(addr) => Display::fmt(&addr.display(), f),
        }
    }
}

fn parse_addr_type<'de, D>(d: D) -> Result<Vec<AddrType>, D::Error>
where
    D: Deserializer<'de>,
{
    let addrs: Vec<String> = Deserialize::deserialize(d)?;
    let mut res = vec![];
    for addr in addrs {
        const UNIX: &str = "unix:";
        if let Some(stripped) = addr.strip_prefix(UNIX) {
            res.push(AddrType::Uds(stripped.to_string().into()));
        } else {
            match addr.to_socket_addrs() {
                Ok(addrs) => res.extend(addrs.map(AddrType::Ip)),
                Err(e) => {
                    return Err(D::Error::custom(format!(
                        "cannot parse `{}`: {}",
                        addr, e
                    )));
                }
            }
        }
    }
    Ok(res)
}
