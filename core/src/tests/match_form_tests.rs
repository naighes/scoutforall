//! The checks of the match form, shared by every user interface.

#[cfg(test)]
mod tests {
    use crate::{
        domain::{Home, Match, ScoutingMode, TeamRef},
        match_form::{MatchForm, MatchFormError, MatchInput, RosterNumber},
        shapes::{
            enums::{EventTypeEnum, RoleEnum, TeamSideEnum},
            r#match::MatchEntry,
            player::PlayerEntry,
            set::SetEntry,
            snapshot::EventEntry,
            team::TeamEntry,
        },
        tests::EN,
    };
    use chrono::{DateTime, FixedOffset, Utc};
    use uuid::Uuid;

    fn date() -> DateTime<FixedOffset> {
        DateTime::parse_from_rfc3339("2026-10-24T18:00:00+02:00").unwrap_or_default()
    }

    fn form(max_substitutions: &str) -> MatchForm {
        MatchForm {
            opponent: " Arezzo ".into(),
            date: Some(date()),
            home: true,
            max_substitutions: max_substitutions.into(),
            ..MatchForm::default()
        }
    }

    /// The row of `m` against its opponent's name, in no competition.
    fn row_of(m: &MatchEntry) -> Match {
        Match {
            id: Uuid::nil(),
            team_a: m.team.id,
            team_b: TeamRef::Name(m.opponent.clone()),
            home: Home::from_a(m.home),
            scouting: ScoutingMode::A,
            start: m.date,
            duration_min: None,
            friendly: false,
            competition_id: None,
            stage: None,
            round: None,
            max_substitutions: m.max_substitutions,
            notes: m.notes.clone(),
            source: m.source,
            video_clock: m.video_clock,
            legacy: None,
            deleted: false,
        }
    }

    /// The form of `m` against its opponent's name, in no competition.
    fn of(m: &MatchEntry) -> MatchForm {
        MatchForm::of(m, &row_of(m), None)
    }

    /// A team of `n` players, numbered from 1, each with the same id in
    /// every team; the seventh is the libero.
    fn team_of(n: u8) -> TeamEntry {
        TeamEntry {
            players: (1..=n)
                .map(|number| PlayerEntry {
                    id: Uuid::from_u128(number.into()),
                    number,
                    name: format!("player {number}"),
                    role: (number == 7).then_some(RoleEnum::Libero),
                    ..PlayerEntry::default()
                })
                .collect(),
            ..TeamEntry::default()
        }
    }

    fn team() -> TeamEntry {
        team_of(7)
    }

    fn substitution() -> EventEntry {
        EventEntry {
            timestamp: Utc::now(),
            event_type: EventTypeEnum::R,
            player: Some(Uuid::new_v4()),
            eval: None,
            target_player: Some(Uuid::new_v4()),
            technique: None,
            flags: Default::default(),
        }
    }

    fn set(set_number: u8, substitutions: usize) -> SetEntry {
        SetEntry {
            set_number,
            serving_team: TeamSideEnum::Us,
            initial_positions: [Uuid::nil(); 6],
            libero: Uuid::nil(),
            fallback_libero: None,
            setter: Uuid::nil(),
            lineup_rules: None,
            events: (0..substitutions).map(|_| substitution()).collect(),
        }
    }

    /// A match with 2 substitutions in the first set and 4 in the second.
    fn played() -> MatchEntry {
        MatchEntry {
            source: Default::default(),
            video_clock: false,
            opponent: "Arezzo".into(),
            date: date(),
            id: "2026-10-24_arezzo".into(),
            team: TeamEntry::default(),
            home: false,
            max_substitutions: 6,
            sets: vec![set(1, 2), set(2, 4)],
            notes: String::new(),
            roster: vec![],
        }
    }

    fn error(form: MatchForm) -> MatchFormError {
        match form.validate(&team(), None) {
            Err(e) => e,
            Ok(_) => unreachable!("the form should not be valid"),
        }
    }

    #[test]
    fn a_valid_form_has_the_opponent_trimmed() {
        assert_eq!(
            form("6").validate(&team(), None),
            Ok(MatchInput {
                // the numbers and roles of the team, copied (D9)
                numbers: team()
                    .players
                    .iter()
                    .map(|p| RosterNumber {
                        player: p.id,
                        number: p.number,
                        role: p.role,
                    })
                    .collect(),
                ..MatchInput::against(
                    "Arezzo",
                    date(),
                    true,
                    6,
                    "",
                    team().players.iter().map(|p| p.id).collect(),
                )
            })
        );
    }

