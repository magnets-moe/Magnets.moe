use anyhow::{Context, Result};
use serde::Deserialize;

pub fn load<T: for<'a> Deserialize<'a>>() -> Result<T> {
    load_().context("cannot load config.toml")
}

fn load_<T: for<'a> Deserialize<'a>>() -> Result<T> {
    let content = std::fs::read_to_string("config.toml")?;
    Ok(toml::from_str(&content)?)
}
