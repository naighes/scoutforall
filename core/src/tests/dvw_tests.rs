//! The parser of the DataVolley files: the fixtures are files of openvolley
//! (MIT, see `fixtures/dvw/LICENSE.md`), from DataVolley 3 and 4 and
//! Click&Scout, in UTF-8 and in the code pages of Windows.

#[cfg(test)]
mod tests {
    use crate::dvw::{
        DvwError,
        meta::match_date,
        model::{Code, DvwRole, MatchDate, ScoutLine, SetStart, Side, Skill},
        parse,
        text::embedded_utf8,
    };
    use chrono::{NaiveDate, NaiveTime};

    /// DataVolley 3.7.3, UTF-8: a youth final in Slovenia.
    const EXAMPLE: &[u8] = include_bytes!("fixtures/dvw/example_data.dvw");
    /// The same, with the six on court cut from some lines.
    const TRUNCATED: &[u8] = include_bytes!("fixtures/dvw/testing_truncated.dvw");
    /// DataVolley 3.5.7, Windows code page, no encoding declared.
    const PM06: &[u8] = include_bytes!("fixtures/dvw/PM06.dvw");
    /// DataVolley 4, Windows-1252 declared, with the UTF-8 copies.
    const DCUP: &[u8] = include_bytes!("fixtures/dvw/DCup-7.dvw");
    /// Click&Scout: no winning symbols, no attack combinations.
    const CLICK_SCOUT: &[u8] = include_bytes!("fixtures/dvw/example-click-scout.dvw");

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    /// The lines the parser doesn't know, with their number.
    fn unknown(lines: &[ScoutLine]) -> Vec<(usize, String)> {
        lines
            .iter()
            .filter_map(|l| match &l.code {
                Code::Unknown(text) => Some((l.line, text.clone())),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_match_the_teams_and_the_sets() {
        let file = parse(EXAMPLE).unwrap();
        assert_eq!(file.generator.as_deref(), Some("Data Volley Release 3.7.3"));
        assert_eq!(file.match_info.date, MatchDate::Known(date(2015, 1, 25)));
        assert_eq!(file.match_info.time, None);
        assert_eq!(file.match_info.season.as_deref(), Some("2014/2015"));
        assert_eq!(file.match_info.league.as_deref(), Some("Finale mladinke"));
        assert_eq!(file.home.code, "BR4");
        assert_eq!(file.home.name, "Braslovče");
        assert_eq!(file.home.sets_won, Some(3));
        assert_eq!(file.home.coach.as_deref(), Some("JERONČIČ ZORAN"));
        assert_eq!(file.visiting.name, "Nova KBM Branik");
        assert_eq!(file.visiting.assistant, None);
        let finals: Vec<(u8, (u8, u8))> = file
            .sets
            .iter()
            .map(|s| (s.set_number, s.final_score))
            .collect();
        assert_eq!(finals, vec![(1, (25, 16)), (2, (25, 14)), (3, (25, 22))]);
        assert_eq!(file.sets[0].partials, vec![(8, 7), (16, 10), (21, 13)]);
        assert_eq!(file.sets[0].duration, Some(21));
    }

    #[test]
    fn the_players_with_their_roles_and_starting_zones() {
        let file = parse(EXAMPLE).unwrap();
        let libero = &file.home_players[0];
        assert_eq!(
            (
                libero.number,
                libero.id.as_str(),
                libero.surname.as_str(),
                libero.name.as_str()
            ),
            (1, "BR1", "BEDNJANIČ", "ŠPELA")
        );
        assert!(libero.libero);
        assert_eq!(libero.role, Some(DvwRole::Libero));
        assert_eq!(libero.starts[..3], [SetStart::Substitute; 3]);
        let starter = file.home_players.iter().find(|p| p.number == 4).unwrap();
        assert!(starter.captain);
        assert_eq!(starter.starts[..3], [SetStart::Zone(3); 3]);
        assert_eq!(starter.starts[3], SetStart::NotPlayed);
        assert!(file.players(Side::Visiting).len() > 6);
    }

    #[test]
    fn the_tables_of_the_codes() {
        let file = parse(EXAMPLE).unwrap();
        let x1 = file
            .attack_combinations
            .iter()
            .find(|c| c.code == "X1")
            .unwrap();
        assert_eq!(
            (x1.start_zone, x1.side, x1.tempo, x1.target),
            (Some(3), Some('R'), Some('Q'), Some('C'))
        );
        let symbols = file.winning_symbols.unwrap();
        assert_eq!(symbols.losing(Skill::Serve), ['=']);
        assert_eq!(symbols.winning(Skill::Serve), ['#']);
        assert_eq!(symbols.losing(Skill::Attack), ['=', '/']);
        assert_eq!(symbols.winning(Skill::Dig), [] as [char; 0]);
    }

    #[test]
    fn a_touch_with_all_its_fields() {
        let file = parse(EXAMPLE).unwrap();
        let serve = file
            .scout
            .iter()
            .find(|l| matches!(l.code, Code::Skill(_, _)))
            .unwrap();
        let Code::Skill(side, skill) = &serve.code else {
            unreachable!()
        };
        assert_eq!(*side, Side::Home);
        assert_eq!(
            (
                skill.player,
                skill.skill,
                skill.skill_type,
                skill.evaluation
            ),
            (Some(6), Skill::Serve, 'M', '#')
        );
        assert_eq!(skill.combination, None);
        assert_eq!(
            (skill.start_zone, skill.end_zone, skill.end_subzone),
            (Some(1), Some(8), Some('C'))
        );
        assert_eq!(serve.point_phase, Some('p'));
        assert_eq!(serve.time.as_deref(), Some("14.31.13"));
        assert_eq!(serve.set_number, Some(1));
        assert_eq!(serve.setter_zones, (Some(3), Some(3)));
        assert_eq!(serve.home_court, Some([6, 15, 4, 12, 10, 9]));
        assert_eq!(serve.visiting_court, Some([11, 15, 10, 13, 3, 16]));
        // an attack: the combination, the target, the subtype
        let attack = file
            .scout
            .iter()
            .find_map(|l| match &l.code {
                Code::Skill(Side::Visiting, s) if s.skill == Skill::Attack => Some(s),
                _ => None,
            })
            .unwrap();
        assert_eq!(attack.combination.as_deref(), Some("VV"));
        assert_eq!((attack.player, attack.evaluation), (Some(13), '+'));
        assert_eq!((attack.start_zone, attack.end_zone), (Some(7), Some(6)));
        assert_eq!(attack.skill_subtype, Some('H'));
        // a set: the setter call and the target
        let set = file
            .scout
            .iter()
            .find_map(|l| match &l.code {
                Code::Skill(_, s) if s.skill == Skill::Set => Some(s),
                _ => None,
            })
            .unwrap();
        assert_eq!(
            (set.combination.as_deref(), set.target),
            (Some("KE"), Some('F'))
        );
    }

    #[test]
    fn the_lines_that_are_not_touches() {
        let file = parse(EXAMPLE).unwrap();
        let first: Vec<&Code> = file.scout.iter().take(4).map(|l| &l.code).collect();
        assert_eq!(
            first,
            [
                &Code::Setter {
                    side: Side::Home,
                    player: 4,
                    lineup: true
                },
                &Code::SetterZone(Side::Home, 3),
                &Code::Setter {
                    side: Side::Visiting,
                    player: 10,
                    lineup: true
                },
                &Code::SetterZone(Side::Visiting, 3),
            ]
        );
        assert!(
            file.scout
                .iter()
                .any(|l| l.code == Code::Point(Side::Home, 1, 0))
        );
        assert!(
            file.scout
                .iter()
                .any(|l| l.code == Code::Green(Side::Home, '#'))
        );
        assert!(
            file.scout
                .iter()
                .any(|l| l.code == Code::Timeout(Side::Visiting))
        );
        let substitution = file
            .scout
            .iter()
            .find(|l| {
                l.code
                    == Code::Substitution {
                        side: Side::Home,
                        out: 15,
                        enters: 8,
                    }
            })
            .unwrap();
        assert_eq!(substitution.line, 351);
        let slip = String::from_utf8(EXAMPLE.to_vec())
            .unwrap()
            .replace("*c15:08;", "*c15.08;");
        assert!(parse(slip.as_bytes()).unwrap().scout.contains(substitution));
        let ends: Vec<u8> = file
            .scout
            .iter()
            .filter_map(|l| match l.code {
                Code::EndOfSet(n) => Some(n),
                _ => None,
            })
            .collect();
        assert_eq!(ends, vec![1, 2, 3]);
        assert_eq!(unknown(&file.scout), vec![]);
        // the score of the last point is the final one of the file
        let last_point = file
            .scout
            .iter()
            .rev()
            .find_map(|l| match l.code {
                Code::Point(_, home, visiting) => Some((home, visiting)),
                _ => None,
            })
            .unwrap();
        assert_eq!(last_point, (25, 22));
    }

    #[test]
    fn the_page_appended_by_a_site_is_not_scouting() {
        let mut served = EXAMPLE.to_vec();
        served.extend_from_slice(b"<script language=\"JavaScript\">\r\n  setTimeout(f, 10);\r\n</script>\r\n<TABLE class=\"content\">\r\n");
        assert_eq!(parse(&served).unwrap(), parse(EXAMPLE).unwrap());
    }

    #[test]
    fn a_truncated_line_has_no_six_on_court() {
        let file = parse(TRUNCATED).unwrap();
        let cut = file.scout.iter().find(|l| l.line == 249).unwrap();
        assert_eq!(
            cut.code,
            Code::Substitution {
                side: Side::Visiting,
                out: 3,
                enters: 8
            }
        );
        assert_eq!((cut.home_court, cut.visiting_court), (None, None));
        assert_eq!(file.scout.len(), parse(EXAMPLE).unwrap().scout.len());
    }

    #[test]
    fn a_windows_code_page_without_its_declaration() {
        let file = parse(PM06).unwrap();
        assert_eq!(file.match_info.date, MatchDate::Known(date(2012, 12, 28)));
        assert_eq!(
            file.match_info.time,
            Some(NaiveTime::from_hms_opt(20, 0, 0).unwrap())
        );
        assert_eq!(
            file.match_info.league.as_deref(),
            Some("1. DOL Radenska Classic - Pokal moški")
        );
        assert!(file.visiting_players.iter().any(|p| p.surname == "Šmitran"));
        assert!(file.visiting_players.iter().any(|p| p.name == "Aljaž"));
        // in 1250 and 1252 alike: from one to the other only some letters
        // change (č, ć, đ...), not in this file
        assert_eq!(
            file.winning_symbols.unwrap().losing(Skill::Reception),
            ['=', '/']
        );
    }

    #[test]
    fn the_utf8_copies_of_datavolley_4_and_the_day_from_the_serial() {
        let file = parse(DCUP).unwrap();
        assert_eq!(
            file.generator.as_deref(),
            Some("Data Volley Release 4.2020.04")
        );
        assert_eq!(file.match_info.text_encoding.as_deref(), Some("1252"));
        // 08/11/2020 is either: the serial 44143 says November
        assert_eq!(file.match_info.date, MatchDate::Known(date(2020, 11, 8)));
        // not a season, as written
        assert_eq!(file.match_info.season.as_deref(), Some("OVV"));
        assert_eq!(file.home.name, "Hartberg w Cup");
        assert_eq!(file.home.coach.as_deref(), Some("BEINSEN Birgit"));
        let first = &file.home_players[0];
        assert_eq!(
            (first.surname.as_str(), first.name.as_str()),
            ("RYBAR", "Julia Maria")
        );
        assert_eq!(file.sets.len(), 4);
        // a set with `~` for the number of the player: left to the import
        assert_eq!(unknown(&file.scout), vec![(1099, "*~3EH+".to_string())]);
    }

    #[test]
    fn click_and_scout_without_the_tables() {
        let file = parse(CLICK_SCOUT).unwrap();
        assert_eq!(file.generator.as_deref(), Some("Click&Scout 1.54"));
        assert_eq!(file.match_info.date, MatchDate::Known(date(2020, 2, 4)));
        assert_eq!(file.winning_symbols, None);
        assert!(file.attack_combinations.is_empty());
        assert_eq!(file.visiting.code, "ABC");
        assert_eq!(unknown(&file.scout), vec![]);
    }

    #[test]
    fn the_day_first_or_month_first() {
        let serial = Some(date(2020, 11, 8));
        assert_eq!(
            match_date("08/11/2020", serial),
            Some(MatchDate::Known(date(2020, 11, 8)))
        );
        assert_eq!(
            match_date("08/11/2020", None),
            Some(MatchDate::Ambiguous(date(2020, 11, 8), date(2020, 8, 11)))
        );
        // a placeholder serial, as written by other programs, doesn't decide
        assert_eq!(
            match_date("08/11/2020", Some(date(1933, 10, 18))),
            Some(MatchDate::Ambiguous(date(2020, 11, 8), date(2020, 8, 11)))
        );
        assert_eq!(
            match_date("21/04/2018", None),
            Some(MatchDate::Known(date(2018, 4, 21)))
        );
        assert_eq!(
            match_date("12/28/2012", None),
            Some(MatchDate::Known(date(2012, 12, 28)))
        );
        assert_eq!(
            match_date("05/05/2012", None),
            Some(MatchDate::Known(date(2012, 5, 5)))
        );
        assert_eq!(
            match_date("15-01-2020", None),
            Some(MatchDate::Known(date(2020, 1, 15)))
        );
        assert_eq!(
            match_date("2020/01/15", None),
            Some(MatchDate::Known(date(2020, 1, 15)))
        );
        assert_eq!(match_date("", None), Some(MatchDate::Missing));
        assert_eq!(match_date("yesterday", None), None);
        assert_eq!(match_date("31/31/2020", None), None);
    }

    #[test]
    fn the_utf8_copies() {
        assert_eq!(
            embedded_utf8("\u{f}2537A616B6DE17279").as_deref(),
            Some("Szakmáry")
        );
        assert_eq!(embedded_utf8("\u{f}4004A0065").as_deref(), Some("Je"));
        assert_eq!(embedded_utf8("\u{f}2").as_deref(), Some(""));
        assert_eq!(embedded_utf8("Szakmáry"), None);
        assert_eq!(embedded_utf8("\u{f}3414"), None);
        assert_eq!(embedded_utf8("\u{f}2ZZ"), None);
    }

    #[test]
    fn not_a_datavolley_file() {
        assert_eq!(parse(b"PK\x03\x04 a zip"), Err(DvwError::NotDvw));
        assert_eq!(parse(b""), Err(DvwError::NotDvw));
        let without_scout: Vec<u8> = EXAMPLE
            .split(|b| *b == b'\n')
            .take_while(|l| !l.starts_with(b"[3SCOUT]"))
            .flat_map(|l| l.iter().copied().chain(*b"\n"))
            .collect();
        assert_eq!(
            parse(&without_scout),
            Err(DvwError::MissingSection("[3SCOUT]"))
        );
        let bad_score = String::from_utf8(EXAMPLE.to_vec()).unwrap().replace(
            "True;8 -7;16-10;21-13;25-16;21;",
            "True;8 -7;16-10;21-13;25-x;21;",
        );
        assert!(matches!(
            parse(bad_score.as_bytes()),
            Err(DvwError::Invalid { reason, .. }) if reason.contains("25-x")
        ));
    }

    /// What the parser understands of a folder of files, e.g. those of
    /// MEVZA (no license: never in the repository):
    /// `DVW_DIR=... cargo test -p scout4all-core dvw_probe -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn dvw_probe() {
        let dir = std::env::var("DVW_DIR").expect("DVW_DIR");
        let mut paths: Vec<std::path::PathBuf> = walk(std::path::Path::new(&dir));
        paths.sort();
        let mut unknown_codes: std::collections::BTreeMap<String, usize> = Default::default();
        let mut notes: std::collections::BTreeMap<String, usize> = Default::default();
        let mut failed = 0;
        for path in &paths {
            let bytes = std::fs::read(path).unwrap();
            let utf8 = std::str::from_utf8(&bytes).is_ok();
            let name = path.file_name().unwrap().to_string_lossy();
            match parse(&bytes) {
                Err(e) => {
                    failed += 1;
                    println!("{name}: ERROR {e}");
                }
                Ok(file) => {
                    let non_ascii: Vec<String> = file
                        .home_players
                        .iter()
                        .chain(&file.visiting_players)
                        .map(|p| format!("{} {}", p.surname, p.name))
                        .chain([file.home.name.clone(), file.visiting.name.clone()])
                        .filter(|n| !n.is_ascii())
                        .collect();
                    for line in &file.scout {
                        match &line.code {
                            Code::Unknown(text) => {
                                *unknown_codes.entry(text.clone()).or_default() += 1
                            }
                            Code::Note(text) => *notes.entry(text.clone()).or_default() += 1,
                            _ => {}
                        }
                    }
                    println!(
                        "{name}: {} | enc {:?} utf8 {utf8} | {:?} | season {:?} | {} - {} | sets {} | lines {} | {}",
                        file.generator.unwrap_or_default(),
                        file.match_info.text_encoding,
                        file.match_info.date,
                        file.match_info.season,
                        file.home.name,
                        file.visiting.name,
                        file.sets.len(),
                        file.scout.len(),
                        non_ascii.join(", "),
                    );
                }
            }
        }
        println!("\n{} files, {failed} failed", paths.len());
        println!("unknown codes: {unknown_codes:?}");
        println!("notes: {notes:?}");
    }

    fn walk(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
        std::fs::read_dir(dir)
            .unwrap()
            .flatten()
            .flat_map(|entry| match entry.path() {
                path if path.is_dir() => walk(&path),
                path if path
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("dvw")) =>
                {
                    vec![path]
                }
                _ => vec![],
            })
            .collect()
    }

    /// The teams of a folder of files as new teams: the season, the names
    /// of the players, the conflicts and the similar teams between files:
    /// `DVW_DIR=... cargo test -p scout4all-core dvw_import_probe -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn dvw_import_probe() {
        use crate::dvw::import::{TeamChoice, season_year, similar_teams, team_plan};
        let dir = std::env::var("DVW_DIR").expect("DVW_DIR");
        let mut paths = walk(std::path::Path::new(&dir));
        paths.sort();
        let mut teams = vec![];
        for path in &paths {
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            let file = parse(&std::fs::read(path).unwrap()).unwrap();
            for side in [Side::Home, Side::Visiting] {
                let similar: Vec<String> = similar_teams(&file, side, &teams)
                    .iter()
                    .map(|s| {
                        format!(
                            "{} ({}, {}/{})",
                            s.team.name, s.same_name, s.same_players, s.players
                        )
                    })
                    .collect();
                let choice = TeamChoice::New {
                    gender: None,
                    country: None,
                };
                match team_plan(&file, side, choice) {
                    Ok(plan) => {
                        println!(
                            "{name}\t{side:?}\t{:?}\t{}\t{}\tliberos {:?}\tsimilar {similar:?}\t{:?}",
                            season_year(&file),
                            plan.team.name,
                            plan.team.players.len(),
                            plan.liberos,
                            plan.team
                                .players
                                .iter()
                                .map(|p| p.name.as_str())
                                .collect::<Vec<_>>()
                        );
                        teams.push(plan.team);
                    }
                    Err(e) => println!("{name}\t{side:?}\tFAIL {e}"),
                }
            }
        }
    }