    #[test]
    fn a_new_form_starts_on_the_date_with_the_default_substitutions() {
        let new = MatchForm::new(date());
        assert_eq!(new.date, Some(date()));
        assert_eq!(new.max_substitutions, "8");
        assert!(new.opponent.is_empty());
        assert!(!new.home);
    }

    #[test]
    fn the_substitutions_are_from_1_to_99() {
        for max in ["", "0", "00", "100", "-1", "6a", "６"] {
            assert_eq!(
                error(form(max)),
                MatchFormError::InvalidMaxSubstitutions,
                "{max:?}"
            );
        }
        for max in ["1", "99", " 6 ", "08"] {
            assert!(form(max).validate(&team(), None).is_ok(), "{max:?}");
        }
    }

    #[test]
    fn editing_keeps_at_least_the_substitutions_of_a_set() {
        let m = played();
        assert_eq!(
            form("3").validate(&team(), Some(&m)),
            Err(MatchFormError::MaxSubstitutionsBelowUsed)
        );
        assert!(form("4").validate(&team(), Some(&m)).is_ok());
        // a new match has no substitutions yet
        assert!(form("1").validate(&team(), None).is_ok());
    }

    #[test]
    fn the_errors_are_in_the_order_of_the_fields() {
        assert_eq!(
            error(MatchForm {
                opponent: "  ".into(),
                date: None,
                roster: Some(vec![]),
                ..MatchForm::default()
            }),
            MatchFormError::EmptyOpponent
        );
        assert_eq!(
            error(MatchForm {
                date: None,
                ..form("")
            }),
            MatchFormError::InvalidDate
        );
    }

    #[test]
    fn the_form_of_a_match_validates_back_to_it() {
        let m = played();
        assert_eq!(
            of(&m),
            MatchForm {
                opponent: "Arezzo".into(),
                date: Some(date()),
                home: false,
                max_substitutions: "6".into(),
                ..MatchForm::default()
            }
        );
        assert!(of(&m).validate(&team(), Some(&m)).is_ok());
    }

    #[test]
    fn the_errors_are_in_the_domain_labels() {
        assert_eq!(
            MatchFormError::EmptyOpponent.message(EN),
            "opponent cannot be empty"
        );
        assert_eq!(MatchFormError::InvalidDate.message(EN), "invalid date");
    }

    #[test]
    fn the_notes_are_optional_and_trimmed() {
        let input = MatchForm {
            notes: "  windy, gym cold \n".into(),
            ..form("6")
        }
        .validate(&team(), None)
        .unwrap();
        assert_eq!(input.notes, "windy, gym cold");
        let m = MatchEntry {
            notes: "windy".into(),
            ..played()
        };
        assert_eq!(of(&m).notes, "windy");
    }

    /// A match without notes is written as before: an older version reads
    /// it alike; one of an older version reads without them.
    #[test]
    fn the_notes_are_written_only_when_given() {
        let old = r#"{"opponent":"Arezzo","date":"2025-09-15T00:00:00Z","home":true}"#;
        let m: MatchEntry = serde_json::from_str(old).unwrap();
        assert_eq!(m.notes, "");
        let written = serde_json::to_value(&m).unwrap();
        assert!(written.get("notes").is_none());
        let noted = MatchEntry {
            notes: "windy".into(),
            ..m
        };
        assert_eq!(serde_json::to_value(&noted).unwrap()["notes"], "windy");
    }

    fn ids(team: &TeamEntry, numbers: &[u8]) -> Vec<Uuid> {
        numbers
            .iter()
            .filter_map(|n| team.players.iter().find(|p| p.number == *n))
            .map(|p| p.id)
            .collect()
    }

    /// A match of `team` with a set started by players 1 to 7, the libero
    /// 7, and its roster.
    fn started(team: &TeamEntry, roster: Vec<Uuid>) -> MatchEntry {
        let on_court = ids(team, &[1, 2, 3, 4, 5, 6]);
        MatchEntry {
            team: team.with_roster(&roster),
            roster,
            sets: vec![SetEntry {
                initial_positions: [
                    on_court[0],
                    on_court[1],
                    on_court[2],
                    on_court[3],
                    on_court[4],
                    on_court[5],
                ],
                libero: ids(team, &[7])[0],
                setter: on_court[0],
                events: vec![],
                ..set(1, 0)
            }],
            ..played()
        }
    }

