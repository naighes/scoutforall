//! The numbers and roles of the score sheet on the database (TODO/001,
//! step 20, D9): copied from the team when the match is created, then of
//! the match, corrected in its form, not rewritten by the team.

#[cfg(test)]
mod tests {
    use crate::{
        constants::DEFAULT_MAX_SUBSTITUTIONS,
        match_form::{MatchForm, MatchInput},
        providers::{
            match_reader::MatchReader,
            match_writer::MatchWriter,
            sqlite::{Database, rows::match_rows},
            team_reader::TeamReader,
            team_writer::{PlayerInput, TeamInput, TeamWriter},
        },
        shapes::{
            country::CountryCode,
            enums::{GenderEnum, RoleEnum, TeamClassificationEnum},
            r#match::MatchEntry,
            player::PlayerEntry,
            team::TeamEntry,
        },
        tests::Sqlite,
    };
    use chrono::{DateTime, FixedOffset};
    use uuid::Uuid;

    fn date() -> DateTime<FixedOffset> {
        DateTime::parse_from_rfc3339("2026-10-18T18:00:00+02:00").unwrap_or_default()
    }

    /// Eight players numbered from 1, the eighth the libero.
    async fn team(p: &Sqlite) -> TeamEntry {
        p.team_writer
            .save(TeamInput::New {
                id: None,
                name: "Cortona Volley".into(),
                year: 2026,
                classification: Some(TeamClassificationEnum::Regional),
                gender: Some(GenderEnum::Men),
                country: CountryCode::known("IT"),
                players: (1..=8)
                    .map(|n| PlayerEntry {
                        id: Uuid::new_v4(),
                        name: format!("player {n}"),
                        number: n,
                        role: Some(match n {
                            8 => RoleEnum::Libero,
                            1 => RoleEnum::Setter,
                            _ => RoleEnum::OutsideHitter,
                        }),
                        ..PlayerEntry::default()
                    })
                    .collect(),
            })
            .await
            .unwrap()
    }

    fn player(team: &TeamEntry, number: u8) -> Uuid {
        team.players
            .iter()
            .find(|p| p.number == number)
            .map(|p| p.id)
            .unwrap_or_default()
    }

    fn new_form() -> MatchForm {
        MatchForm {
            opponent: "Cascina 2".into(),
            ..MatchForm::new(date())
        }
    }

    /// The form of the saved match as the apps read it.
    fn form_of(p: &Sqlite, team: &TeamEntry, m: &MatchEntry) -> MatchForm {
        let (row, competition) = p.match_reader.details(team, &m.id).unwrap();
        MatchForm::of(m, &row, competition.as_ref())
    }

    /// Saves `form` for `team`: a new match, or `existing` updated.
    async fn save(
        p: &Sqlite,
        team: &TeamEntry,
        existing: Option<&MatchEntry>,
        form: MatchForm,
    ) -> MatchEntry {
        let input = form.validate(team, existing).unwrap();
        match existing {
            Some(m) => {
                p.match_writer.update(m, input).await.unwrap();
                p.match_reader.read_single(team, &m.id).await.unwrap()
            }
            None => p.match_writer.create(team, input).await.unwrap(),
        }
    }

    /// Number and role of `person` on the score sheet of `m`, as saved.
    fn saved(db: &Database, m: &MatchEntry, person: Uuid) -> Option<(u8, Option<RoleEnum>)> {
        let id = Uuid::parse_str(&m.id).unwrap();
        db.read(|tx| match_rows(tx, id))
            .unwrap()
            .unwrap()
            .roster
            .iter()
            .find(|r| r.person_id == person)
            .map(|r| (r.number, r.role))
    }

    fn numbers(m: &MatchEntry, person: Uuid) -> Option<(u8, Option<RoleEnum>)> {
        m.team.find_player(person).map(|p| (p.number, p.role))
    }

