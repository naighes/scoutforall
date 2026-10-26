//! The team a new installation starts with, to try the app before typing
//! one: the Italian women's national team. Its ids are new each time, so
//! the statistics uploaded from different installations don't mix.

use crate::{
    errors::AppError,
    localization::Labels,
    match_form::MatchForm,
    providers::team_writer::{TeamInput, TeamWriter},
    set_start::SetStartForm,
    shapes::{
        enums::{GenderEnum, RoleEnum, TeamClassificationEnum, TeamSideEnum},
        player::PlayerEntry,
        team::TeamEntry,
    },
    storage::{Store, StoreEntry, layout},
};
use chrono::{DateTime, Datelike, FixedOffset, Local};

/// The players: number, name, role.
const PLAYERS: [(u8, &str, RoleEnum); 14] = [
    (2, "Denise Meli", RoleEnum::MiddleBlocker),
    (3, "Carlotta Cambi", RoleEnum::Setter),
    (5, "Ilaria Spirito", RoleEnum::Libero),
    (7, "Eleonora Fersino", RoleEnum::Libero),
    (8, "Alessia Orro", RoleEnum::Setter),
    (11, "Anna Danesi", RoleEnum::MiddleBlocker),
    (14, "Linda Nwakalor", RoleEnum::MiddleBlocker),
    (15, "Merit Adigwe", RoleEnum::OppositeHitter),
    (16, "Stella Nervini", RoleEnum::OutsideHitter),
    (17, "Myriam Sylla", RoleEnum::OutsideHitter),
    (18, "Paola Egonu", RoleEnum::OppositeHitter),
    (19, "Sarah Fahr", RoleEnum::MiddleBlocker),
    (22, "Gaia Giovannini", RoleEnum::OutsideHitter),
    (24, "Ekaterina Antropova", RoleEnum::OppositeHitter),
];

/// The example team, named in the language of `labels`, of `year`.
pub fn example_team(labels: &Labels, year: u16) -> TeamInput {
    TeamInput::New {
        id: None,
        name: labels.example_team_name.to_string(),
        year,
        classification: Some(TeamClassificationEnum::TopInternational),
        gender: Some(GenderEnum::Women),
        players: PLAYERS
            .iter()
            .map(|(number, name, role)| PlayerEntry {
                name: name.to_string(),
                role: Some(*role),
                number: *number,
                ..Default::default()
            })
            .collect(),
    }
}

/// Whether the team still has every player of the example team, as the
/// tutorial expects.
pub fn is_example_team(team: &TeamEntry) -> bool {
    PLAYERS.iter().all(|(number, name, role)| {
        team.active_players()
            .iter()
            .any(|p| p.number == *number && p.name == *name && p.role == Some(*role))
    })
}

/// Whether the data are those of a new installation: no settings and no
/// team. An update has the settings (the terms of use accepted) or a team.
pub async fn is_new_installation(store: &dyn Store) -> Result<bool, AppError> {
    Ok(!store
        .list(layout::ROOT)
        .await?
        .iter()
        .any(|entry| match entry {
            StoreEntry::Folder(_) => true,
            StoreEntry::Blob(name) => name == layout::CONFIG,
        }))
}

/// Saves the example team, of this year.
pub async fn add_example_team(
    writer: &(dyn TeamWriter + Send + Sync),
    labels: &Labels,
) -> Result<TeamEntry, AppError> {
    let year = u16::try_from(Local::now().year()).unwrap_or_default();
    writer.save(example_team(labels, year)).await
}

/// The example team, saved when the installation is new; nothing otherwise.
pub async fn add_example_team_if_new(
    store: &dyn Store,
    writer: &(dyn TeamWriter + Send + Sync),
    labels: &Labels,
) -> Result<Option<TeamEntry>, AppError> {
    match is_new_installation(store).await? {
        true => add_example_team(writer, labels).await.map(Some),
        false => Ok(None),
    }
}

/// The match of the tutorial, on `date`: at home, against the opponent of
/// the example.
pub fn tutorial_match(labels: &Labels, date: DateTime<FixedOffset>) -> MatchForm {
    MatchForm {
        opponent: labels.example_opponent.to_string(),
        home: true,
        ..MatchForm::new(date)
    }
}

/// The lineup of the tutorial, by number, positions 1 to 6: the setter in 6
/// (P6), the outside hitter next to her in 1, then the middle blocker, the
/// opposite, the other outside hitter (in 4, she receives) and the other
/// middle blocker, whom the libero replaces.
const TUTORIAL_POSITIONS: [u8; 6] = [17, 19, 18, 16, 11, 8];
const TUTORIAL_SETTER: u8 = 8;
const TUTORIAL_LIBERO: u8 = 5;
const TUTORIAL_FALLBACK_LIBERO: u8 = 7;

/// The start of the set of the tutorial: they serve, we receive in P6. A
/// player missing from `team` leaves her place empty.
pub fn tutorial_lineup(form: &mut SetStartForm, team: &TeamEntry) {
    let id = |number: u8| {
        team.active_players()
            .iter()
            .find(|p| p.number == number)
            .map(|p| p.id)
    };
    TUTORIAL_POSITIONS
        .iter()
        .enumerate()
        .for_each(|(position, number)| form.place(position, id(*number)));
    form.serving_team = Some(TeamSideEnum::Them);
    form.setter = id(TUTORIAL_SETTER);
    form.libero = id(TUTORIAL_LIBERO);
    form.fallback_libero = id(TUTORIAL_FALLBACK_LIBERO);
}