    /// A new match has the active players of the team on the score sheet,
    /// by number, the removed ones not.
    #[test]
    fn a_new_match_has_the_active_players() {
        let mut team = team_of(9);
        team.players.reverse();
        team.players[0].deleted = true;
        let input = form("6").validate(&team, None).unwrap();
        assert_eq!(input.roster, ids(&team, &[1, 2, 3, 4, 5, 6, 7, 8]));
        let choices = form("6").roster_choices(&team, None);
        assert_eq!(
            choices
                .iter()
                .map(|c| (c.player.number, c.chosen))
                .collect::<Vec<_>>(),
            (1..=8).map(|n| (n, true)).collect::<Vec<_>>()
        );
    }

    /// From 7 to 14 players: six and the libero, as many as the score sheet
    /// takes.
    #[test]
    fn the_roster_is_from_7_to_14_players() {
        assert_eq!(
            form("6").validate(&team_of(15), None),
            Err(MatchFormError::RosterTooLarge)
        );
        assert_eq!(
            form("6").validate(&team_of(6), None),
            Err(MatchFormError::RosterTooSmall)
        );
        let team = team_of(15);
        let mut f = form("6");
        f.toggle(ids(&team, &[15])[0], &team, None);
        assert_eq!(f.clone().validate(&team, None).unwrap().roster.len(), 14);
        for n in 1..=8 {
            f.toggle(ids(&team, &[n])[0], &team, None);
        }
        assert_eq!(f.validate(&team, None), Err(MatchFormError::RosterTooSmall));
    }

    /// Over 12 players the score sheet needs a libero: up to 12 it doesn't,
    /// and a libero off the roster doesn't count.
    #[test]
    fn over_12_players_the_roster_needs_a_libero() {
        let mut team = team_of(14);
        assert_eq!(form("6").validate(&team, None).unwrap().roster.len(), 14);
        let libero = ids(&team, &[7])[0];
        let mut f = form("6");
        f.toggle(libero, &team, None);
        assert_eq!(
            f.clone().validate(&team, None),
            Err(MatchFormError::RosterWithoutLibero)
        );
        f.toggle(ids(&team, &[14])[0], &team, None);
        assert_eq!(f.validate(&team, None).unwrap().roster.len(), 12);
        team.players[6].role = Some(RoleEnum::Setter);
        assert_eq!(
            form("6").validate(&team, None),
            Err(MatchFormError::RosterWithoutLibero)
        );
        assert!(MatchFormError::RosterWithoutLibero.is_roster());
    }

    /// A match saved before v0.11 without a roster keeps the whole team,
    /// with or without a libero.
    #[test]
    fn a_match_without_roster_needs_no_libero() {
        let mut team = team_of(14);
        team.players[6].role = None;
        let m = MatchEntry {
            roster: vec![],
            ..played()
        };
        let input = of(&m).validate(&team, Some(&m)).unwrap();
        assert!(input.roster.is_empty());
    }

    /// Toggled twice, a player is back as it was; someone not in the team
    /// changes nothing.
    #[test]
    fn toggling_puts_a_player_on_and_off_the_score_sheet() {
        let team = team_of(9);
        let mut f = form("6");
        let eight = ids(&team, &[8])[0];
        f.toggle(eight, &team, None);
        assert!(!f.roster.clone().unwrap().contains(&eight));
        f.toggle(eight, &team, None);
        assert!(f.roster.clone().unwrap().contains(&eight));
        let before = f.roster.clone();
        f.toggle(Uuid::new_v4(), &team, None);
        assert_eq!(f.roster, before);
    }

    /// Who played stays on the score sheet, also when removed from the
    /// team since; who didn't can leave it.
    #[test]
    fn who_played_stays_on_the_score_sheet() {
        let mut team = team_of(9);
        let m = started(&team, ids(&team, &[1, 2, 3, 4, 5, 6, 7, 8]));
        team.players[0].deleted = true;
        let mut f = of(&m);
        let choices = f.roster_choices(&team, Some(&m));
        assert_eq!(
            choices
                .iter()
                .map(|c| (c.player.number, c.chosen))
                .collect::<Vec<_>>(),
            vec![
                (1, true),
                (2, true),
                (3, true),
                (4, true),
                (5, true),
                (6, true),
                (7, true),
                (8, true),
                (9, false)
            ]
        );
        assert_eq!(
            choices
                .iter()
                .filter(|c| c.played)
                .map(|c| c.player.number)
                .collect::<Vec<_>>(),
            vec![1, 2, 3, 4, 5, 6, 7]
        );
        // toggled, who played stays
        f.toggle(ids(&team, &[1])[0], &team, Some(&m));
        assert_eq!(f, of(&m));
        // a roster without them, as no form makes it, is refused
        let without = MatchForm {
            roster: Some(ids(&team, &[2, 3, 4, 5, 6, 7, 8, 9])),
            ..f.clone()
        };
        assert_eq!(
            without.validate(&team, Some(&m)),
            Err(MatchFormError::RosterWithoutPlayed)
        );
        f.toggle(ids(&team, &[9])[0], &team, Some(&m));
        f.toggle(ids(&team, &[8])[0], &team, Some(&m));
        assert_eq!(
            f.validate(&team, Some(&m)).unwrap().roster,
            ids(&team, &[1, 2, 3, 4, 5, 6, 7, 9])
        );
    }

