//! The core of scout4all for the native apps: UniFFI generates the Kotlin
//! and Swift bindings of what's exported here.
//!
//! The API is blocking for now: the app calls it off the main thread
//! (`Dispatchers.IO` on Android) and a tokio runtime here drives the async
//! providers of the core. The final app must never block the UI: the plan is
//! to move to the `async fn` of UniFFI.

use scout4all_core::{
    errors::AppError,
    localization::{Labels, Language},
    logging::logger::init_logger,
    player_form::{PlayerForm, PlayerFormError, removal},
    providers::{
        match_reader::MatchReader,
        store::{
            match_reader::StoreMatchReader, set_reader::StoreSetReader,
            team_reader::StoreTeamReader, team_writer::StoreTeamWriter,
        },
        team_reader::TeamReader,
        team_writer::TeamWriter,
    },
    shapes::{
        enums::{FriendlyName, GenderEnum, LanguageEnum, RoleEnum, TeamClassificationEnum},
        player::PlayerEntry,
        team::TeamEntry,
    },
    storage::{Store, fs::FsStore},
    team_form::{TeamForm, TeamFormError},
};
use std::{path::PathBuf, str::FromStr, sync::Arc};
use tokio::runtime::{Builder, Runtime};
use uuid::Uuid;

uniffi::setup_scaffolding!();

// the types of the core the app sees as they are

#[uniffi::remote(Enum)]
pub enum TeamClassificationEnum {
    TopInternational,
    HighNational,
    NationalMidLevel,
    NationalLowLevel,
    Regional,
    LocalDivision,
}

#[uniffi::remote(Enum)]
pub enum GenderEnum {
    Men,
    Women,
}

#[uniffi::remote(Enum)]
pub enum RoleEnum {
    Libero,
    OppositeHitter,
    Setter,
    OutsideHitter,
    MiddleBlocker,
}

#[uniffi::remote(Record)]
pub struct PlayerForm {
    pub name: String,
    pub role: Option<RoleEnum>,
    pub number: String,
}

#[uniffi::remote(Record)]
pub struct TeamForm {
    pub name: String,
    pub classification: Option<TeamClassificationEnum>,
    pub gender: Option<GenderEnum>,
    pub year: String,
}

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum MobileError {
    #[error("{reason}")]
    Failed { reason: String },
    /// a field of the team form, which the app shows next to it
    #[error("{reason}")]
    InvalidTeam { field: TeamField, reason: String },
    /// a field of the player form
    #[error("{reason}")]
    InvalidPlayer { field: PlayerField, reason: String },
}

