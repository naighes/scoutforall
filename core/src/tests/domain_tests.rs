//! The types of the database (`domain`): every value to the database and
//! back, the closed sets equal to the lists the schema checks, and the
//! rows turned into the types of today and back, on the data of every
//! older version (the legacy fixtures) and on the cases the files can't
//! have (a number of the match, a neutral court, team B a team).

#[cfg(test)]
mod tests {
    use crate::{
        db,
        domain::{
            CompetitionFormat, Event, Home, LegacyId, Membership, Person, RosterEntry,
            ScoutingMode, Side, Stage, Team, TeamRef,
            adapters::{MatchRows, SetRows, TeamRows},
            sql::{DbText, Sql, technique_from_db},
        },
        legacy::folder::read_folder,
        shapes::{
            country::CountryCode,
            enums::{
                EvalEnum, EventTypeEnum, GenderEnum, LineupRulesEnum, RoleEnum,
                TeamClassificationEnum, TeamSideEnum,
            },
            flags::EventFlags,
            r#match::{MatchEntry, MatchSource},
            snapshot::EventEntry,
            team::TeamEntry,
            technique::{ServeTypeEnum, SetTypeEnum, TechniqueEnum},
        },
        storage::fs::FsStore,
        tests::{EN, LEGACY_VERSIONS, copy_dir, legacy_fixtures},
    };
    use chrono::{DateTime, FixedOffset, NaiveDate, Utc};
    use rusqlite::{
        Connection,
        types::{FromSql, ToSql},
    };
    use std::{collections::BTreeSet, fmt::Debug};
    use tempfile::TempDir;
    use uuid::Uuid;

    // values

