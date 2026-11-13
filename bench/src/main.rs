//! How long the app takes on large archives: reading the teams and the
//! matches, the statistics of a team, scouting a match. The archives are
//! made of real matches, from a folder of DataVolley files (both teams of
//! each, those without mistakes of scouting), imported once and copied into
//! as many teams and matches as asked: the numbers can be compared from one
//! change to the next. An archive is a data folder with its database, the
//! rows of the imported matches copied with new ids (`copy`).
//! `scripts/bench.sh` downloads the files.
//!
//! ```text
//! scout4all-bench DVW                  the archives 1x300 and 10x100 and a
//!                                      match in progress, in a temporary
//!                                      folder: the table of the times
//! scout4all-bench generate DVW DIR [TxM]
//!                                      an archive of T teams of M matches
//!                                      each (default 1x300) and a match in
//!                                      progress, kept in DIR
//! scout4all-bench measure DIR          the times of the reads on DIR (e.g.
//!                                      a copy of ~/.scout4all): only the
//!                                      cache of the lists is rebuilt
//! scout4all-bench reports DIR OUT      the statistics of every match and
//!                                      team of DIR, as files in OUT, to
//!                                      compare before and after a change
//! ```

use chrono::{Duration, NaiveDate};
use rusqlite::{Connection, ToSql, Transaction};
use scout4all_core::{
    db,
    scouting::ScoutingStep,
    shapes::{
        country::CountryCode,
        enums::{EventTypeEnum, GenderEnum, TeamClassificationEnum},
    },
    storage::layout,
};
use scout4all_mobile::{
    AnalysisPeriod, DetailFilter, DvwChoice, DvwSide, Scout4All, StatsScope, TeamListing,
};
use std::{
    fs,
    path::{Path, PathBuf},
    process::ExitCode,
    sync::Arc,
    time::Instant,
};

mod reports;

type R<T> = Result<T, String>;

/// The day of the most recent match; the others a day before each.
const LAST_DAY: (i32, u32, u32) = (2026, 11, 1);

