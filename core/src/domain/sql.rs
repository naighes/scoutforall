//! The values of the domain to and from the database.
//!
//! - Enumerations: snake_case words, the database's own (not the serde
//!   names of the files): `'outside_hitter'`, `'top_international'`,
//!   `'a'`. The closed sets are checked by the schema too, and the tests
//!   compare its lists with these. The codes of the events (type,
//!   evaluation, technique) are written as in the files (`S`, `#`, `H`):
//!   the schema doesn't check them, reading does.
//! - `Home`: `'a'`, `'b'`, null for a neutral court.
//! - Ids: UUIDs as lowercase text with hyphens, only that spelling read
//!   back (the same id never spelled two ways).
//! - Times: RFC 3339 text, with the offset of the match (`matches.start`),
//!   in UTC with `Z` for the events; every digit of the fractions kept.
//!   Dates of birth `YYYY-MM-DD`.
//! - The flags of an event: their integer, the bits of a later version
//!   too. Booleans 0/1, numbers as integers (rusqlite's own).
//!
//! The types of other crates (`Uuid`, chrono's) go through `Sql`.

use crate::{
    domain::{CompetitionFormat, Home, ScoutingMode, Side, Stage, TeamRef},
    errors::{AppError, IOError},
    shapes::{
        country::CountryCode,
        enums::{
            EvalEnum, EventTypeEnum, GenderEnum, LineupRulesEnum, RoleEnum, TeamClassificationEnum,
        },
        flags::EventFlags,
        r#match::MatchSource,
        technique::TechniqueEnum,
    },
};
use chrono::{DateTime, FixedOffset, NaiveDate, SecondsFormat, Utc};
use rusqlite::types::{FromSql, FromSqlError, FromSqlResult, ToSql, ToSqlOutput, ValueRef};
use uuid::Uuid;

/// A value of the database that isn't one of the values of its kind.
#[derive(Debug, thiserror::Error)]
#[error("{what} not valid: '{text}'")]
pub struct NotValid {
    pub what: &'static str,
    pub text: String,
}

fn not_valid(what: &'static str, text: &str) -> FromSqlError {
    FromSqlError::Other(Box::new(NotValid {
        what,
        text: text.to_string(),
    }))
}

/// An enumeration kept as one word of a closed set.
pub trait DbText: Copy + PartialEq + Sized + 'static {
    /// what the values are, for the errors
    const WHAT: &'static str;
    fn all() -> &'static [Self];
    fn db_text(self) -> &'static str;

    fn from_db_text(text: &str) -> Option<Self> {
        Self::all().iter().copied().find(|v| v.db_text() == text)
    }
}

macro_rules! db_text {
    ($type:ty, $what:literal, $all:expr, { $($variant:path => $text:literal),+ $(,)? }) => {
        impl DbText for $type {
            const WHAT: &'static str = $what;
            fn all() -> &'static [Self] {
                &$all
            }
            fn db_text(self) -> &'static str {
                match self {
                    $($variant => $text),+
                }
            }
        }

        impl ToSql for $type {
            fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
                Ok(ToSqlOutput::from(self.db_text()))
            }
        }

        impl FromSql for $type {
            fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
                let text = value.as_str()?;
                Self::from_db_text(text).ok_or_else(|| not_valid(Self::WHAT, text))
            }
        }
    };
}

db_text!(RoleEnum, "role", RoleEnum::ALL, {
    RoleEnum::Libero => "libero",
    RoleEnum::OppositeHitter => "opposite_hitter",
    RoleEnum::Setter => "setter",
    RoleEnum::OutsideHitter => "outside_hitter",
    RoleEnum::MiddleBlocker => "middle_blocker",
});

db_text!(TeamClassificationEnum, "classification", TeamClassificationEnum::ALL, {
    TeamClassificationEnum::TopInternational => "top_international",
    TeamClassificationEnum::HighNational => "high_national",
    TeamClassificationEnum::NationalMidLevel => "national_mid_level",
    TeamClassificationEnum::NationalLowLevel => "national_low_level",
    TeamClassificationEnum::Regional => "regional",
    TeamClassificationEnum::LocalDivision => "local_division",
});

db_text!(GenderEnum, "gender", GenderEnum::ALL, {
    GenderEnum::Men => "men",
    GenderEnum::Women => "women",
});

db_text!(LineupRulesEnum, "lineup rules", LineupRulesEnum::ALL, {
    LineupRulesEnum::V1 => "v1",
    LineupRulesEnum::V2 => "v2",
});

db_text!(MatchSource, "source", MatchSource::ALL, {
    MatchSource::Scout4all => "scout4all",
    MatchSource::Dvw => "dvw",
});

db_text!(Side, "side", Side::ALL, {
    Side::A => "a",
    Side::B => "b",
});

db_text!(ScoutingMode, "scouting", ScoutingMode::ALL, {
    ScoutingMode::A => "a",
    ScoutingMode::Both => "both",
});

db_text!(Stage, "stage", Stage::ALL, {
    Stage::Regular => "regular",
    Stage::Pool => "pool",
    Stage::Qualification => "qualification",
    Stage::Playoff => "playoff",
    Stage::Playout => "playout",
    Stage::FinalRound => "final_round",
    Stage::Placement => "placement",
    Stage::Other => "other",
});