    /// A match saved before v0.11 has no roster: edited, it keeps none, with
    /// the whole team, however many; chosen, the roster is checked.
    #[test]
    fn a_match_without_roster_keeps_none_until_chosen() {
        let team = team_of(16);
        let m = started(&team, vec![]);
        let f = of(&m);
        assert_eq!(f.roster, None);
        assert!(
            f.clone()
                .validate(&team, Some(&m))
                .unwrap()
                .roster
                .is_empty()
        );
        let mut chosen = f;
        chosen.toggle(ids(&team, &[16])[0], &team, Some(&m));
        assert_eq!(
            chosen.validate(&team, Some(&m)),
            Err(MatchFormError::RosterTooLarge)
        );
    }

    /// The team as a match with a roster sees it: the roster active, also
    /// if removed since, the others removed, all still there.
    #[test]
    fn a_match_sees_its_roster_as_the_active_players() {
        let mut team = team_of(9);
        team.players[0].deleted = true;
        let roster = ids(&team, &[1, 2, 3, 4, 5, 6, 7]);
        let seen = team.with_roster(&roster);
        assert_eq!(
            seen.active_players()
                .iter()
                .map(|p| p.number)
                .collect::<Vec<_>>(),
            vec![1, 2, 3, 4, 5, 6, 7]
        );
        assert_eq!(seen.players.len(), 9);
        // without a roster, the team as it is
        assert_eq!(team.with_roster(&[]).active_players().len(), 8);
    }

    /// The roster is written only when there is one: an older version reads
    /// the file as before, ignoring it, and a file without one reads as a
    /// match with the whole team.
    #[test]
    fn the_roster_is_written_only_when_given() {
        let old = r#"{"opponent":"Arezzo","date":"2025-09-15T00:00:00Z","home":true}"#;
        let m: MatchEntry = serde_json::from_str(old).unwrap();
        assert!(m.roster.is_empty());
        assert!(serde_json::to_value(&m).unwrap().get("roster").is_none());
        let roster = vec![Uuid::new_v4()];
        let declared = MatchEntry {
            roster: roster.clone(),
            ..m
        };
        let written = serde_json::to_string(&declared).unwrap();
        let read: MatchEntry = serde_json::from_str(&written).unwrap();
        assert_eq!(read.roster, roster);
        // as v0.10.1 reads it: its fields, without the roster
        #[derive(serde::Deserialize)]
        #[allow(dead_code)]
        struct MatchOfV0_10_1 {
            opponent: String,
            date: DateTime<FixedOffset>,
            home: bool,
            #[serde(default)]
            max_substitutions: u8,
            #[serde(default)]
            notes: String,
        }
        assert!(serde_json::from_str::<MatchOfV0_10_1>(&written).is_ok());
    }

    fn numbered(team: &TeamEntry, number: u8, role: Option<RoleEnum>) -> RosterNumber {
        RosterNumber {
            player: ids(team, &[number])[0],
            number,
            role,
        }
    }

    /// The number and the role of the match: as corrected, else as in the
    /// team; the score sheet by number.
    #[test]
    fn a_number_is_corrected_for_the_match() {
        let team = team_of(9);
        let mut f = form("6");
        let two = ids(&team, &[2])[0];
        f.correct(two, " 12 ", Some(RoleEnum::OppositeHitter), &team, None)
            .unwrap();
        let choices = f.roster_choices(&team, None);
        assert_eq!(
            choices
                .iter()
                .map(|c| (c.player.number, c.player.role))
                .collect::<Vec<_>>(),
            vec![
                (1, None),
                (3, None),
                (4, None),
                (5, None),
                (6, None),
                (7, Some(RoleEnum::Libero)),
                (8, None),
                (9, None),
                (12, Some(RoleEnum::OppositeHitter))
            ]
        );
        let input = f.validate(&team, None).unwrap();
        assert_eq!(
            input.numbers.last(),
            Some(&RosterNumber {
                player: two,
                number: 12,
                role: Some(RoleEnum::OppositeHitter)
            })
        );
        assert_eq!(input.numbers.first(), Some(&numbered(&team, 1, None)));
        assert_eq!(team.find_player(two).map(|p| p.number), Some(2));
    }