/// Each read measured this many times, the median kept.
const READS: usize = 3;
/// Each action of the scouting, quicker.
const ACTIONS: usize = 20;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let run = match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        [dvw] => default_run(Path::new(dvw)),
        ["generate", dvw, dir] => generate_kept(Path::new(dvw), Path::new(dir), "1x300"),
        ["generate", dvw, dir, size] => generate_kept(Path::new(dvw), Path::new(dir), size),
        ["measure", dir] => {
            measure_reads(Path::new(dir)).map(|rows| print_table(&["read", dir], &rows))
        }
        ["scouting", dir] => {
            measure_scouting(Path::new(dir)).map(|rows| print_table(&["scouting", "time"], &rows))
        }
        ["reports", dir, out] => reports::write_reports(Path::new(dir), Path::new(out)),
        _ => Err(USAGE.to_string()),
    };
    match run {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

const USAGE: &str = "usage: scout4all-bench DVW | generate DVW DIR [TEAMSxMATCHES] | measure DIR | scouting DIR | reports DIR OUT";

/// The two archives and the match in progress, in a temporary folder.
fn default_run(dvw: &Path) -> R<()> {
    let tmp = tempfile::tempdir().map_err(|e| format!("temporary folder: {e}"))?;
    let seeds = seed(dvw, &tmp.path().join("seed"))?;
    let sizes = ["1x300", "10x100"];
    for size in sizes {
        generate(&seeds, &tmp.path().join(size), size)?;
    }
    let mut columns = vec![];
    for size in sizes {
        columns.push(measure_reads(&tmp.path().join(size))?);
    }
    let rows = merge(&columns);
    print_table(&["read", "1x300", "10x100"], &rows);
    println!();
    let live = tmp.path().join("live");
    generate(&seeds, &live, "1x1")?;
    print_table(&["scouting", "time"], &measure_scouting(&live)?);
    Ok(())
}

fn generate_kept(dvw: &Path, dir: &Path, size: &str) -> R<()> {
    let tmp = tempfile::tempdir().map_err(|e| format!("temporary folder: {e}"))?;
    let seeds = seed(dvw, &tmp.path().join("seed"))?;
    generate(&seeds, dir, size)?;
    println!(
        "{}: {size}, the match in progress in the first team",
        dir.display()
    );
    Ok(())
}

/// The matches imported once, copied into the archives.
struct Seeds {
    /// the database they're in
    database: PathBuf,
    matches: Vec<Seed>,
}

/// A match imported once and its team, A.
struct Seed {
    team: String,
    id: String,
    sets: u32,
}

/// Imports each DataVolley file of the folder, both teams, each into a team
/// of its own: the files with mistakes of scouting are left out.
fn seed(dvw: &Path, dir: &Path) -> R<Seeds> {
    let app = open(dir)?;
    let country = CountryCode::parse("IT").ok_or("the country of the teams")?;
    let mut files: Vec<PathBuf> = fs::read_dir(dvw)
        .map_err(|e| format!("{}: {e}", dvw.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("dvw")))
        .collect();
    files.sort();
    let mut seeds = vec![];
    let mut left_out = 0;
    for file in &files {
        let bytes = read(file)?;
        for side in [DvwSide::Home, DvwSide::Visiting] {
            let choice = DvwChoice {
                side,
                day: "2026-01-01".to_string(),
                max_substitutions: 6,
                team_id: None,
                classification: Some(TeamClassificationEnum::NationalMidLevel),
                gender: Some(GenderEnum::Women),
                country: Some(country),
            };
            match app.import_dvw(bytes.clone(), choice, false) {
                Ok(imported) => seeds.push(Seed {
                    team: imported.team_id,
                    id: imported.match_id,
                    sets: 0,
                }),
                Err(_) => left_out += 1,
            }
        }
    }
    let database = dir.join(layout::DATABASE);
    let connection = db::open(&database).map_err(|e| e.to_string())?;
    for seed in &mut seeds {
        seed.sets = connection
            .query_row(
                "SELECT count(*) FROM sets WHERE match_id = ?1 AND deleted = 0",
                [&seed.id],
                |row| row.get(0),
            )
            .map_err(|e| e.to_string())?;
    }
    eprintln!(
        "{} files of {}: {} matches imported, {left_out} left out",
        files.len(),
        dvw.display(),
        seeds.len()
    );
    match seeds.is_empty() {
        true => Err(format!("{}: no match could be imported", dvw.display())),
        false => Ok(Seeds {
            database,
            matches: seeds,
        }),
    }
}

/// `T` teams of `M` matches each, the seeds in turn from the one with the
/// most sets (the longest match to scout): the first team, its team, has
/// also a match in progress, that match with its last set cut halfway.
fn generate(seeds: &Seeds, dir: &Path, size: &str) -> R<()> {
    let (teams, matches) = parse_size(size)?;
    fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let last = NaiveDate::from_ymd_opt(LAST_DAY.0, LAST_DAY.1, LAST_DAY.2)
        .ok_or("the last day of the matches")?;
    let longest = seeds
        .matches
        .iter()
        .enumerate()
        .max_by_key(|(i, s)| (s.sets, std::cmp::Reverse(*i)))
        .map(|(i, _)| i)
        .ok_or("no seed for the match in progress")?;
    let mut connection = db::open(&dir.join(layout::DATABASE)).map_err(|e| e.to_string())?;
    sql(connection.execute(
        "ATTACH DATABASE ?1 AS seed",
        [seeds.database.to_string_lossy()],
    ))?;
    let tx = sql(connection.transaction())?;
    for table in ["persons", "competitions"] {
        sql(tx.execute_batch(&format!(
            "INSERT OR IGNORE INTO main.{table} SELECT * FROM seed.{table}"
        )))?;
    }
    for t in 0..teams {
        let seed = seeds
            .matches
            .get((longest + t) % seeds.matches.len())
            .ok_or("no seed for the team")?;
        let team = uuid::Uuid::new_v4().to_string();
        copy(&tx, "teams", "id = ?1", &[&seed.team], "id = ?1", &[&team])?;
        copy(
            &tx,
            "team_members",
            "team_id = ?1",
            &[&seed.team],
            "team_id = ?1",
            &[&team],
        )?;
        for m in 0..matches {
            let day = last - Duration::days(i64::try_from(m).map_err(|e| e.to_string())?);
            copy_match(&tx, seed, &team, &day)?;
        }
        if t == 0 {
            let live = copy_match(&tx, seed, &team, &(last + Duration::days(6)))?;
            cut_last_set(&tx, &live)?;
        }
    }
    sql(tx.commit())?;
    sql(connection.execute("DETACH DATABASE seed", []))?;
    Ok(())
}

fn parse_size(size: &str) -> R<(usize, usize)> {
    let parsed = size
        .split_once('x')
        .and_then(|(t, m)| Some((t.parse().ok()?, m.parse().ok()?)));
    match parsed {
        Some((t, m)) if t > 0 => Ok((t, m)),
        _ => Err(format!("{size:?}: TEAMSxMATCHES, e.g. 10x100")),
    }
}

/// A new id of the events copied: a UUID v4, as the app makes them.
const NEW_ID: &str = "lower(hex(randomblob(4)) || '-' || hex(randomblob(2)) || '-4' \
    || substr(hex(randomblob(2)), 2) || '-' || substr('89ab', 1 + abs(random()) % 4, 1) \
    || substr(hex(randomblob(2)), 2) || '-' || hex(randomblob(6)))";

/// The rows of the match `seed` in `team` (a copy of its team), on another
/// day, with new ids: its id.
fn copy_match(tx: &Transaction, seed: &Seed, team: &str, day: &NaiveDate) -> R<String> {
    let id = uuid::Uuid::new_v4().to_string();
    let start = format!("{}T20:30:00+02:00", day.format("%Y-%m-%d"));
    let of_seed: &[&dyn ToSql] = &[&seed.id];
    copy(
        tx,
        "matches",
        "id = ?1",
        of_seed,
        "id = ?1, team_a = ?2, start = ?3",
        &[&id, &team, &start],
    )?;
    copy(
        tx,
        "match_roster",
        "match_id = ?1",
        of_seed,
        "match_id = ?1, team_id = CASE team_id WHEN ?2 THEN ?3 ELSE team_id END",
        &[&id, &seed.team, &team],
    )?;
    for table in ["sets", "set_lineups"] {
        copy(tx, table, "match_id = ?1", of_seed, "match_id = ?1", &[&id])?;
    }
    copy(
        tx,
        "events",
        "match_id = ?1",
        of_seed,
        &format!("match_id = ?1, id = {NEW_ID}"),
        &[&id],
    )?;
    Ok(id)
}

/// Copies the rows of `table` of the seeds chosen by `filter` into the
/// archive, changed by `set`: the columns the copy shares with the seed
/// aren't named, whatever they are.
fn copy(
    tx: &Transaction,
    table: &str,
    filter: &str,
    filter_params: &[&dyn ToSql],
    set: &str,
    set_params: &[&dyn ToSql],
) -> R<()> {
    sql(tx.execute(
        &format!("CREATE TEMP TABLE copied AS SELECT * FROM seed.{table} WHERE {filter}"),
        filter_params,
    ))?;
    sql(tx.execute(&format!("UPDATE copied SET {set}"), set_params))?;
    sql(tx.execute_batch(&format!(
        "INSERT INTO main.{table} SELECT * FROM copied; DROP TABLE copied;"
    )))
}

/// The events of the last set cut about halfway, after an event `undo`
/// takes back to its evaluation: then saving it again is measured.
fn cut_last_set(tx: &Transaction, id: &str) -> R<()> {
    let set: u8 = sql(tx.query_row(
        "SELECT max(number) FROM sets WHERE match_id = ?1 AND deleted = 0",
        [id],
        |row| row.get(0),
    ))?;
    let mut statement = sql(tx.prepare(
        "SELECT seq, event_type FROM events
         WHERE match_id = ?1 AND set_number = ?2 AND deleted = 0 ORDER BY seq",
    ))?;
    let events = sql(statement
        .query_map(rusqlite::params![id, set], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })
        .and_then(|rows| rows.collect::<Result<Vec<_>, _>>()))?;
    let evaluated = |event_type: &str| matches!(event_type, "A" | "B" | "D" | "P" | "S");
    let keep = events
        .get(..events.len() / 2)
        .unwrap_or_default()
        .iter()
        .rev()
        .find(|(_, event_type)| evaluated(event_type))
        .map(|(seq, _)| *seq)
        .ok_or("no evaluated event in the first half of the last set")?;
    sql(tx.execute(
        "DELETE FROM events WHERE match_id = ?1 AND set_number = ?2 AND seq > ?3",
        rusqlite::params![id, set, keep],
    ))
    .map(|_| ())
}