    /// How the sets of a folder of files translate, for both teams:
    /// `DVW_DIR=... cargo test -p scout4all-core dvw_translate_probe -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn dvw_translate_probe() {
        use crate::dvw::{
            model::{DvwRole, MatchDate},
            translate::{Issue, Us, translate_set},
        };
        use std::collections::{BTreeMap, HashMap};
        let dir = std::env::var("DVW_DIR").expect("DVW_DIR");
        let max_substitutions: u8 = std::env::var("DVW_MAX_SUBSTITUTIONS")
            .ok()
            .and_then(|m| m.parse().ok())
            .unwrap_or(6);
        let uuid =
            regex::Regex::new(r"[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}")
                .unwrap();
        let mut paths = walk(std::path::Path::new(&dir));
        paths.sort();
        let (mut sets_ok, mut sets_failed, mut sides_ok, mut sides) = (0, 0, 0, 0);
        let mut issues: BTreeMap<String, (usize, String)> = BTreeMap::new();
        let mut mappings: BTreeMap<&str, usize> = BTreeMap::new();
        let mut events = 0;
        for path in &paths {
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            let file = parse(&std::fs::read(path).unwrap()).unwrap();
            let date = match file.match_info.date {
                MatchDate::Known(d) | MatchDate::Ambiguous(d, _) => d,
                MatchDate::Missing => date(2000, 1, 1),
            };
            for side in [Side::Home, Side::Visiting] {
                sides += 1;
                let players: HashMap<u8, uuid::Uuid> = file
                    .players(side)
                    .iter()
                    .map(|p| (p.number, uuid::Uuid::new_v4()))
                    .collect();
                let liberos: Vec<u8> = file
                    .players(side)
                    .iter()
                    .filter(|p| p.libero || p.role == Some(DvwRole::Libero))
                    .map(|p| p.number)
                    .collect();
                let us = Us {
                    side,
                    players: &players,
                    liberos: &liberos,
                };
                let mut all_ok = true;
                for set in &file.sets {
                    match translate_set(&file, &us, set.set_number, date, max_substitutions) {
                        Ok((entry, used)) => {
                            sets_ok += 1;
                            events += entry.events.len();
                            for (k, v) in used {
                                *mappings.entry(k).or_default() += v;
                            }
                        }
                        Err(e) => {
                            sets_failed += 1;
                            all_ok = false;
                            let key = match &e.issue {
                                Issue::Event(text) => {
                                    format!("Event({})", uuid.replace_all(text, "<id>"))
                                }
                                Issue::Score { .. } => "Score".to_string(),
                                Issue::UnknownPlayer(_) => "UnknownPlayer".to_string(),
                                other => format!("{other:?}"),
                            };
                            let example = format!(
                                "{name} {side:?} set {} line {:?}: {:?}",
                                e.set_number, e.line, e.issue
                            );
                            if std::env::var("DVW_ALL").is_ok() {
                                println!(
                                    "FAIL\t{name}\t{side:?}\t{}\t{:?}\t{key}",
                                    e.set_number, e.line
                                );
                            }
                            let entry = issues.entry(key).or_insert((0, example));
                            entry.0 += 1;
                        }
                    }
                }
                if all_ok {
                    sides_ok += 1;
                }
            }
        }
        println!(
            "{} files, max substitutions {max_substitutions}",
            paths.len()
        );
        println!("sets: {sets_ok} translated ({events} events), {sets_failed} failed");
        println!("matches (one team): {sides_ok} of {sides} translated whole");
        println!("\nissues (count, first example):");
        let mut sorted: Vec<_> = issues.into_iter().collect();
        sorted.sort_by_key(|entry| std::cmp::Reverse(entry.1.0));
        for (key, (count, example)) in sorted {
            println!("{count:5}  {key}\n       {example}");
        }
        println!("\nmappings: {mappings:#?}");
    }
}