    /// Corrected in the form of a new match: saved for the match, the team
    /// as it was.
    #[tokio::test]
    async fn a_number_corrected_is_saved_for_the_match_only() {
        let p = Sqlite::new();
        let team = team(&p).await;
        let two = player(&team, 2);
        let mut form = new_form();
        form.correct(two, "12", Some(RoleEnum::OppositeHitter), &team, None)
            .unwrap();
        let m = save(&p, &team, None, form).await;
        let corrected = Some((12, Some(RoleEnum::OppositeHitter)));
        assert_eq!(saved(&p.db, &m, two), corrected);
        assert_eq!(numbers(&m, two), corrected);
        let read = p.match_reader.read_single(&team, &m.id).await.unwrap();
        assert_eq!(numbers(&read, two), corrected);
        let team = p.team_reader.read_single(&team.id).await.unwrap();
        assert_eq!(
            team.find_player(two).map(|p| (p.number, p.role)),
            Some((2, Some(RoleEnum::OutsideHitter)))
        );
    }

    /// The team changes a number: the matches saved keep theirs, also in
    /// their form, a new match copies the new one.
    #[tokio::test]
    async fn the_team_changing_a_number_leaves_the_matches_saved() {
        let p = Sqlite::new();
        let mut team = team(&p).await;
        let three = player(&team, 3);
        let m = save(&p, &team, None, new_form()).await;
        let mut changed = team.find_player(three).cloned().unwrap_or_default();
        changed.number = 33;
        p.team_writer
            .save_player(PlayerInput::Existing(changed), &mut team)
            .await
            .unwrap();
        let team = p.team_reader.read_single(&team.id).await.unwrap();
        let read = p.match_reader.read_single(&team, &m.id).await.unwrap();
        assert_eq!(
            saved(&p.db, &read, three),
            Some((3, Some(RoleEnum::OutsideHitter)))
        );
        assert_eq!(numbers(&read, three).map(|(n, _)| n), Some(3));
        let shown = form_of(&p, &team, &read)
            .roster_choices(&team, Some(&read))
            .iter()
            .find(|c| c.player.id == three)
            .map(|c| c.player.number);
        assert_eq!(shown, Some(3));
        // saved again as it is, it stays
        let again = save(&p, &team, Some(&read), form_of(&p, &team, &read)).await;
        assert_eq!(saved(&p.db, &again, three).map(|(n, _)| n), Some(3));
        let new = save(&p, &team, None, new_form()).await;
        assert_eq!(saved(&p.db, &new, three).map(|(n, _)| n), Some(33));
    }

    /// Two players of a saved match swap their numbers (unique on the
    /// score sheet), through a free one as the form wants.
    #[tokio::test]
    async fn two_players_swap_their_numbers() {
        let p = Sqlite::new();
        let team = team(&p).await;
        let (one, two) = (player(&team, 1), player(&team, 2));
        let m = save(&p, &team, None, new_form()).await;
        let mut form = form_of(&p, &team, &m);
        for (who, number) in [(one, "99"), (two, "1"), (one, "2")] {
            form.correct(who, number, None, &team, Some(&m)).unwrap();
        }
        let m = save(&p, &team, Some(&m), form).await;
        assert_eq!(saved(&p.db, &m, one), Some((2, None)));
        assert_eq!(saved(&p.db, &m, two), Some((1, None)));
        assert_eq!(numbers(&m, one), Some((2, None)));
    }

    /// Without numbers given (an import overwriting the match), the ones
    /// saved stay; a player added copies the team's.
    #[tokio::test]
    async fn the_numbers_saved_stay_when_not_given() {
        let p = Sqlite::new();
        let team = team(&p).await;
        let (two, eight) = (player(&team, 2), player(&team, 8));
        let mut form = new_form();
        form.toggle(eight, &team, None);
        form.correct(two, "12", None, &team, None).unwrap();
        let m = save(&p, &team, None, form).await;
        assert_eq!(saved(&p.db, &m, eight), None);
        p.match_writer.replace_details(&m).await.unwrap();
        assert_eq!(saved(&p.db, &m, two), Some((12, None)));
        let with_eight = MatchInput::against(
            "Cascina 2",
            date(),
            true,
            DEFAULT_MAX_SUBSTITUTIONS,
            "",
            team.players.iter().map(|p| p.id).collect(),
        );
        p.match_writer.update(&m, with_eight).await.unwrap();
        assert_eq!(saved(&p.db, &m, two), Some((12, None)));
        assert_eq!(saved(&p.db, &m, eight), Some((8, Some(RoleEnum::Libero))));
    }
}