fn sql<T>(result: rusqlite::Result<T>) -> R<T> {
    result.map_err(|e| format!("database: {e}"))
}

/// The reads of the archive, as the apps do them.
fn measure_reads(dir: &Path) -> R<Vec<(String, f64)>> {
    let app = open(dir)?;
    let team = largest_team(&app)?;
    let period = AnalysisPeriod::Between {
        from: "1900-01-01".to_string(),
        to: "2999-12-31".to_string(),
    };
    let analysis = |period: AnalysisPeriod| {
        app.team_analysis(
            team.clone(),
            period,
            EventTypeEnum::A,
            DetailFilter::default(),
        )
        .map(|_| ())
    };
    let count = app.matches(team.clone()).map_err(|e| e.to_string())?.len();
    Ok(vec![
        ("matches of the largest team".to_string(), count as f64),
        time("teams()", READS, || app.teams().map(|_| ()))?,
        // after an update of the app: the cache of the lists rebuilt
        time("home(), no cache", READS, || {
            without_cache(dir)?;
            open(dir)?.home().map_err(|e| e.to_string()).map(|_| ())
        })?,
        // the app just opened: the cache in the database
        time("home(), app opened", READS, || {
            open(dir)?.home().map_err(|e| e.to_string()).map(|_| ())
        })?,
        time("home()", READS, || app.home().map(|_| ()))?,
        time("matches(team), no cache", READS, || {
            without_cache(dir)?;
            open(dir)?
                .matches(team.clone())
                .map_err(|e| e.to_string())
                .map(|_| ())
        })?,
        time("matches(team)", READS, || {
            app.matches(team.clone()).map(|_| ())
        })?,
        time("opponents(team)", READS, || {
            app.opponents(team.clone()).map(|_| ())
        })?,
        time("team_analysis(last 10)", READS, || {
            analysis(AnalysisPeriod::LastMatches { count: 10 })
        })?,
        // a month of the generated matches (they end on 2026-11-01)
        time("team_analysis(October)", READS, || {
            analysis(AnalysisPeriod::Between {
                from: "2026-10-01".to_string(),
                to: "2026-10-31".to_string(),
            })
        })?,
        time("team_analysis(all)", READS, || analysis(period.clone()))?,
    ])
}

