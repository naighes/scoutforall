//! The team of a side of a DataVolley file among ours: the similar teams,
//! and the players paired by number and surname.

#[cfg(test)]
mod tests {
    use crate::dvw::{
        import::{TeamChoice, TeamIssue, normalized, season_year, similar_teams, team_plan},
        model::{DvwFile, DvwRole, MatchDate, MatchInfo, Player, Side, Team},
    };
    use crate::shapes::{
        country::CountryCode,
        enums::{GenderEnum, RoleEnum, TeamClassificationEnum},
        player::PlayerEntry,
        team::TeamEntry,
    };
    use crate::team_form::TeamFormError;
    use chrono::NaiveDate;
    use uuid::Uuid;

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).expect("a date")
    }

    fn player(number: u8, surname: &str, name: &str, role: Option<DvwRole>) -> Player {
        Player {
            number,
            starts: vec![],
            id: String::new(),
            surname: surname.to_string(),
            name: name.to_string(),
            role,
            libero: false,
            captain: false,
        }
    }

    fn team(name: &str) -> Team {
        Team {
            code: String::new(),
            name: name.to_string(),
            sets_won: None,
            coach: None,
            assistant: None,
        }
    }

    // the home team: Volley Wien, 2021/22
    fn file(season: Option<&str>, match_date: MatchDate, home: Vec<Player>) -> DvwFile {
        DvwFile {
            generator: None,
            last_change: None,
            match_info: MatchInfo {
                date: match_date,
                time: None,
                season: season.map(str::to_string),
                league: None,
                phase: None,
                text_encoding: None,
            },
            home: team("Volley Wien"),
            visiting: team("Ljubljana"),
            sets: vec![],
            home_players: home,
            visiting_players: vec![],
            attack_combinations: vec![],
            winning_symbols: None,
            scout: vec![],
        }
    }

    fn wien_players() -> Vec<Player> {
        let mut libero = player(11, "HASELSTEINER", "Sophie", None);
        libero.libero = true;
        vec![
            player(1, "VASSILAKOPOULOS", "Martha", Some(DvwRole::OutsideHitter)),
            player(2, "MITRENGOVÁ", "Diana", Some(DvwRole::MiddleBlocker)),
            player(7, "MEHIC", "Aida", Some(DvwRole::Setter)),
            player(14, "KIRÁLY-TÓTÁS", "Zsuzsanna", Some(DvwRole::Opposite)),
            libero,
            player(16, "BAJI", "Samia", Some(DvwRole::Libero)),
        ]
    }

    fn wien() -> DvwFile {
        file(
            Some("2021/2022"),
            MatchDate::Known(date(2021, 10, 6)),
            wien_players(),
        )
    }

    fn ours(name: &str, year: u16, players: &[(u8, &str)]) -> TeamEntry {
        TeamEntry {
            name: name.to_string(),
            year,
            players: players
                .iter()
                .map(|(number, name)| PlayerEntry {
                    name: name.to_string(),
                    number: *number,
                    ..PlayerEntry::default()
                })
                .collect(),
            id: Uuid::new_v4(),
            ..TeamEntry::default()
        }
    }

    fn new_team() -> TeamChoice<'static> {
        TeamChoice::New {
            classification: Some(TeamClassificationEnum::Regional),
            gender: Some(GenderEnum::Women),
            country: CountryCode::parse("AT"),
        }
    }

    #[test]
    fn names_compared_lowercase_without_accents_and_punctuation() {
        assert_eq!(
            normalized("  Király-Tótás,  Zsuzsanna "),
            "kiraly totas zsuzsanna"
        );
        assert_eq!(normalized("MITRENGOVÁ"), "mitrengova");
        assert_eq!(normalized("Volley  Wien!"), "volley wien");
        assert_eq!(normalized(""), "");
    }

    /// The first year written in the season, otherwise the season of the
    /// date (from July the year itself), otherwise none.
    #[test]
    fn the_year_of_the_season() {
        let known = MatchDate::Known(date(2022, 2, 3));
        for (season, match_date, year) in [
            (Some("2014/2015"), MatchDate::Missing, Some(2014)),
            (Some("MEVZA 2021/22"), MatchDate::Missing, Some(2021)),
            (Some("2021"), known, Some(2021)),
            (Some("OVV"), known, Some(2021)),
            (None, MatchDate::Known(date(2021, 10, 6)), Some(2021)),
            (None, MatchDate::Known(date(2021, 7, 1)), Some(2021)),
            (None, MatchDate::Known(date(2021, 6, 30)), Some(2020)),
            // day or month first, the same season either way
            (
                None,
                MatchDate::Ambiguous(date(2021, 11, 8), date(2021, 8, 11)),
                Some(2021),
            ),
            // a season apart: it can't be told
            (
                None,
                MatchDate::Ambiguous(date(2022, 1, 9), date(2022, 9, 1)),
                None,
            ),
            (Some("OVV"), MatchDate::Missing, None),
            (None, MatchDate::Missing, None),
        ] {
            let f = file(season, match_date, vec![]);
            assert_eq!(season_year(&f), year, "{season:?} {match_date:?}");
        }
    }

    /// The same season, and the same name or at least half of the players
    /// by number and surname; the same name first, then the most players.
    #[test]
    fn similar_teams_and_why() {
        let same_name = ours("VOLLEY WIEN", 2021, &[]);
        let half = ours(
            "Wien Damen",
            2021,
            &[
                (1, "Martha Vassilakopoulos"),
                (2, "Diana Mitrengova"),
                (7, "Aida Mehic"),
            ],
        );
        let most = ours(
            "Posthalle",
            2021,
            &[
                (1, "Martha Vassilakopoulos"),
                (2, "Diana Mitrengova"),
                (7, "Aida Mehic"),
                (14, "Zsuzsanna Király-Tótás"),
            ],
        );
        let fewer = ours(
            "Other",
            2021,
            &[(1, "Martha Vassilakopoulos"), (2, "Diana Mitrengova")],
        );
        let other_season = ours("Volley Wien", 2020, &[(1, "Martha Vassilakopoulos")]);
        let teams = vec![
            fewer.clone(),
            half.clone(),
            other_season,
            most.clone(),
            same_name.clone(),
        ];
        let similar = similar_teams(&wien(), Side::Home, &teams);
        let found: Vec<(Uuid, bool, usize, usize)> = similar
            .iter()
            .map(|s| (s.team.id, s.same_name, s.same_players, s.players))
            .collect();
        assert_eq!(
            found,
            vec![
                (same_name.id, true, 0, 6),
                (most.id, false, 4, 6),
                (half.id, false, 3, 6),
            ]
        );
    }

    /// A removed player doesn't make the team similar; another number, or
    /// another surname, doesn't count.
    #[test]
    fn similar_by_the_active_players_with_number_and_surname() {
        let mut removed = ours(
            "A",
            2021,
            &[
                (1, "Martha Vassilakopoulos"),
                (2, "Diana Mitrengova"),
                (7, "Aida Mehic"),
            ],
        );
        removed.players[0].deleted = true;
        let other_numbers = ours(
            "B",
            2021,
            &[
                (3, "Martha Vassilakopoulos"),
                (4, "Diana Mitrengova"),
                (5, "Aida Mehic"),
            ],
        );
        let other_surnames = ours(
            "C",
            2021,
            &[(1, "Martha Rossi"), (2, "Diana Bianchi"), (7, "Aida Verdi")],
        );
        let teams = vec![removed, other_numbers, other_surnames];
        assert!(similar_teams(&wien(), Side::Home, &teams).is_empty());
    }

    /// A new team: the name and the season of the file, the gender and the
    /// country chosen; every player added, "Nome Cognome", with the role.
    #[test]
    fn a_new_team_with_the_players_of_the_file() {
        let plan = team_plan(&wien(), Side::Home, new_team()).expect("a plan");
        assert!(plan.new_team);
        assert_eq!(plan.team.name, "Volley Wien");
        assert_eq!(plan.team.year, 2021);
        assert_eq!(plan.team.gender, Some(GenderEnum::Women));
        assert_eq!(plan.team.country, CountryCode::parse("AT"));
        assert_eq!(
            plan.team.classification,
            Some(TeamClassificationEnum::Regional)
        );
        let players: Vec<(u8, &str, Option<RoleEnum>)> = plan
            .team
            .players
            .iter()
            .map(|p| (p.number, p.name.as_str(), p.role))
            .collect();
        assert_eq!(
            players,
            vec![
                (1, "Martha Vassilakopoulos", Some(RoleEnum::OutsideHitter)),
                (2, "Diana Mitrengová", Some(RoleEnum::MiddleBlocker)),
                (7, "Aida Mehic", Some(RoleEnum::Setter)),
                (11, "Sophie Haselsteiner", Some(RoleEnum::Libero)),
                (14, "Zsuzsanna Király-Tótás", Some(RoleEnum::OppositeHitter)),
                (16, "Samia Baji", Some(RoleEnum::Libero)),
            ]
        );
        assert_eq!(plan.new_players.len(), 6);
        let ids: Vec<Uuid> = plan.team.players.iter().map(|p| p.id).collect();
        assert_eq!(plan.roster, ids);
        assert_eq!(plan.numbers.get(&14), Some(&ids[4]));
        assert_eq!(plan.liberos, vec![11, 16]);
    }

    /// A new team needs its level, gender and country, as the form of a
    /// team: the first one missing, in the order of the form, is said.
    #[test]
    fn a_new_team_needs_level_gender_and_country() {
        let level = Some(TeamClassificationEnum::Regional);
        let gender = Some(GenderEnum::Women);
        let country = CountryCode::parse("AT");
        for (classification, gender, country, missing) in [
            (None, None, None, TeamFormError::MissingClassification),
            (None, gender, country, TeamFormError::MissingClassification),
            (level, None, None, TeamFormError::MissingGender),
            (level, None, country, TeamFormError::MissingGender),
            (level, gender, None, TeamFormError::MissingCountry),
        ] {
            let choice = TeamChoice::New {
                classification,
                gender,
                country,
            };
            assert_eq!(
                team_plan(&wien(), Side::Home, choice).map(|p| p.team.id),
                Err(TeamIssue::Incomplete(missing)),
                "{classification:?} {gender:?} {country:?}"
            );
        }
    }

    /// Said in the language of the app, as by the form of a team.
    #[test]
    fn a_new_team_incomplete_is_said_as_by_the_form() {
        use crate::localization::labels;
        use crate::shapes::enums::LanguageEnum;
        let it = labels(LanguageEnum::It);
        for missing in [
            TeamFormError::MissingClassification,
            TeamFormError::MissingGender,
            TeamFormError::MissingCountry,
        ] {
            assert_eq!(
                TeamIssue::Incomplete(missing).describe(it),
                missing.message(it)
            );
        }
    }

    /// No season in the file and no date: a new team has no year.
    #[test]
    fn a_new_team_needs_the_season() {
        let f = file(None, MatchDate::Missing, wien_players());
        assert_eq!(
            team_plan(&f, Side::Home, new_team()).map(|p| p.team.year),
            Err(TeamIssue::NoYear)
        );
    }

    /// The chosen team: its players by number and surname kept as they
    /// are (name and role), the others added; its own players not in the
    /// file left, out of the roster.
    #[test]
    fn the_chosen_team_with_the_new_players() {
        let mut chosen = ours(
            "Wien",
            2021,
            &[
                (1, "Martha Vassilakopoulos"),
                (2, "Diana MITRENGOVA"),
                (9, "Anna Huber"),
            ],
        );
        chosen.players[0].role = Some(RoleEnum::Setter);
        let kept: Vec<Uuid> = chosen.players.iter().map(|p| p.id).collect();
        let plan = team_plan(&wien(), Side::Home, TeamChoice::Existing(&chosen)).expect("a plan");
        assert!(!plan.new_team);
        assert_eq!(plan.team.id, chosen.id);
        assert_eq!(plan.team.players.len(), 7);
        assert_eq!(plan.team.players[0].role, Some(RoleEnum::Setter));
        assert_eq!(plan.team.players[1].name, "Diana MITRENGOVA");
        assert_eq!(plan.numbers.get(&1), Some(&kept[0]));
        assert_eq!(plan.numbers.get(&2), Some(&kept[1]));
        assert_eq!(plan.new_players.len(), 4);
        assert!(!plan.roster.contains(&kept[2]));
        assert_eq!(plan.roster.len(), 6);
        assert_eq!(&plan.roster[..2], &kept[..2]);
    }

    /// A removed player with the number and the surname: the same, still
    /// removed from the team, in the roster of the match.
    #[test]
    fn a_removed_player_is_the_same() {
        let mut chosen = ours("Wien", 2021, &[(7, "Aida Mehic")]);
        chosen.players[0].deleted = true;
        let id = chosen.players[0].id;
        let plan = team_plan(&wien(), Side::Home, TeamChoice::Existing(&chosen)).expect("a plan");
        assert_eq!(plan.numbers.get(&7), Some(&id));
        assert!(plan.roster.contains(&id));
        assert!(plan.team.players[0].deleted);
        assert_eq!(plan.new_players.len(), 5);
    }

    /// The number of a removed player with another surname: a new player.
    #[test]
    fn the_number_of_a_removed_player_for_another_one() {
        let mut chosen = ours("Wien", 2021, &[(7, "Anna Huber")]);
        chosen.players[0].deleted = true;
        let plan = team_plan(&wien(), Side::Home, TeamChoice::Existing(&chosen)).expect("a plan");
        let id = plan.numbers.get(&7).copied();
        assert_ne!(id, Some(chosen.players[0].id));
        assert!(id.is_some_and(|id| plan.new_players.contains(&id)));
    }

    /// The same number with another surname: the import stops, saying who.
    #[test]
    fn the_same_number_with_another_surname_is_a_conflict() {
        let chosen = ours("Wien", 2021, &[(7, "Anna Huber")]);
        assert_eq!(
            team_plan(&wien(), Side::Home, TeamChoice::Existing(&chosen)).map(|p| p.team.id),
            Err(TeamIssue::PlayerConflict {
                number: 7,
                file: "Aida Mehic".to_string(),
                team: "Anna Huber".to_string(),
            })
        );
    }

    /// The same surname with another number: another player (siblings),
    /// never merged.
    #[test]
    fn the_same_surname_with_another_number_is_another_player() {
        let chosen = ours("Wien", 2021, &[(8, "Aida Mehic")]);
        let plan = team_plan(&wien(), Side::Home, TeamChoice::Existing(&chosen)).expect("a plan");
        assert_ne!(plan.numbers.get(&7), Some(&chosen.players[0].id));
        assert_eq!(plan.new_players.len(), 6);
    }

    /// A surname matches the end of our name only as whole words.
    #[test]
    fn the_surname_as_whole_words() {
        let chosen = ours("Wien", 2021, &[(16, "Sara Abaji")]);
        assert!(matches!(
            team_plan(&wien(), Side::Home, TeamChoice::Existing(&chosen)),
            Err(TeamIssue::PlayerConflict { number: 16, .. })
        ));
        let chosen = ours("Wien", 2021, &[(16, "Baji")]);
        let plan = team_plan(&wien(), Side::Home, TeamChoice::Existing(&chosen)).expect("a plan");
        assert_eq!(plan.numbers.get(&16), Some(&chosen.players[0].id));
    }

    /// A part written in lowercase is kept as written, one in capitals (the
    /// surname or the name) written as a name; an empty first name leaves
    /// the surname alone.
    #[test]
    fn names_as_written() {
        let f = file(
            Some("2021"),
            MatchDate::Missing,
            vec![
                player(3, "de Rossi", "Anna", None),
                player(4, "O'BRIEN", "", None),
                player(5, "Bednjanič", "ŠPELA", None),
                player(6, "VOVK", "ANA MARIJA", None),
            ],
        );
        let plan = team_plan(&f, Side::Home, new_team()).expect("a plan");
        let names: Vec<&str> = plan.team.players.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(
            names,
            vec![
                "Anna de Rossi",
                "O'Brien",
                "Špela Bednjanič",
                "Ana Marija Vovk"
            ]
        );
        assert_eq!(plan.team.players[0].role, None);
    }

    /// The issues as the user reads them, in the language chosen; the
    /// detail of the translator kept as it is.
    #[test]
    fn the_issues_described() {
        use crate::dvw::{
            import::MatchIssue,
            model::Skill,
            translate::{Issue, TranslateError},
        };
        use crate::localization::labels;
        use crate::shapes::enums::LanguageEnum;
        let (en, it) = (labels(LanguageEnum::En), labels(LanguageEnum::It));
        let conflict = TeamIssue::PlayerConflict {
            number: 7,
            file: "Aida Mehic".to_string(),
            team: "Anna Huber".to_string(),
        };
        assert_eq!(
            conflict.describe(it),
            "numero 7: Aida Mehic nel file, Anna Huber nella squadra"
        );
        let set = |line, issue| {
            MatchIssue::Set(TranslateError {
                set_number: 2,
                line,
                issue,
            })
        };
        for (issue, text) in [
            (
                set(Some(761), Issue::Untranslatable("our touch after the end")),
                "set 2, line 761: a rally that can't be translated exactly (our touch after the end)",
            ),
            (
                set(None, Issue::NoLibero),
                "set 2: no libero: the matches without one can't be imported",
            ),
            (
                set(
                    Some(12),
                    Issue::Ending {
                        skill: Skill::Attack,
                        evaluation: '!',
                    },
                ),
                "set 2, line 12: A! ends the rally differently from our evaluations",
            ),
            (
                set(
                    Some(80),
                    Issue::Score {
                        file: (3, 1),
                        ours: (2, 2),
                    },
                ),
                "set 2, line 80: the score: 3-1 in the file, 2-2 ours",
            ),
            (
                MatchIssue::SetScore {
                    set_number: 1,
                    file: (25, 2),
                    ours: (25, 0),
                },
                "set 1: 25-2 in the file, 25-0 from its rallies",
            ),
            (
                MatchIssue::Unfinished,
                "the sets of the file don't finish the match",
            ),
        ] {
            assert_eq!(issue.describe(en), text);
        }
    }
}

/// Six substitutions for the matches before the rules of 2026, eight from
/// then on and when the day isn't known.
#[test]
fn the_substitutions_of_the_rules_of_the_time() {
    use crate::dvw::import::default_max_substitutions;
    use chrono::NaiveDate;
    let day = |y, m, d| NaiveDate::from_ymd_opt(y, m, d);
    assert_eq!(default_max_substitutions(day(2015, 1, 25)), 6);
    assert_eq!(default_max_substitutions(day(2025, 12, 31)), 6);
    assert_eq!(default_max_substitutions(day(2026, 1, 1)), 8);
    assert_eq!(default_max_substitutions(None), 8);
}