    /// From 0 to 99, not the number of another player on the score sheet:
    /// a wrong one leaves the form as it was.
    #[test]
    fn a_corrected_number_is_valid_and_not_of_another_on_the_sheet() {
        let team = team_of(9);
        let mut f = form("6");
        let two = ids(&team, &[2])[0];
        let nine = ids(&team, &[9])[0];
        for (number, error) in [
            ("100", MatchFormError::InvalidRosterNumber),
            ("x", MatchFormError::InvalidRosterNumber),
            ("", MatchFormError::InvalidRosterNumber),
            ("3", MatchFormError::RosterNumberInUse),
        ] {
            assert_eq!(f.correct(two, number, None, &team, None), Err(error));
        }
        assert_eq!(f, form("6"));
        // keeping its own number is fine
        assert_eq!(f.correct(two, "2", None, &team, None), Ok(()));
        // 9 off the score sheet: its number is free
        f.toggle(nine, &team, None);
        assert_eq!(f.correct(two, "9", None, &team, None), Ok(()));
        // 9 back on: two with the same number
        f.toggle(nine, &team, None);
        assert_eq!(
            f.validate(&team, None),
            Err(MatchFormError::RosterNumberInUse)
        );
    }

    /// Over 12 players, the libero is the role in the match, not in the
    /// team.
    #[test]
    fn the_role_of_the_match_counts_for_the_libero() {
        let team = team_of(13);
        let mut f = form("6");
        f.correct(
            ids(&team, &[7])[0],
            "7",
            Some(RoleEnum::OutsideHitter),
            &team,
            None,
        )
        .unwrap();
        assert_eq!(
            f.clone().validate(&team, None),
            Err(MatchFormError::RosterWithoutLibero)
        );
        f.correct(
            ids(&team, &[8])[0],
            "8",
            Some(RoleEnum::Libero),
            &team,
            None,
        )
        .unwrap();
        assert!(f.validate(&team, None).is_ok());
    }

    /// A saved match shows the numbers of its score sheet, not the ones
    /// the team has now; who isn't on it, the team's.
    #[test]
    fn a_saved_match_shows_its_numbers() {
        let team = team_of(9);
        let mut m = started(&team, ids(&team, &[1, 2, 3, 4, 5, 6, 7, 8]));
        let two = ids(&team, &[2])[0];
        let nine = ids(&team, &[9])[0];
        m.team.players.iter_mut().for_each(|p| {
            match (p.id == two, p.id == nine) {
                (true, _) => p.number = 22,
                // not on the score sheet: as the team when read
                (_, true) => p.number = 99,
                _ => {}
            }
        });
        let f = of(&m);
        let numbers: Vec<(Uuid, u8)> = f
            .roster_choices(&team, Some(&m))
            .iter()
            .map(|c| (c.player.id, c.player.number))
            .collect();
        assert_eq!(numbers.last(), Some(&(two, 22)));
        assert!(numbers.contains(&(nine, 9)));
        let input = f.validate(&team, Some(&m)).unwrap();
        assert!(
            input
                .numbers
                .iter()
                .any(|n| n.player == two && n.number == 22)
        );
    }

    /// Who played can have number and role corrected (the events tell of
    /// the player, not of the number).
    #[test]
    fn who_played_can_have_number_and_role_corrected() {
        let team = team_of(9);
        let m = started(&team, ids(&team, &[1, 2, 3, 4, 5, 6, 7, 8]));
        let one = ids(&team, &[1])[0];
        let mut f = of(&m);
        f.correct(one, "10", Some(RoleEnum::Setter), &team, Some(&m))
            .unwrap();
        let input = f.validate(&team, Some(&m)).unwrap();
        assert!(input.numbers.contains(&RosterNumber {
            player: one,
            number: 10,
            role: Some(RoleEnum::Setter)
        }));
    }

    /// A match saved without a roster gets the one it shows when a number
    /// is corrected.
    #[test]
    fn a_match_without_roster_gets_one_when_a_number_is_corrected() {
        let team = team_of(9);
        let m = started(&team, vec![]);
        let mut f = of(&m);
        f.correct(ids(&team, &[9])[0], "19", None, &team, Some(&m))
            .unwrap();
        assert_eq!(f.roster, Some(ids(&team, &[1, 2, 3, 4, 5, 6, 7, 8, 9])));
        assert_eq!(f.validate(&team, Some(&m)).unwrap().roster.len(), 9);
    }
}