/// The actions of the scout on the match in progress of a generated
/// archive.
fn measure_scouting(dir: &Path) -> R<Vec<(String, f64)>> {
    let app = open(dir)?;
    let team = largest_team(&app)?;
    let live = app
        .home()
        .map_err(|e| e.to_string())?
        .in_progress
        .into_iter()
        .find(|m| m.team_id == team)
        .map(|m| m.id)
        .ok_or("no match in progress in the largest team")?;
    let open_match = || app.scouting(team.clone(), live.clone());
    let opened = time("scouting() (open)", ACTIONS, || open_match().map(|_| ()))?;
    let sc = open_match().map_err(|e| e.to_string())?;
    let set = sc.state().set_number;
    let mut rows = vec![
        opened,
        time("state()", ACTIONS, || {
            Ok::<_, String>(sc.state()).map(|_| ())
        })?,
        time("stats(set)", ACTIONS, || {
            sc.stats(StatsScope::Set { number: set }).map(|_| ())
        })?,
        time("stats(match)", ACTIONS, || {
            sc.stats(StatsScope::Match).map(|_| ())
        })?,
        // what the desktop reads after each key
        time("desktop refresh", ACTIONS, || {
            let _ = (sc.state(), sc.available_events(), sc.available_evals());
            sc.stats(StatsScope::Set { number: set }).map(|_| ())
        })?,
    ];
    // undo goes back to the evaluation of the last event, choosing it
    // saves the event again: the same point of the match each time
    let mut undos = vec![];
    let mut saves = vec![];
    for _ in 0..ACTIONS {
        undos.push(elapsed(|| sc.undo())?);
        let state = sc.state();
        let eval = match state.step {
            ScoutingStep::Eval => sc.available_evals().into_iter().next(),
            _ => None,
        }
        .ok_or("undo didn't go back to an evaluation")?;
        saves.push(elapsed(|| sc.choose_eval(eval.value))?);
    }
    rows.push(("undo()".to_string(), median(undos)));
    rows.push(("save an event".to_string(), median(saves)));
    Ok(rows)
}