    /// The value through SQLite: the type it's stored as and the value
    /// read back.
    fn through_sqlite<T: ToSql + FromSql>(value: &T) -> (String, T) {
        let db = Connection::open_in_memory().unwrap();
        db.query_row("SELECT typeof(?1), ?1", [value], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })
        .unwrap()
    }

    /// The SQL value `literal` read as a `T`.
    fn read<T: FromSql>(literal: &str) -> rusqlite::Result<T> {
        let db = Connection::open_in_memory().unwrap();
        db.query_row(&format!("SELECT {literal}"), [], |row| row.get(0))
    }

    fn texts<T: DbText>() -> BTreeSet<String> {
        T::all().iter().map(|v| v.db_text().to_string()).collect()
    }

    /// Every value of the enumeration to its text and back, through
    /// SQLite, as text; texts all different.
    fn check_texts<T: DbText + ToSql + FromSql + Debug>() {
        assert_eq!(texts::<T>().len(), T::all().len(), "{}", T::WHAT);
        for value in T::all() {
            let (kind, back) = through_sqlite(value);
            assert_eq!(kind, "text", "{value:?}");
            assert_eq!(back, *value);
            assert_eq!(T::from_db_text(value.db_text()), Some(*value));
        }
    }

    #[test]
    fn every_enumeration_goes_to_its_text_and_back() {
        check_texts::<RoleEnum>();
        check_texts::<TeamClassificationEnum>();
        check_texts::<GenderEnum>();
        check_texts::<LineupRulesEnum>();
        check_texts::<MatchSource>();
        check_texts::<Side>();
        check_texts::<ScoutingMode>();
        check_texts::<Stage>();
        check_texts::<CompetitionFormat>();
        check_texts::<EventTypeEnum>();
        check_texts::<EvalEnum>();
    }

    #[test]
    fn the_enumerations_are_snake_case_words_of_the_database() {
        assert_eq!(RoleEnum::OutsideHitter.db_text(), "outside_hitter");
        assert_eq!(RoleEnum::MiddleBlocker.db_text(), "middle_blocker");
        assert_eq!(
            TeamClassificationEnum::TopInternational.db_text(),
            "top_international"
        );
        assert_eq!(Side::A.db_text(), "a");
        assert_eq!(ScoutingMode::Both.db_text(), "both");
        assert_eq!(Stage::FinalRound.db_text(), "final_round");
        assert_eq!(LineupRulesEnum::V2.db_text(), "v2");
        assert_eq!(MatchSource::Scout4all.db_text(), "scout4all");
        // not the names of the files
        assert_eq!(
            serde_json::to_string(&RoleEnum::OutsideHitter).unwrap(),
            "\"OutsideHitter\""
        );
        let all = [
            texts::<RoleEnum>(),
            texts::<TeamClassificationEnum>(),
            texts::<GenderEnum>(),
            texts::<LineupRulesEnum>(),
            texts::<MatchSource>(),
            texts::<Side>(),
            texts::<ScoutingMode>(),
            texts::<Stage>(),
            texts::<CompetitionFormat>(),
        ];
        for text in all.iter().flatten() {
            assert!(
                text.chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'),
                "{text}"
            );
        }
    }

    /// The codes of the events are those of the files: the CSV of the sets
    /// and the upload keep them.
    #[test]
    fn the_codes_of_the_events_are_those_of_the_files() {
        for event_type in EventTypeEnum::ALL {
            assert_eq!(event_type.db_text(), event_type.to_string());
            assert_eq!(
                serde_json::to_string(&event_type).unwrap(),
                format!("\"{}\"", event_type.db_text())
            );
        }
        for eval in EvalEnum::ALL {
            assert_eq!(eval.db_text(), eval.to_string());
            assert_eq!(
                serde_json::to_string(&eval).unwrap(),
                format!("\"{}\"", eval.db_text())
            );
        }
    }

    #[test]
    fn an_unknown_word_or_another_type_is_an_error() {
        let error = read::<RoleEnum>("'goalkeeper'").unwrap_err().to_string();
        assert!(error.contains("role not valid: 'goalkeeper'"), "{error}");
        // the files' names aren't read
        assert!(read::<RoleEnum>("'OutsideHitter'").is_err());
        assert!(read::<RoleEnum>("'outside-hitter'").is_err());
        assert!(read::<Side>("'A'").is_err());
        assert!(read::<EvalEnum>("'?'").is_err());
        assert!(read::<EventTypeEnum>("'s'").is_err());
        assert!(read::<Stage>("1").is_err());
        assert!(read::<Side>("NULL").is_err());
        assert_eq!(read::<Option<Side>>("NULL").unwrap(), None);
        assert_eq!(read::<Option<Side>>("'b'").unwrap(), Some(Side::B));
    }

    /// The `IN (...)` list of the check of `column` in the table.
    fn schema_list(db: &Connection, table: &str, column: &str) -> BTreeSet<String> {
        let sql: String = db
            .query_row(
                "SELECT sql FROM sqlite_schema WHERE type = 'table' AND name = ?1",
                [table],
                |row| row.get(0),
            )
            .unwrap();
        let start = format!("CHECK ({column} IN (");
        let list = sql
            .split_once(&start)
            .and_then(|(_, rest)| rest.split_once(')'))
            .map(|(list, _)| list)
            .unwrap_or_else(|| panic!("no list of {table}.{column}"));
        list.split(',')
            .map(|v| v.trim().trim_matches('\'').to_string())
            .collect()
    }

    /// The closed sets of the schema are exactly the values of the types:
    /// none missing on either side.
    #[test]
    fn the_closed_sets_are_those_the_schema_checks() {
        let db = db::open_in_memory().unwrap();
        let columns = [
            ("teams", "classification", texts::<TeamClassificationEnum>()),
            ("teams", "gender", texts::<GenderEnum>()),
            ("competitions", "gender", texts::<GenderEnum>()),
            ("competitions", "format", texts::<CompetitionFormat>()),
            ("team_members", "role", texts::<RoleEnum>()),
            ("match_roster", "role", texts::<RoleEnum>()),
            ("matches", "home", texts::<Side>()),
            ("matches", "scouting", texts::<ScoutingMode>()),
            ("matches", "stage", texts::<Stage>()),
            ("matches", "source", texts::<MatchSource>()),
            ("sets", "serving", texts::<Side>()),
            ("sets", "lineup_rules", texts::<LineupRulesEnum>()),
            ("set_lineups", "side", texts::<Side>()),
            ("events", "side", texts::<Side>()),
        ];
        for (table, column, values) in columns {
            assert_eq!(schema_list(&db, table, column), values, "{table}.{column}");
        }
    }

    /// The values of a team, a competition and a match, written to the
    /// schema and read back.
    #[test]
    fn the_schema_takes_the_values_and_gives_them_back() {
        let db = db::open_in_memory().unwrap();
        let team = Uuid::new_v4();
        let competition = Uuid::new_v4();
        for (i, classification) in TeamClassificationEnum::ALL.iter().enumerate() {
            let id = match i {
                0 => team,
                _ => Uuid::new_v4(),
            };
            db.execute(
                "INSERT INTO teams (id, name, year, classification, gender, country)
                 VALUES (?1, 'T', 2026, ?2, ?3, ?4)",
                rusqlite::params![
                    Sql(id),
                    classification,
                    GenderEnum::ALL[i % 2],
                    CountryCode::parse("it")
                ],
            )
            .unwrap();
            let back: (TeamClassificationEnum, GenderEnum, CountryCode) = db
                .query_row(
                    "SELECT classification, gender, country FROM teams WHERE id = ?1",
                    [Sql(id)],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .unwrap();
            assert_eq!(
                back,
                (
                    *classification,
                    GenderEnum::ALL[i % 2],
                    CountryCode::parse("IT").unwrap()
                )
            );
        }
        for (i, format) in CompetitionFormat::ALL.iter().enumerate() {
            let id = match i {
                0 => competition,
                _ => Uuid::new_v4(),
            };
            db.execute(
                "INSERT INTO competitions (id, name, format) VALUES (?1, 'C', ?2)",
                rusqlite::params![Sql(id), format],
            )
            .unwrap();
        }
        let start: DateTime<FixedOffset> =
            DateTime::parse_from_rfc3339("2026-11-12T18:30:00.25-05:30").unwrap();
        for (i, (home, stage)) in Home::ALL.iter().cycle().zip(Stage::ALL).enumerate() {
            let id = Uuid::new_v4();
            let team_b_ref = TeamRef::Name(format!("B{i}"));
            let (team_b, name) = team_b_ref.columns();
            db.execute(
                "INSERT INTO matches (id, team_a, team_b, team_b_name, home, start,
                     competition_id, stage, max_substitutions, source)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 6, ?9)",
                rusqlite::params![
                    Sql(id),
                    Sql(team),
                    team_b.map(Sql),
                    name,
                    home,
                    Sql(start),
                    Sql(competition),
                    stage,
                    MatchSource::ALL[i % 2]
                ],
            )
            .unwrap();
            let back: (
                Home,
                Stage,
                Sql<DateTime<FixedOffset>>,
                MatchSource,
                ScoutingMode,
            ) = db
                .query_row(
                    "SELECT home, stage, start, source, scouting FROM matches WHERE id = ?1",
                    [Sql(id)],
                    |row| {
                        Ok((
                            row.get(0)?,
                            row.get(1)?,
                            row.get(2)?,
                            row.get(3)?,
                            row.get(4)?,
                        ))
                    },
                )
                .unwrap();
            assert_eq!(
                back,
                (
                    *home,
                    stage,
                    Sql(start),
                    MatchSource::ALL[i % 2],
                    ScoutingMode::A
                )
            );
            assert_eq!(back.2.0.offset(), start.offset());
        }
        let neutral: i64 = db
            .query_row(
                "SELECT count(*) FROM matches WHERE home IS NULL",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(neutral, 2);
    }

    #[test]
    fn a_neutral_court_is_null() {
        assert_eq!(
            through_sqlite(&Home::Neutral),
            ("null".to_string(), Home::Neutral)
        );
        assert_eq!(through_sqlite(&Home::A), ("text".to_string(), Home::A));
        assert_eq!(through_sqlite(&Home::B), ("text".to_string(), Home::B));
        assert!(read::<Home>("'neutral'").is_err());
        // the files: whether we, A, played at home
        assert_eq!(Home::from_a(true), Home::A);
        assert_eq!(Home::from_a(false), Home::B);
        assert!(Home::A.is_a());
        assert!(!Home::B.is_a());
        assert!(!Home::Neutral.is_a());
    }

    #[test]
    fn the_sides_are_as_a_sees_them() {
        assert_eq!(Side::from_a(TeamSideEnum::Us), Side::A);
        assert_eq!(Side::from_a(TeamSideEnum::Them), Side::B);
        for side in Side::ALL {
            assert_eq!(Side::from_a(side.to_a()), side);
        }
    }

    #[test]
    fn an_id_is_lowercase_text_with_hyphens() {
        let id = Uuid::parse_str("A1B2C3D4-0000-4000-8000-00000000000F").unwrap();
        let (kind, back) = through_sqlite(&Sql(id));
        assert_eq!(kind, "text");
        assert_eq!(back, Sql(id));
        let text: String = read_value(&Sql(id));
        assert_eq!(text, "a1b2c3d4-0000-4000-8000-00000000000f");
        assert_eq!(text.len(), 36);
        // the same id never spelled another way
        assert!(read::<Sql<Uuid>>("'A1B2C3D4-0000-4000-8000-00000000000F'").is_err());
        assert!(read::<Sql<Uuid>>("'a1b2c3d4000040008000000000000000f'").is_err());
        assert!(read::<Sql<Uuid>>("'{a1b2c3d4-0000-4000-8000-00000000000f}'").is_err());
        assert!(read::<Sql<Uuid>>("'not an id'").is_err());
        assert!(read::<Sql<Uuid>>("x'00'").is_err());
    }

    /// The text a value is written as.
    fn read_value<T: ToSql>(value: &T) -> String {
        let db = Connection::open_in_memory().unwrap();
        db.query_row("SELECT ?1", [value], |row| row.get(0))
            .unwrap()
    }

    #[test]
    fn the_time_of_a_match_keeps_its_offset_and_every_digit() {
        for text in [
            "2026-11-12T18:30:00+01:00",
            "2026-07-01T09:00:00-05:30",
            "2026-11-12T18:30:00+00:00",
            "2026-11-12T18:30:00.123+01:00",
            "2026-11-12T18:30:00.123456789+14:00",
        ] {
            let time = DateTime::parse_from_rfc3339(text).unwrap();
            assert_eq!(read_value(&Sql(time)), text);
            let (kind, back) = through_sqlite(&Sql(time));
            assert_eq!(kind, "text");
            // equal times compare equal whatever the offset: it too
            assert_eq!(back.0, time);
            assert_eq!(back.0.offset(), time.offset(), "{text}");
        }
        assert!(read::<Sql<DateTime<FixedOffset>>>("'2026-11-12 18:30'").is_err());
        assert!(read::<Sql<DateTime<FixedOffset>>>("'2026-11-12T18:30:00'").is_err());
    }

    #[test]
    fn the_time_of_an_event_is_utc_with_every_digit() {
        let time: DateTime<Utc> = DateTime::parse_from_rfc3339("2026-11-12T17:30:00.123456Z")
            .unwrap()
            .with_timezone(&Utc);
        assert_eq!(read_value(&Sql(time)), "2026-11-12T17:30:00.123456Z");
        assert_eq!(through_sqlite(&Sql(time)).1, Sql(time));
        let now = Utc::now();
        assert_eq!(through_sqlite(&Sql(now)).1, Sql(now));
        // another offset: the same moment
        assert_eq!(
            read::<Sql<DateTime<Utc>>>("'2026-11-12T18:30:00.123456+01:00'").unwrap(),
            Sql(time)
        );
    }

    #[test]
    fn a_date_of_birth_is_year_month_day() {
        let date = NaiveDate::from_ymd_opt(2008, 2, 29).unwrap();
        assert_eq!(read_value(&Sql(date)), "2008-02-29");
        assert_eq!(through_sqlite(&Sql(date)).1, Sql(date));
        let early = NaiveDate::from_ymd_opt(987, 1, 2).unwrap();
        assert_eq!(read_value(&Sql(early)), "0987-01-02");
        assert_eq!(through_sqlite(&Sql(early)).1, Sql(early));
        assert!(read::<Sql<NaiveDate>>("'2009-02-29'").is_err());
        assert!(read::<Sql<NaiveDate>>("'2008-2-9'").is_err());
        assert!(read::<Sql<NaiveDate>>("'2008-02-29T00:00:00'").is_err());
        // the schema takes it
        let db = db::open_in_memory().unwrap();
        db.execute(
            "INSERT INTO persons (id, name, birth_date) VALUES (?1, 'P', ?2)",
            rusqlite::params![Sql(Uuid::new_v4()), Sql(date)],
        )
        .unwrap();
    }

    #[test]
    fn the_flags_keep_every_bit() {
        for bits in [0, 1, 0b110, 1 << 13, u16::MAX] {
            let flags = EventFlags::from_bits(bits);
            assert_eq!(flags.bits(), bits);
            let (kind, back) = through_sqlite(&flags);
            assert_eq!(kind, "integer");
            assert_eq!(back, flags);
        }
        assert!(read::<EventFlags>("65536").is_err());
        assert!(read::<EventFlags>("-1").is_err());
        assert!(read::<EventFlags>("'1'").is_err());
    }

    #[test]
    fn a_country_is_two_capital_letters() {
        let italy = CountryCode::parse("it").unwrap();
        assert_eq!(read_value(&italy), "IT");
        assert_eq!(through_sqlite(&italy).1, italy);
        // a code this version doesn't know is kept
        let unknown = CountryCode::parse("QQ").unwrap();
        assert_eq!(through_sqlite(&unknown).1, unknown);
        assert!(read::<CountryCode>("'it'").is_err());
        assert!(read::<CountryCode>("'ITA'").is_err());
        assert!(read::<CountryCode>("' IT'").is_err());
    }

    #[test]
    fn the_technique_is_read_with_the_type_of_its_event() {
        for set in SetTypeEnum::ALL {
            let technique = TechniqueEnum::Set(set);
            let text = read_value(&technique);
            assert_eq!(
                technique_from_db(EventTypeEnum::A, Some(&text)).unwrap(),
                Some(technique)
            );
        }
        for serve in ServeTypeEnum::ALL {
            let technique = TechniqueEnum::Serve(serve);
            let text = read_value(&technique);
            assert_eq!(
                technique_from_db(EventTypeEnum::S, Some(&text)).unwrap(),
                Some(technique)
            );
        }
        assert_eq!(technique_from_db(EventTypeEnum::P, None).unwrap(), None);
        let serve = read_value(&TechniqueEnum::Serve(ServeTypeEnum::Float));
        assert!(technique_from_db(EventTypeEnum::P, Some(&serve)).is_err());
        // a set for a serve (`F` is both a float serve and a fast set)
        let sets: Vec<String> = SetTypeEnum::ALL
            .iter()
            .map(|set| read_value(&TechniqueEnum::Set(*set)))
            .filter(|code| technique_from_db(EventTypeEnum::S, Some(code)).is_err())
            .collect();
        assert_eq!(sets.len(), SetTypeEnum::ALL.len() - 2, "{sets:?}");
        assert!(technique_from_db(EventTypeEnum::A, Some("?")).is_err());
    }

    #[test]
    fn team_b_is_one_of_its_two_columns() {
        let id = Uuid::new_v4();
        for team_b in [TeamRef::Team(id), TeamRef::Name("Volley Pisa".to_string())] {
            let (team, name) = team_b.columns();
            assert_eq!(
                TeamRef::from_columns(team, name.map(str::to_string)).unwrap(),
                team_b
            );
        }
        assert!(TeamRef::from_columns(None, None).is_err());
        assert!(TeamRef::from_columns(Some(id), Some("Pisa".to_string())).is_err());
    }

    // adapters

    struct Data {
        teams: Vec<TeamEntry>,
        /// each with its team
        matches: Vec<(TeamEntry, MatchEntry)>,
    }

    /// The teams and the matches of a version, read with the code of today.
    async fn legacy_data(version: &str) -> Data {
        let dir = TempDir::new().unwrap();
        copy_dir(&legacy_fixtures(version).join("data"), dir.path());
        let read = read_folder(&FsStore::new(dir.path()), EN).await.unwrap();
        let mut data = Data {
            teams: vec![],
            matches: vec![],
        };
        for team in read {
            let team = team.unwrap();
            for m in team.matches {
                data.matches.push((team.team.clone(), m.unwrap()));
            }
            data.teams.push(team.team);
        }
        data
    }

    #[tokio::test]
    async fn every_team_of_every_version_goes_to_rows_and_back() {
        let mut players = 0;
        let mut removed = 0;
        for version in LEGACY_VERSIONS {
            for team in legacy_data(version).await.teams {
                let rows = TeamRows::of(&team).unwrap();
                assert_eq!(rows.row.id, team.id);
                assert_eq!(rows.persons.len(), team.players.len());
                assert!(rows.members.iter().all(|m| m.team_id == team.id));
                assert!(rows.persons.iter().all(|p| !p.deleted));
                assert_eq!(rows.entry().unwrap(), team, "{version} {}", team.name);
                assert_eq!(TeamRows::of(&rows.entry().unwrap()).unwrap(), rows);
                players += team.players.len();
                removed += team.players.iter().filter(|p| p.deleted).count();
            }
        }
        // the fixtures have removed players
        assert!(players > 100 && removed > 0, "{players} {removed}");
    }

    /// Every match of every version from the files to the rows and back:
    /// the same match from A's side, its sets replaying the same.
    #[tokio::test]
    async fn every_match_of_every_version_goes_to_rows_and_back() {
        let mut matches = 0;
        let mut without_roster = 0;
        for version in LEGACY_VERSIONS {
            for (team, m) in legacy_data(version).await.matches {
                let id = Uuid::new_v4();
                let legacy = LegacyId {
                    id: m.id.clone(),
                    team: team.id,
                };
                let rows = MatchRows::of(
                    &m,
                    id,
                    TeamRef::Name(m.opponent.clone()),
                    Some(legacy.clone()),
                )
                .unwrap();
                assert_eq!(rows.row.legacy, Some(legacy));
                assert_eq!(rows.row.team_a, team.id);
                assert_eq!(rows.row.home, Home::from_a(m.home));
                assert_eq!(rows.row.scouting, ScoutingMode::A);
                // the roster: the one of the match, or the active team
                let expected: BTreeSet<Uuid> = match m.roster.as_slice() {
                    [] => {
                        without_roster += 1;
                        team.active_players().iter().map(|p| p.id).collect()
                    }
                    ids => ids.iter().copied().collect(),
                };
                let roster: BTreeSet<Uuid> = rows.roster.iter().map(|r| r.person_id).collect();
                assert_eq!(roster, expected, "{version} {}", m.id);
                for r in &rows.roster {
                    let player = team.find_player(r.person_id).unwrap();
                    assert_eq!((r.number, r.role), (player.number, player.role));
                }
                assert!(
                    rows.sets
                        .iter()
                        .flat_map(|s| &s.events)
                        .all(|e| e.side == Side::A)
                );

                let back = rows.entry(&team, None).unwrap();
                assert_eq!(back.id, id.to_string());
                assert_eq!(
                    back.roster.iter().copied().collect::<BTreeSet<_>>(),
                    expected
                );
                assert_eq!(
                    back,
                    MatchEntry {
                        id: id.to_string(),
                        roster: back.roster.clone(),
                        ..m.clone()
                    },
                    "{version} {}",
                    m.id
                );
                assert_eq!(back.replay_issue(), None);
                assert_eq!(back.set_scores().unwrap(), m.set_scores().unwrap());
                matches += 1;
            }
        }
        assert!(
            matches >= 30 && without_roster >= 6,
            "{matches} {without_roster}"
        );
    }

    /// The finished match of v0.18.0 and its team.
    async fn finished_match() -> (TeamEntry, MatchEntry) {
        legacy_data("v0.18.0")
            .await
            .matches
            .into_iter()
            .find(|(_, m)| m.id == "2025-10-04_rivali_pisa")
            .unwrap()
    }

    fn rows_of(m: &MatchEntry) -> MatchRows {
        MatchRows::of(m, Uuid::new_v4(), TeamRef::Name(m.opponent.clone()), None).unwrap()
    }

    /// D9: the team of the match has the numbers and roles of its roster,
    /// not of the team today.
    #[tokio::test]
    async fn the_team_of_a_match_has_the_numbers_of_its_roster() {
        let (team, m) = finished_match().await;
        let mut rows = rows_of(&m);
        let changed = rows.roster[0].person_id;
        rows.roster[0].number = 99;
        rows.roster[0].role = Some(RoleEnum::OppositeHitter);
        let back = rows.entry(&team, None).unwrap();
        let player = back.team.find_player(changed).unwrap();
        assert_eq!(
            (player.number, player.role),
            (99, Some(RoleEnum::OppositeHitter))
        );
        assert!(!player.deleted);
        assert_ne!(team.find_player(changed).unwrap().number, 99);
        // the others as in the team, those off the roster removed
        for p in back.team.players.iter().filter(|p| p.id != changed) {
            let in_team = team.find_player(p.id).unwrap();
            assert_eq!((p.number, p.role), (in_team.number, in_team.role));
            assert_eq!(p.deleted, !m.roster.contains(&p.id));
        }
        // the team itself keeps its numbers
        assert_eq!(
            rows.entry(&team, None).unwrap().team.players.len(),
            team.players.len()
        );
    }

    #[tokio::test]
    async fn on_a_neutral_court_a_isnt_at_home() {
        let (team, m) = finished_match().await;
        let mut rows = rows_of(&m);
        for (home, at_home) in [(Home::A, true), (Home::B, false), (Home::Neutral, false)] {
            rows.row.home = home;
            assert_eq!(rows.entry(&team, None).unwrap().home, at_home);
        }
    }

    #[tokio::test]
    async fn team_b_a_team_gives_its_name() {
        let (team, m) = finished_match().await;
        let b = Team {
            id: Uuid::new_v4(),
            name: "Rivali Pisa".to_string(),
            year: 2025,
            classification: None,
            gender: None,
            country: None,
            deleted: false,
        };
        let rows = MatchRows::of(&m, Uuid::new_v4(), TeamRef::Team(b.id), None).unwrap();
        assert_eq!(rows.row.team_b, TeamRef::Team(b.id));
        assert_eq!(rows.entry(&team, Some(&b)).unwrap().opponent, "Rivali Pisa");
        assert!(rows.entry(&team, None).is_err());
        let other = Team {
            id: Uuid::new_v4(),
            ..b.clone()
        };
        assert!(rows.entry(&team, Some(&other)).is_err());
        // a name doesn't need the team
        let named = rows_of(&m);
        assert_eq!(
            named.entry(&team, Some(&other)).unwrap().opponent,
            m.opponent
        );
    }

    #[tokio::test]
    async fn a_match_is_read_with_its_team_a() {
        let (team, m) = finished_match().await;
        let rows = rows_of(&m);
        let other = TeamEntry {
            id: Uuid::new_v4(),
            ..team.clone()
        };
        assert!(rows.entry(&other, None).is_err());
    }

    #[tokio::test]
    async fn a_player_of_the_roster_not_in_the_team_is_an_error() {
        let (team, m) = finished_match().await;
        let stranger = Uuid::new_v4();
        let mut rows = rows_of(&m);
        rows.roster.push(RosterEntry {
            match_id: rows.row.id,
            team_id: team.id,
            person_id: stranger,
            number: 98,
            role: None,
        });
        let error = rows.entry(&team, None).unwrap_err().to_string();
        assert!(error.contains(&stranger.to_string()), "{error}");
        // the roster of B doesn't matter to A
        let mut rows = rows_of(&m);
        rows.roster.push(RosterEntry {
            match_id: rows.row.id,
            team_id: Uuid::new_v4(),
            person_id: stranger,
            number: 98,
            role: None,
        });
        assert_eq!(
            rows.entry(&team, None).unwrap().roster.len(),
            m.roster.len()
        );
        // and from the files
        let with_stranger = MatchEntry {
            roster: [m.roster.clone(), vec![stranger]].concat(),
            ..m.clone()
        };
        assert!(
            MatchRows::of(
                &with_stranger,
                Uuid::new_v4(),
                TeamRef::Name("B".into()),
                None
            )
            .is_err()
        );
    }

    #[tokio::test]
    async fn a_set_gets_ids_and_places_for_its_events() {
        let (_, m) = finished_match().await;
        let match_id = Uuid::new_v4();
        for set in &m.sets {
            let rows = SetRows::of(set, match_id).unwrap();
            assert_eq!(rows.row.number, set.set_number);
            assert_eq!(rows.row.serving, Side::from_a(set.serving_team));
            assert_eq!(rows.row.lineup_rules, set.lineup_rules);
            assert_eq!(rows.lineups.len(), 1);
            assert_eq!(rows.lineups[0].side, Side::A);
            assert_eq!(rows.events.len(), set.events.len());
            let seqs: Vec<u32> = rows.events.iter().map(|e| e.seq).collect();
            assert_eq!(seqs, (0..set.events.len() as u32).collect::<Vec<_>>());
            let ids: BTreeSet<Uuid> = rows.events.iter().map(|e| e.id).collect();
            assert_eq!(ids.len(), set.events.len());
            assert!(rows.events.iter().all(|e| e.match_id == match_id
                && e.set_number == set.set_number
                && e.side == Side::A
                && !e.deleted));
            assert_eq!(rows.entry().unwrap(), *set);
        }
        // the serve of the other team
        let them = crate::shapes::set::SetEntry {
            serving_team: TeamSideEnum::Them,
            ..m.sets[0].clone()
        };
        let rows = SetRows::of(&them, match_id).unwrap();
        assert_eq!(rows.row.serving, Side::B);
        assert_eq!(rows.entry().unwrap().serving_team, TeamSideEnum::Them);
    }

    /// The events in the order of their places, the undone ones and B's
    /// left out; the sets in their order, the deleted ones left out.
    #[tokio::test]
    async fn the_events_are_as_placed_and_the_undone_ones_are_left_out() {
        let (team, m) = finished_match().await;
        let mut rows = rows_of(&m);
        let set = &mut rows.sets[0];
        let expected: Vec<EventEntry> =
            set.events.iter().skip(1).map(|e| e.entry.clone()).collect();
        set.events[0].deleted = true;
        set.events.reverse();
        let b_event = Event {
            id: Uuid::new_v4(),
            seq: 10_000,
            side: Side::B,
            ..set.events[0].clone()
        };
        set.events.push(b_event);
        assert_eq!(set.entry().unwrap().events, expected);

        rows.sets.reverse();
        rows.sets[0].row.deleted = true;
        let back = rows.entry(&team, None).unwrap();
        let numbers: Vec<u8> = back.sets.iter().map(|s| s.set_number).collect();
        let mut kept: Vec<u8> = m.sets.iter().map(|s| s.set_number).collect();
        kept.sort();
        kept.pop();
        assert_eq!(numbers, kept);
        assert_eq!(back.sets[0].events, expected);
    }

    #[tokio::test]
    async fn a_set_without_the_lineup_of_a_is_an_error() {
        let (team, m) = finished_match().await;
        let mut rows = rows_of(&m);
        rows.sets[1].lineups[0].side = Side::B;
        assert!(rows.sets[1].entry().is_err());
        assert!(rows.entry(&team, None).is_err());
        rows.sets[1].lineups.clear();
        assert!(rows.entry(&team, None).is_err());
    }

    fn sample_team() -> TeamRows {
        let team = Uuid::new_v4();
        let persons: Vec<Person> = (1..=3)
            .map(|n| Person {
                id: Uuid::new_v4(),
                name: format!("player {n}"),
                birth_date: NaiveDate::from_ymd_opt(2008, 2, n),
                height_cm: Some(180),
                notes: format!("notes {n}"),
                deleted: false,
            })
            .collect();
        TeamRows {
            row: Team {
                id: team,
                name: "Team".to_string(),
                year: 2026,
                classification: Some(TeamClassificationEnum::Regional),
                gender: Some(GenderEnum::Women),
                country: CountryCode::parse("IT"),
                deleted: false,
            },
            members: persons
                .iter()
                .enumerate()
                .map(|(i, p)| Membership {
                    team_id: team,
                    person_id: p.id,
                    position: u16::try_from(i).unwrap(),
                    number: 7,
                    role: RoleEnum::ALL.get(i).copied(),
                    deleted: i == 2,
                })
                .collect(),
            persons,
        }
    }

    #[test]
    fn a_player_is_a_person_and_a_membership() {
        let rows = sample_team();
        let team = rows.entry().unwrap();
        assert_eq!(team.players.len(), 3);
        for ((player, person), member) in team.players.iter().zip(&rows.persons).zip(&rows.members)
        {
            assert_eq!((player.id, &player.name), (person.id, &person.name));
            assert_eq!(
                (player.number, player.role, player.deleted),
                (member.number, member.role, member.deleted)
            );
        }
        // the active ones with the details of the person
        for (player, person) in team.players.iter().zip(&rows.persons).take(2) {
            assert_eq!(
                (player.birth_date, player.height_cm, &player.notes),
                (person.birth_date, person.height_cm, &person.notes)
            );
        }
        // the same number twice in the team: allowed in the database
        assert_eq!(team.players[0].number, team.players[1].number);
        let back = TeamRows::of(&team).unwrap();
        assert_eq!((&back.row, &back.members), (&rows.row, &rows.members));
        assert_eq!(back.persons[..2], rows.persons[..2]);
    }

    /// A player removed from the team shows only what the matches show:
    /// the person keeps the rest, for its other teams.
    #[test]
    fn a_removed_member_shows_no_personal_details() {
        let rows = sample_team();
        assert!(rows.members[2].deleted);
        let person = &rows.persons[2];
        assert!(person.birth_date.is_some() && person.height_cm.is_some());
        assert!(!person.notes.is_empty());
        let player = rows.entry().unwrap().players[2].clone();
        assert!(player.deleted);
        assert_eq!(player.name, person.name);
        assert_eq!((player.birth_date, player.height_cm), (None, None));
        assert_eq!(player.notes, "");
    }

    #[test]
    fn a_deleted_person_is_a_removed_player_without_data() {
        let mut rows = sample_team();
        rows.persons[0] = Person {
            name: String::new(),
            birth_date: None,
            height_cm: None,
            notes: String::new(),
            deleted: true,
            ..rows.persons[0].clone()
        };
        let player = rows.entry().unwrap().players[0].clone();
        assert!(player.deleted);
        assert_eq!(player.name, "");
        assert!(!rows.members[0].deleted);
    }

    #[test]
    fn a_member_of_another_team_or_without_the_person_is_an_error() {
        let mut rows = sample_team();
        rows.members[1].team_id = Uuid::new_v4();
        assert!(rows.entry().is_err());
        let mut rows = sample_team();
        rows.persons.remove(1);
        let error = rows.entry().unwrap_err().to_string();
        assert!(
            error.contains(&rows.members[1].person_id.to_string()),
            "{error}"
        );
    }

    /// The players are in the order of the places of their memberships,
    /// whatever the order of the rows; a removed player keeps its place.
    #[test]
    fn the_players_are_in_the_order_of_their_places() {
        let mut rows = sample_team();
        let ids = |rows: &TeamRows| -> Vec<Uuid> {
            rows.entry().unwrap().players.iter().map(|p| p.id).collect()
        };
        let order: Vec<Uuid> = rows.persons.iter().map(|p| p.id).collect();
        rows.members.reverse();
        rows.persons.rotate_left(1);
        assert_eq!(ids(&rows), order);
        // the first player last
        rows.members
            .iter_mut()
            .for_each(|m| m.position = (m.position + 2) % 3);
        assert_eq!(ids(&rows), [order[1], order[2], order[0]]);
        assert!(rows.entry().unwrap().players[1].deleted);
        // read back, the places start from 0 again in that order
        let back = TeamRows::of(&rows.entry().unwrap()).unwrap();
        assert_eq!(
            back.members
                .iter()
                .map(|m| (m.person_id, m.position))
                .collect::<Vec<_>>(),
            vec![(order[1], 0), (order[2], 1), (order[0], 2)]
        );
    }
}
