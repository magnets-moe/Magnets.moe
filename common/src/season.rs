use anyhow::{anyhow, Context, Result};
use chrono::Datelike;
use std::{
    convert::TryInto,
    fmt,
    fmt::{Debug, Display, Formatter},
};

/// The part of the year in which a show aired
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Season {
    Winter,
    Spring,
    Summer,
    Fall,
}

impl Season {
    /// Converts the season to the corresponding database constant
    fn to_db(self) -> i32 {
        match self {
            Self::Winter => 1,
            Self::Spring => 2,
            Self::Summer => 3,
            Self::Fall => 4,
        }
    }

    /// Converts the season from the corresponding database constant
    fn from_db(db: i32) -> Result<Self> {
        let season = match db {
            1 => Self::Winter,
            2 => Self::Spring,
            3 => Self::Summer,
            4 => Self::Fall,
            _ => return Err(anyhow!("invalid season {}", db)),
        };
        Ok(season)
    }

    /// Parses the season name returned by the anilist API
    pub fn from_anilist_str(s: &str) -> Result<Self> {
        let s = match s {
            "WINTER" => Self::Winter,
            "SPRING" => Self::Spring,
            "SUMMER" => Self::Summer,
            "FALL" => Self::Fall,
            _ => return Err(anyhow!("invalid season {}", s)),
        };
        Ok(s)
    }

    /// Parses the string created by formatting the season with `{}`
    pub fn from_display_string(s: &str) -> Result<Self> {
        let s = match s {
            "Winter" => Self::Winter,
            "Spring" => Self::Spring,
            "Summer" => Self::Summer,
            "Fall" => Self::Fall,
            _ => return Err(anyhow!("invalid season {}", s)),
        };
        Ok(s)
    }
}

impl Display for Season {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        Debug::fmt(self, f)
    }
}

/// The year and season in which a show aired
#[derive(Copy, Clone, Eq, PartialEq)]
pub struct YearSeason {
    pub year: u16,
    pub season: Season,
}

impl YearSeason {
    /// Returns the database format of this YearSeason
    ///
    /// Example: Spring 2020: 202002
    pub fn to_db(&self) -> i32 {
        self.year as i32 * 100 + self.season.to_db()
    }

    /// Parses the database format of a YearSeason
    pub fn from_db(db: i32) -> Result<Self> {
        let year = (db / 100)
            .try_into()
            .with_context(|| format!("invalid year {}", db / 100))?;
        let season = Season::from_db(db % 100)?;
        Ok(YearSeason { year, season })
    }

    /// Returns the season at the time of the function call
    pub fn current() -> YearSeason {
        let today = chrono::Utc::today();
        let year = today.year() as u16;
        let season = match today.month() {
            1..=3 => Season::Winter,
            4..=6 => Season::Spring,
            7..=9 => Season::Summer,
            10..=12 => Season::Fall,
            _ => unreachable!(),
        };
        YearSeason { year, season }
    }

    /// Returns a pretty printed version of this YearSeason
    pub fn display_name(&self) -> String {
        format!("{} {}", self.season, self.year)
    }

    /// Returns the previous season
    pub fn prev(&self) -> Self {
        let mut year = self.year;
        let season = match self.season {
            Season::Winter => {
                year -= 1;
                Season::Fall
            }
            Season::Spring => Season::Winter,
            Season::Summer => Season::Spring,
            Season::Fall => Season::Summer,
        };
        Self { year, season }
    }

    /// Returns the next season
    pub fn next(&self) -> Self {
        let mut year = self.year;
        let season = match self.season {
            Season::Winter => Season::Spring,
            Season::Spring => Season::Summer,
            Season::Summer => Season::Fall,
            Season::Fall => {
                year += 1;
                Season::Winter
            }
        };
        Self { year, season }
    }

    /// Returns a unique identifier of this YearSeason suitable for use in a url
    pub fn to_url_str(&self) -> String {
        format!("{}-{}", self.season, self.year)
    }

    /// Parses a YearSeason from its url string
    pub fn from_url_str(s: &str) -> Result<Self> {
        let (l, r) = match s.find('-') {
            Some(p) => (&s[..p], &s[p + 1..]),
            _ => return Err(anyhow!("invalid year season {}", s)),
        };
        let season = Season::from_display_string(l)?;
        let year = match r.parse() {
            Ok(y) => y,
            _ => return Err(anyhow!("invalid year season {}", s)),
        };
        Ok(YearSeason { year, season })
    }
}

impl Debug for YearSeason {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.year, self.season)
    }
}