/// The id of the team with the most matches.
fn largest_team(app: &Arc<Scout4All>) -> R<String> {
    app.teams()
        .map_err(|e| e.to_string())?
        .into_iter()
        .filter_map(|t| match t {
            TeamListing::Team { id, .. } => Some(id),
            TeamListing::Unreadable { .. } => None,
        })
        .max_by_key(|id| app.matches(id.clone()).map_or(0, |m| m.len()))
        .ok_or_else(|| "no team".to_string())
}

fn open(dir: &Path) -> R<Arc<Scout4All>> {
    fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    Scout4All::new(dir.to_string_lossy().into_owned(), "en".to_string()).map_err(|e| e.to_string())
}

/// Empties the cache of the lists (`match_summary`), as after an update
/// of the app: it's rebuilt from the events.
fn without_cache(dir: &Path) -> R<()> {
    let connection: Connection =
        db::open(&dir.join(layout::DATABASE)).map_err(|e| e.to_string())?;
    sql(connection.execute("DELETE FROM match_summary", [])).map(|_| ())
}

/// The median of `times` runs, in milliseconds.
fn time<E: ToString>(
    label: &str,
    times: usize,
    mut f: impl FnMut() -> Result<(), E>,
) -> R<(String, f64)> {
    let runs = (0..times)
        .map(|_| elapsed(&mut f))
        .collect::<R<Vec<f64>>>()?;
    Ok((label.to_string(), median(runs)))
}

fn elapsed<T, E: ToString>(f: impl FnOnce() -> Result<T, E>) -> R<f64> {
    let started = Instant::now();
    f().map_err(|e| e.to_string())?;
    Ok(started.elapsed().as_secs_f64() * 1000.0)
}

fn median(mut runs: Vec<f64>) -> f64 {
    runs.sort_by(f64::total_cmp);
    runs.get(runs.len() / 2).copied().unwrap_or_default()
}

/// The rows of the columns side by side, by the labels of the first.
fn merge(columns: &[Vec<(String, f64)>]) -> Vec<(String, Vec<f64>)> {
    columns
        .first()
        .map(|first| {
            first
                .iter()
                .map(|(label, _)| {
                    let values = columns
                        .iter()
                        .map(|c| {
                            c.iter()
                                .find(|(l, _)| l == label)
                                .map_or(f64::NAN, |(_, v)| *v)
                        })
                        .collect();
                    (label.clone(), values)
                })
                .collect()
        })
        .unwrap_or_default()
}

/// A Markdown table, to paste where the times are kept: milliseconds, the
/// counts as they are.
fn print_table<V: Values>(header: &[&str], rows: &[(String, V)]) {
    println!("| {} |", header.join(" | "));
    println!("|{}", "---|".repeat(header.len()));
    for (label, values) in rows {
        let cells: Vec<String> = values
            .values()
            .iter()
            .map(|v| match label.starts_with("matches of") {
                true => format!("{v:.0}"),
                false => format!("{v:.1} ms"),
            })
            .collect();
        println!("| {label} | {} |", cells.join(" | "));
    }
}

trait Values {
    fn values(&self) -> Vec<f64>;
}

impl Values for f64 {
    fn values(&self) -> Vec<f64> {
        vec![*self]
    }
}

impl Values for Vec<f64> {
    fn values(&self) -> Vec<f64> {
        self.clone()
    }
}

fn read(path: &Path) -> R<Vec<u8>> {
    fs::read(path).map_err(|e| format!("{}: {e}", path.display()))
}
