//! The checks of the match form, shared by every user interface.

#[cfg(test)]
mod tests {
    use crate::{
        match_form::{MatchForm, MatchFormError, MatchInput},
        shapes::{
            enums::{EventTypeEnum, TeamSideEnum},
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
            notes: String::new(),
            roster: None,
        }
    }

    /// A team of `n` players, numbered from 1, each with the same id in
    /// every team.
    fn team_of(n: u8) -> TeamEntry {
        TeamEntry {
            players: (1..=n)
                .map(|number| PlayerEntry {
                    id: Uuid::from_u128(number.into()),
                    number,
                    name: format!("player {number}"),
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
                opponent: "Arezzo".into(),
                date: date(),
                home: true,
                max_substitutions: 6,
                notes: String::new(),
                roster: team().players.iter().map(|p| p.id).collect(),
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
                home: false,
                max_substitutions: String::new(),
                notes: String::new(),
                roster: Some(vec![]),
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
            MatchForm::of(&m),
            MatchForm {
                opponent: "Arezzo".into(),
                date: Some(date()),
                home: false,
                max_substitutions: "6".into(),
                notes: String::new(),
                roster: None,
            }
        );
        assert!(MatchForm::of(&m).validate(&team(), Some(&m)).is_ok());
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
        assert_eq!(MatchForm::of(&m).notes, "windy");
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
        let mut f = MatchForm::of(&m);
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
        f.toggle(ids(&team, &[1])[0], &team, Some(&m));
        f.toggle(ids(&team, &[9])[0], &team, Some(&m));
        assert_eq!(
            f.clone().validate(&team, Some(&m)),
            Err(MatchFormError::RosterWithoutPlayed)
        );
        f.toggle(ids(&team, &[1])[0], &team, Some(&m));
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
        let f = MatchForm::of(&m);
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
}
