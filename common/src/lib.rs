#![allow(clippy::needless_lifetimes)]
#![allow(clippy::new_without_default)]

pub use format::*;

pub use season::*;

pub mod env;
mod format;
pub mod pg;
mod season;
pub mod time;

pub struct ShowNameType;

/// Corresponds to `magnets.show_name_type`
impl ShowNameType {
    pub const ROMAJI: i32 = 1;
    pub const ENGLISH: i32 = 2;
    pub const ADDITIONAL: i32 = 3;
}