impl From<AppError> for MobileError {
    fn from(e: AppError) -> Self {
        MobileError::Failed {
            reason: e.to_string(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum TeamField {
    Name,
    Classification,
    Gender,
    Year,
}

impl From<TeamFormError> for TeamField {
    fn from(e: TeamFormError) -> Self {
        match e {
            TeamFormError::EmptyName => TeamField::Name,
            TeamFormError::MissingClassification => TeamField::Classification,
            TeamFormError::MissingGender => TeamField::Gender,
            TeamFormError::InvalidYear => TeamField::Year,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum PlayerField {
    Name,
    Role,
    Number,
}

impl From<PlayerFormError> for PlayerField {
    fn from(e: PlayerFormError) -> Self {
        match e {
            PlayerFormError::EmptyName => PlayerField::Name,
            PlayerFormError::MissingRole => PlayerField::Role,
            PlayerFormError::InvalidNumber | PlayerFormError::NumberInUse => PlayerField::Number,
        }
    }
}

/// A role as the form offers it.
#[derive(Debug, PartialEq, uniffi::Record)]
pub struct RoleChoice {
    pub value: RoleEnum,
    pub name: String,
}

/// A player as the team page shows it.
#[derive(Debug, PartialEq, uniffi::Record)]
pub struct Player {
    pub id: String,
    pub name: String,
    pub number: u8,
    pub role: Option<String>,
    /// what the form starts from, to edit the player
    pub form: PlayerForm,
}

/// A classification as the form offers it.
#[derive(Debug, PartialEq, uniffi::Record)]
pub struct ClassificationChoice {
    pub value: TeamClassificationEnum,
    pub name: String,
    pub description: String,
}

/// A gender as the form offers it.
#[derive(Debug, PartialEq, uniffi::Record)]
pub struct GenderChoice {
    pub value: GenderEnum,
    pub name: String,
}

/// A team as its page shows it.
#[derive(Debug, PartialEq, uniffi::Record)]
pub struct TeamDetails {
    pub id: String,
    pub name: String,
    pub year: u16,
    pub classification: Option<String>,
    pub gender: Option<String>,
    /// the active players, by number
    pub players: Vec<Player>,
    /// what the form starts from, to edit the team
    pub form: TeamForm,
}

/// A team as the list shows it: one that can't be read keeps its folder.
#[derive(Debug, PartialEq, uniffi::Enum)]
pub enum TeamListing {
    Team {
        id: String,
        name: String,
        year: u16,
        players: u32,
    },
    Unreadable {
        folder: String,
    },
}

/// A match as the team page lists it: one that can't be read, or whose sets
/// don't replay, keeps its folder.
#[derive(Debug, PartialEq, uniffi::Enum)]
pub enum MatchListing {
    Match {
        id: String,
        opponent: String,
        home: bool,
        /// RFC 3339, with the offset of where it was played: the app formats
        /// it in its language
        date: String,
        /// the sets won by the team and by the opponent
        won: u8,
        lost: u8,
        finished: bool,
    },
    Unreadable {
        folder: String,
    },
}

/// The data of the app, in a folder with the same layout as `~/.scout4all`.
#[derive(uniffi::Object)]
pub struct Scout4All {
    runtime: Runtime,
    language: Language,
    team_reader: StoreTeamReader,
    team_writer: StoreTeamWriter,
    match_reader: StoreMatchReader,
}

#[uniffi::export]
impl Scout4All {
    /// The data in `data_dir`; `language` is a code like `"it"`, English when
    /// it isn't supported.
    #[uniffi::constructor]
    pub fn new(data_dir: String, language: String) -> Result<Arc<Self>, MobileError> {
        let data_dir = PathBuf::from(data_dir);
        init_logger(data_dir.join("scout4all.log"));
        let language = Language::new(LanguageEnum::from_str(&language).unwrap_or(LanguageEnum::En));
        let store: Arc<dyn Store> = Arc::new(FsStore::new(&data_dir));
        let runtime = Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| MobileError::Failed {
                reason: e.to_string(),
            })?;
        Ok(Arc::new(Self {
            runtime,
            team_reader: StoreTeamReader::new(store.clone(), language.clone()),
            team_writer: StoreTeamWriter::new(store.clone()),
            match_reader: StoreMatchReader::new(
                store.clone(),
                Arc::new(StoreSetReader::new(store, language.clone())),
                language.clone(),
            ),
            language,
        }))
    }

    /// The teams, sorted by name.
    pub fn teams(&self) -> Result<Vec<TeamListing>, MobileError> {
        let mut teams: Vec<TeamListing> = self
            .runtime
            .block_on(self.team_reader.read_all())?
            .into_iter()
            .map(|listed| match listed {
                Ok(team) => TeamListing::Team {
                    id: team.id.to_string(),
                    players: count(&team),
                    name: team.name,
                    year: team.year,
                },
                Err(unreadable) => TeamListing::Unreadable {
                    folder: unreadable.id,
                },
            })
            .collect();
        teams.sort_by_key(|t| match t {
            TeamListing::Team { name, .. } => name.to_lowercase(),
            TeamListing::Unreadable { folder } => folder.to_lowercase(),
        });
        Ok(teams)
    }

    /// The team with this id.
    pub fn team(&self, id: String) -> Result<TeamDetails, MobileError> {
        let team = self.read_team(&id)?;
        let labels = self.language.labels();
        Ok(TeamDetails {
            id: team.id.to_string(),
            name: team.name.clone(),
            year: team.year,
            classification: team
                .classification
                .map(|c| c.friendly_name(labels).to_string()),
            gender: team.gender.map(|g| g.friendly_name(labels).to_string()),
            players: players(&team, labels),
            form: TeamForm::of(&team),
        })
    }

    /// The matches of the team, the most recent first, then the unreadable
    /// ones.
    pub fn matches(&self, team_id: String) -> Result<Vec<MatchListing>, MobileError> {
        let team = self.read_team(&team_id)?;
        Ok(self
            .runtime
            .block_on(self.match_reader.read_all(&team))?
            .into_iter()
            .map(|listed| match listed.map(|m| (m.get_status(), m)) {
                Ok((Ok(status), m)) => MatchListing::Match {
                    date: m.date.to_rfc3339(),
                    id: m.id,
                    opponent: m.opponent,
                    home: m.home,
                    won: status.us_wins,
                    lost: status.them_wins,
                    finished: status.match_finished,
                },
                Ok((Err(_), m)) => MatchListing::Unreadable { folder: m.id },
                Err(unreadable) => MatchListing::Unreadable {
                    folder: unreadable.id,
                },
            })
            .collect())
    }

    /// An empty form for a new team, with the year given.
    pub fn new_team_form(&self, year: u16) -> TeamForm {
        TeamForm {
            name: String::new(),
            classification: None,
            gender: None,
            year: year.to_string(),
        }
    }

    pub fn classifications(&self) -> Vec<ClassificationChoice> {
        let labels = self.language.labels();
        TeamClassificationEnum::ALL
            .iter()
            .map(|c| ClassificationChoice {
                value: *c,
                name: c.friendly_name(labels).to_string(),
                description: c.friendly_description(labels).to_string(),
            })
            .collect()
    }

    pub fn genders(&self) -> Vec<GenderChoice> {
        let labels = self.language.labels();
        GenderEnum::ALL
            .iter()
            .map(|g| GenderChoice {
                value: *g,
                name: g.friendly_name(labels).to_string(),
            })
            .collect()
    }

    pub fn roles(&self) -> Vec<RoleChoice> {
        let labels = self.language.labels();
        RoleEnum::ALL
            .iter()
            .map(|r| RoleChoice {
                value: *r,
                name: r.friendly_name(labels).to_string(),
            })
            .collect()
    }

    /// Adds a player to the team (no `player_id`) or updates the one with
    /// `player_id`; returns its id.
    pub fn save_player(
        &self,
        team_id: String,
        player_id: Option<String>,
        form: PlayerForm,
    ) -> Result<String, MobileError> {
        let mut team = self.read_team(&team_id)?;
        let existing = player_id.map(|id| find_player(&team, &id)).transpose()?;
        let input =
            form.validate(&team, existing.as_ref())
                .map_err(|e| MobileError::InvalidPlayer {
                    field: e.into(),
                    reason: e.message(self.language.labels()),
                })?;
        let player = self
            .runtime
            .block_on(self.team_writer.save_player(input, &mut team))?;
        Ok(player.id.to_string())
    }

    /// Removes a player, who stays for the matches already played.
    pub fn remove_player(&self, team_id: String, player_id: String) -> Result<(), MobileError> {
        let mut team = self.read_team(&team_id)?;
        let player = find_player(&team, &player_id)?;
        self.runtime
            .block_on(self.team_writer.save_player(removal(&player), &mut team))?;
        Ok(())
    }

    /// Creates a team (no `id`) or updates the one with `id`; returns its id.
    pub fn save_team(&self, id: Option<String>, form: TeamForm) -> Result<String, MobileError> {
        let existing = id.map(|id| self.read_team(&id)).transpose()?;
        let input = form
            .validate(existing.as_ref())
            .map_err(|e| MobileError::InvalidTeam {
                field: e.into(),
                reason: e.message(self.language.labels()),
            })?;
        let team = self.runtime.block_on(self.team_writer.save(input))?;
        Ok(team.id.to_string())
    }
}

impl Scout4All {
    fn read_team(&self, id: &str) -> Result<TeamEntry, MobileError> {
        let id = parse_id(id)?;
        Ok(self.runtime.block_on(self.team_reader.read_single(&id))?)
    }
}

fn parse_id(id: &str) -> Result<Uuid, MobileError> {
    Uuid::parse_str(id).map_err(|e| MobileError::Failed {
        reason: e.to_string(),
    })
}

fn find_player(team: &TeamEntry, id: &str) -> Result<PlayerEntry, MobileError> {
    let id = parse_id(id)?;
    team.find_player(id)
        .cloned()
        .ok_or_else(|| MobileError::Failed {
            reason: format!("no player {id} in the team"),
        })
}

fn players(team: &TeamEntry, labels: &Labels) -> Vec<Player> {
    let mut players: Vec<Player> = team
        .active_players()
        .into_iter()
        .map(|p| Player {
            id: p.id.to_string(),
            name: p.name.clone(),
            number: p.number,
            role: p.role.map(|r| r.friendly_name(labels).to_string()),
            form: PlayerForm::of(p),
        })
        .collect();
    players.sort_by_key(|p| p.number);
    players
}

/// The players still in the team: the deleted ones stay for the old matches.
fn count(team: &TeamEntry) -> u32 {
    u32::try_from(team.active_players().len()).unwrap_or(u32::MAX)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::unreachable)]
mod tests {
    use super::*;
    use chrono::DateTime;
    use scout4all_core::providers::{
        match_writer::MatchWriter, store::match_writer::StoreMatchWriter,
    };

    #[test]
    fn an_empty_folder_has_no_teams() {
        let dir = tempfile::tempdir().unwrap();
        let app = Scout4All::new(dir.path().to_string_lossy().into_owned(), "it".into()).unwrap();
        assert_eq!(app.teams().unwrap(), vec![]);
    }

    #[test]
    fn a_team_folder_that_is_not_an_id_is_unreadable() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("not-a-team")).unwrap();
        let app = Scout4All::new(dir.path().to_string_lossy().into_owned(), "xx".into()).unwrap();
        assert_eq!(
            app.teams().unwrap(),
            vec![TeamListing::Unreadable {
                folder: "not-a-team".into()
            }]
        );
    }

    fn form(name: &str) -> TeamForm {
        TeamForm {
            name: name.into(),
            classification: Some(TeamClassificationEnum::Regional),
            gender: Some(GenderEnum::Women),
            year: "2026".into(),
        }
    }

    #[test]
    fn a_new_team_is_listed_and_can_be_edited() {
        let dir = tempfile::tempdir().unwrap();
        let app = Scout4All::new(dir.path().to_string_lossy().into_owned(), "it".into()).unwrap();
        let id = app.save_team(None, form(" Cortona ")).unwrap();
        let team = app.team(id.clone()).unwrap();
        assert_eq!(team.name, "Cortona");
        assert_eq!(team.gender.as_deref(), Some("femminile"));
        assert_eq!(team.form, form("Cortona"));

        let edited = app
            .save_team(Some(id.clone()), form("Cortona Volley"))
            .unwrap();
        assert_eq!(edited, id);
        assert_eq!(
            app.teams().unwrap(),
            vec![TeamListing::Team {
                id,
                name: "Cortona Volley".into(),
                year: 2026,
                players: 0,
            }]
        );
    }

    #[test]
    fn an_invalid_form_names_its_field() {
        let dir = tempfile::tempdir().unwrap();
        let app = Scout4All::new(dir.path().to_string_lossy().into_owned(), "en".into()).unwrap();
        let result = app.save_team(
            None,
            TeamForm {
                year: "26".into(),
                ..form("Cortona")
            },
        );
        match result {
            Err(MobileError::InvalidTeam { field, reason }) => {
                assert_eq!(field, TeamField::Year);
                assert_eq!(reason, "year must be a 4-digit number");
            }
            other => unreachable!("expected an invalid year, got {other:?}"),
        }
        assert_eq!(app.teams().unwrap(), vec![]);
    }

    #[test]
    fn the_new_form_has_the_year_given() {
        let dir = tempfile::tempdir().unwrap();
        let app = Scout4All::new(dir.path().to_string_lossy().into_owned(), "en".into()).unwrap();
        assert_eq!(app.new_team_form(2026).year, "2026");
        assert_eq!(
            app.classifications().len(),
            TeamClassificationEnum::ALL.len()
        );
        assert_eq!(app.genders()[0].name, "men");
    }

    fn player(name: &str, number: &str) -> PlayerForm {
        PlayerForm {
            name: name.into(),
            role: Some(RoleEnum::Setter),
            number: number.into(),
        }
    }

    #[test]
    fn players_are_added_edited_and_removed() {
        let dir = tempfile::tempdir().unwrap();
        let app = Scout4All::new(dir.path().to_string_lossy().into_owned(), "it".into()).unwrap();
        let team = app.save_team(None, form("Cortona")).unwrap();
        let anna = app
            .save_player(team.clone(), None, player("Anna", "9"))
            .unwrap();
        app.save_player(team.clone(), None, player("Bea", "3"))
            .unwrap();
        app.save_player(team.clone(), Some(anna.clone()), player("Anna", "12"))
            .unwrap();

        let players = app.team(team.clone()).unwrap().players;
        let listed: Vec<(&str, u8)> = players
            .iter()
            .map(|p| (p.name.as_str(), p.number))
            .collect();
        assert_eq!(listed, vec![("Bea", 3), ("Anna", 12)]);
        assert_eq!(players[1].id, anna);
        assert_eq!(players[1].role.as_deref(), Some("palleggiatore"));
        assert_eq!(players[1].form, player("Anna", "12"));

        app.remove_player(team.clone(), anna).unwrap();
        let players = app.team(team.clone()).unwrap().players;
        assert_eq!(players.len(), 1);
        // the number of a removed player is free again
        app.save_player(team, None, player("Carla", "12")).unwrap();
    }

    #[test]
    fn matches_are_listed_the_most_recent_first() {
        let dir = tempfile::tempdir().unwrap();
        let app = Scout4All::new(dir.path().to_string_lossy().into_owned(), "it".into()).unwrap();
        let team_id = app.save_team(None, form("Cortona")).unwrap();
        assert_eq!(app.matches(team_id.clone()).unwrap(), vec![]);

        let team = app.read_team(&team_id).unwrap();
        let writer = StoreMatchWriter::new(Arc::new(FsStore::new(dir.path())));
        for (opponent, date, home) in [
            ("Arezzo", "2026-10-04T18:00:00+02:00", true),
            ("Siena", "2026-10-11T21:00:00+02:00", false),
        ] {
            let date = DateTime::parse_from_rfc3339(date).unwrap();
            app.runtime
                .block_on(writer.create(&team, opponent.into(), date, home, 6))
                .unwrap();
        }
        std::fs::create_dir(dir.path().join(&team_id).join("not-a-match")).unwrap();

        assert_eq!(
            app.matches(team_id).unwrap(),
            vec![
                MatchListing::Match {
                    id: "2026-10-11_siena".into(),
                    opponent: "Siena".into(),
                    home: false,
                    date: "2026-10-11T21:00:00+02:00".into(),
                    won: 0,
                    lost: 0,
                    finished: false,
                },
                MatchListing::Match {
                    id: "2026-10-04_arezzo".into(),
                    opponent: "Arezzo".into(),
                    home: true,
                    date: "2026-10-04T18:00:00+02:00".into(),
                    won: 0,
                    lost: 0,
                    finished: false,
                },
                MatchListing::Unreadable {
                    folder: "not-a-match".into()
                },
            ]
        );
    }

    #[test]
    fn a_number_in_use_names_its_field() {
        let dir = tempfile::tempdir().unwrap();
        let app = Scout4All::new(dir.path().to_string_lossy().into_owned(), "en".into()).unwrap();
        let team = app.save_team(None, form("Cortona")).unwrap();
        app.save_player(team.clone(), None, player("Anna", "9"))
            .unwrap();
        match app.save_player(team, None, player("Bea", "9")) {
            Err(MobileError::InvalidPlayer { field, reason }) => {
                assert_eq!(field, PlayerField::Number);
                assert_eq!(reason, "number already in use");
            }
            other => unreachable!("expected a number in use, got {other:?}"),
        }
    }
}