db_text!(CompetitionFormat, "format", CompetitionFormat::ALL, {
    CompetitionFormat::League => "league",
    CompetitionFormat::Cup => "cup",
    CompetitionFormat::Tournament => "tournament",
});

// the codes of the events, as in the files
db_text!(EventTypeEnum, "event type", EventTypeEnum::ALL, {
    EventTypeEnum::S => "S",
    EventTypeEnum::P => "P",
    EventTypeEnum::A => "A",
    EventTypeEnum::D => "D",
    EventTypeEnum::B => "B",
    EventTypeEnum::F => "F",
    EventTypeEnum::OS => "OS",
    EventTypeEnum::OE => "OE",
    EventTypeEnum::FB => "FB",
    EventTypeEnum::OF => "OF",
    EventTypeEnum::R => "R",
    EventTypeEnum::CL => "CL",
    EventTypeEnum::LR => "LR",
    EventTypeEnum::LF => "LF",
    EventTypeEnum::CS => "CS",
});

db_text!(EvalEnum, "evaluation", EvalEnum::ALL, {
    EvalEnum::Perfect => "#",
    EvalEnum::Positive => "+",
    EvalEnum::Exclamative => "!",
    EvalEnum::Over => "/",
    EvalEnum::Error => "=",
    EvalEnum::Negative => "-",
});

/// `'a'`, `'b'`, null: neutral.
impl ToSql for Home {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        match self {
            Home::A => Side::A.to_sql(),
            Home::B => Side::B.to_sql(),
            Home::Neutral => Ok(ToSqlOutput::from(rusqlite::types::Null)),
        }
    }
}

impl FromSql for Home {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        match value {
            ValueRef::Null => Ok(Home::Neutral),
            other => Side::column_result(other).map(|side| match side {
                Side::A => Home::A,
                Side::B => Home::B,
            }),
        }
    }
}

/// Two letters, upper case, as `team.json` has them.
impl ToSql for CountryCode {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::from(self.as_str()))
    }
}

impl FromSql for CountryCode {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        let text = value.as_str()?;
        CountryCode::parse(text)
            .filter(|c| c.as_str() == text)
            .ok_or_else(|| not_valid("country", text))
    }
}

impl ToSql for EventFlags {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::from(i64::from(self.bits())))
    }
}

impl FromSql for EventFlags {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        u16::column_result(value).map(EventFlags::from_bits)
    }
}

/// As in the files (`H`, `F`): read back with the type of its event
/// (`technique_from_db`).
impl ToSql for TechniqueEnum {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::from(self.to_string()))
    }
}

/// The technique of an event of `event_type`: a set type for an attack, a
/// serve type for a serve.
pub fn technique_from_db(
    event_type: EventTypeEnum,
    code: Option<&str>,
) -> Result<Option<TechniqueEnum>, AppError> {
    code.map(|code| TechniqueEnum::parse(event_type, code))
        .transpose()
}

/// Team B in its two columns: the team, or the name.
impl TeamRef {
    pub fn columns(&self) -> (Option<Uuid>, Option<&str>) {
        match self {
            TeamRef::Team(id) => (Some(*id), None),
            TeamRef::Name(name) => (None, Some(name)),
        }
    }

    /// Exactly one of the two.
    pub fn from_columns(team: Option<Uuid>, name: Option<String>) -> Result<Self, AppError> {
        match (team, name) {
            (Some(team), None) => Ok(TeamRef::Team(team)),
            (None, Some(name)) => Ok(TeamRef::Name(name)),
            (team, name) => Err(AppError::IO(IOError::Msg(format!(
                "team B not valid: team {team:?}, name {name:?}"
            )))),
        }
    }
}

/// A value of another crate as the database keeps it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sql<T>(pub T);

impl ToSql for Sql<Uuid> {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::from(self.0.to_string()))
    }
}

impl FromSql for Sql<Uuid> {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        let text = value.as_str()?;
        Uuid::parse_str(text)
            .ok()
            .filter(|id| id.to_string() == text)
            .map(Sql)
            .ok_or_else(|| not_valid("id", text))
    }
}

impl ToSql for Sql<DateTime<FixedOffset>> {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::from(
            self.0.to_rfc3339_opts(SecondsFormat::AutoSi, false),
        ))
    }
}

impl FromSql for Sql<DateTime<FixedOffset>> {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        let text = value.as_str()?;
        DateTime::parse_from_rfc3339(text)
            .map(Sql)
            .map_err(|_| not_valid("time", text))
    }
}

impl ToSql for Sql<DateTime<Utc>> {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::from(
            self.0.to_rfc3339_opts(SecondsFormat::AutoSi, true),
        ))
    }
}

impl FromSql for Sql<DateTime<Utc>> {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        Sql::<DateTime<FixedOffset>>::column_result(value).map(|t| Sql(t.0.with_timezone(&Utc)))
    }
}

impl ToSql for Sql<NaiveDate> {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::from(self.0.format("%Y-%m-%d").to_string()))
    }
}

impl FromSql for Sql<NaiveDate> {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        let text = value.as_str()?;
        NaiveDate::parse_from_str(text, "%Y-%m-%d")
            .ok()
            .filter(|date| date.format("%Y-%m-%d").to_string() == text)
            .map(Sql)
            .ok_or_else(|| not_valid("date", text))
    }
}
