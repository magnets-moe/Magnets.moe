use anyhow::{anyhow, Result};
use core::fmt;
use std::fmt::{Display, Formatter};

/// Format of a "show"
///
/// We use the generic term "show" for any of these.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Format {
    Tv,
    TvShort,
    Movie,
    Special,
    Ova,
    Ona,
}

impl Format {
    /// Parses format string returned by the anilist API
    pub fn from_anilist(n: &str) -> Result<Format> {
        let v = match n {
            "TV" => Self::Tv,
            "TV_SHORT" => Self::TvShort,
            "MOVIE" => Self::Movie,
            "SPECIAL" => Self::Special,
            "OVA" => Self::Ova,
            "ONA" => Self::Ona,
            _ => return Err(anyhow!("invalid format {}", n)),
        };
        Ok(v)
    }

    /// Returns the database constant of the format
    pub fn to_db(self) -> i32 {
        match self {
            Self::Tv => 1,
            Self::TvShort => 2,
            Self::Movie => 3,
            Self::Special => 4,
            Self::Ova => 5,
            Self::Ona => 6,
        }
    }

    /// Parses a database format constant
    pub fn from_db(n: i32) -> Result<Self> {
        let v = match n {
            1 => Self::Tv,
            2 => Self::TvShort,
            3 => Self::Movie,
            4 => Self::Special,
            5 => Self::Ova,
            6 => Self::Ona,
            _ => return Err(anyhow!("invalid format {}", n)),
        };
        Ok(v)
    }

    /// Formats the format as a human-readable string
    pub fn as_str(self) -> &'static str {
        match self {
            Format::Tv => "TV Show",
            Format::TvShort => "TV Short",
            Format::Movie => "Movie",
            Format::Special => "Special",
            Format::Ova => "OVA",
            Format::Ona => "ONA",
        }
    }
}

impl Display for Format {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
