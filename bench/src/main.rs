//! How long the app takes on large archives: reading the teams and the
//! matches, the statistics of a team, scouting a match. The archives are
//! made of real matches, from a folder of DataVolley files (both teams of
//! each, those without mistakes of scouting), imported once and copied into
//! as many teams and matches as asked: the numbers can be compared from one
//! change to the next. `scripts/bench.sh` downloads the files.
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
//!                                      ~/.scout4all): nothing is written
//! scout4all-bench reports DIR OUT      the statistics of every match and
//!                                      team of DIR, as files in OUT, to
//!                                      compare before and after a change
//! ```

use chrono::{Duration, NaiveDate};
use scout4all_core::{
    scouting::ScoutingStep,
    shapes::{
        country::CountryCode,
        enums::{EventTypeEnum, GenderEnum, TeamClassificationEnum},
    },
};
use scout4all_mobile::{
    AnalysisPeriod, DetailFilter, DvwChoice, DvwSide, Scout4All, StatsScope, TeamListing,
};
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    process::ExitCode,
    sync::Arc,
    time::Instant,
};

mod reports;

type R<T> = Result<T, String>;

/// The match in progress of a generated archive, in the first team.
const LIVE_MATCH: &str = "2026-11-07_live";

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

const USAGE: &str =
    "usage: scout4all-bench DVW | generate DVW DIR [TEAMSxMATCHES] | measure DIR | reports DIR OUT";

/// The two archives and the match in progress, in a temporary folder.
fn default_run(dvw: &Path) -> R<()> {
    let tmp = tempfile::tempdir().map_err(|e| format!("temporary folder: {e}"))?;
    let seeds = seed(dvw, &tmp.path().join("seed"))?;
    let sizes = ["1x300", "10x100"];
    let mut columns = vec![];
    for size in sizes {
        let dir = tmp.path().join(size);
        generate(&seeds, &dir, size)?;
        columns.push(measure_reads(&dir)?);
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
        "{}: {size}, the match in progress is {LIVE_MATCH}",
        dir.display()
    );
    Ok(())
}

/// A match imported once, copied into the archives.
struct Seed {
    team: PathBuf,
    matches: PathBuf,
    sets: usize,
}

/// Imports each DataVolley file of the folder, both teams, each into a team
/// of its own: the files with mistakes of scouting are left out.
fn seed(dvw: &Path, dir: &Path) -> R<Vec<Seed>> {
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
                Ok(imported) => {
                    let team = dir.join(&imported.team_id);
                    let matches = team.join(&imported.match_id);
                    let sets = set_files(&matches, "csv")?.len();
                    seeds.push(Seed {
                        team,
                        matches,
                        sets,
                    });
                }
                Err(_) => left_out += 1,
            }
        }
    }
    eprintln!(
        "{} files of {}: {} matches imported, {left_out} left out",
        files.len(),
        dvw.display(),
        seeds.len()
    );
    match seeds.is_empty() {
        true => Err(format!("{}: no match could be imported", dvw.display())),
        false => Ok(seeds),
    }
}

/// `T` teams of `M` matches each, the seeds in turn; the first team has
/// also a match in progress, its last set cut halfway.
fn generate(seeds: &[Seed], dir: &Path, size: &str) -> R<()> {
    let (teams, matches) = parse_size(size)?;
    fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let last = NaiveDate::from_ymd_opt(LAST_DAY.0, LAST_DAY.1, LAST_DAY.2)
        .ok_or("the last day of the matches")?;
    for t in 0..teams {
        let seed = seeds.get(t % seeds.len()).ok_or("no seed for the team")?;
        let team = dir.join(uuid::Uuid::new_v4().to_string());
        fs::create_dir_all(&team).map_err(|e| format!("{}: {e}", team.display()))?;
        copy(&seed.team.join("team.json"), &team.join("team.json"))?;
        for m in 0..matches {
            let day = last - Duration::days(i64::try_from(m).map_err(|e| e.to_string())?);
            let folder = team.join(format!("{}_bench_{m}", day.format("%Y-%m-%d")));
            copy_match(&seed.matches, &folder, &day)?;
        }
        if t == 0 {
            // the seed with the most sets: the longest match to scout
            let longest = seeds
                .iter()
                .max_by_key(|s| s.sets)
                .ok_or("no seed for the match in progress")?;
            let folder = team.join(LIVE_MATCH);
            copy_match(&longest.matches, &folder, &(last + Duration::days(6)))?;
            cut_last_set(&folder)?;
        }
    }
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

/// The files of the match, on another day.
fn copy_match(from: &Path, to: &Path, day: &NaiveDate) -> R<()> {
    fs::create_dir_all(to).map_err(|e| format!("{}: {e}", to.display()))?;
    for entry in fs::read_dir(from).map_err(|e| format!("{}: {e}", from.display()))? {
        let path = entry.map_err(|e| e.to_string())?.path();
        let name = path.file_name().ok_or("a file without name")?;
        match name.to_str() {
            Some("match.json") => {
                let mut descriptor: Value = serde_json::from_slice(&read(&path)?)
                    .map_err(|e| format!("{}: {e}", path.display()))?;
                descriptor["date"] =
                    Value::String(format!("{}T20:30:00+02:00", day.format("%Y-%m-%d")));
                let json = serde_json::to_vec_pretty(&descriptor).map_err(|e| e.to_string())?;
                write(&to.join(name), &json)?;
            }
            _ => copy(&path, &to.join(name))?,
        }
    }
    Ok(())
}

/// The events of the last set cut about halfway, after an event `undo`
/// takes back to its evaluation: then saving it again is measured.
fn cut_last_set(folder: &Path) -> R<()> {
    let last = set_files(folder, "csv")?
        .into_iter()
        .max()
        .ok_or("the match in progress has no events")?;
    let csv = read(&last)?;
    let rows: Vec<&[u8]> = csv.split_inclusive(|b| *b == b'\n').collect();
    let evaluated = |row: &&[u8]| {
        matches!(
            row.split(|b| *b == b',').nth(1),
            Some(b"A" | b"B" | b"D" | b"P" | b"S")
        )
    };
    let keep = (1..=rows.len() / 2)
        .rev()
        .find(|n| rows.get(n - 1).is_some_and(evaluated))
        .ok_or("no evaluated event in the first half of the last set")?;
    write(&last, &rows.get(..keep).unwrap_or_default().concat())
}

/// The files of the sets of the match with the extension, sorted by name.
fn set_files(folder: &Path, extension: &str) -> R<Vec<PathBuf>> {
    let mut files: Vec<PathBuf> = fs::read_dir(folder)
        .map_err(|e| format!("{}: {e}", folder.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.extension().is_some_and(|e| e == extension)
                && p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with("set_"))
        })
        .collect();
    files.sort();
    Ok(files)
}

/// The reads of the archive, as the apps do them.
fn measure_reads(dir: &Path) -> R<Vec<(String, f64)>> {
    let app = open(dir)?;
    let team = largest_team(&app, dir)?;
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
        time("home()", READS, || app.home().map(|_| ()))?,
        time("matches(team)", READS, || {
            app.matches(team.clone()).map(|_| ())
        })?,
        time("opponents(team)", READS, || {
            app.opponents(team.clone()).map(|_| ())
        })?,
        time("team_analysis(last 10)", READS, || {
            analysis(AnalysisPeriod::LastMatches { count: 10 })
        })?,
        time("team_analysis(all)", READS, || analysis(period.clone()))?,
    ])
}

/// The actions of the scout on the match in progress of a generated
/// archive.
fn measure_scouting(dir: &Path) -> R<Vec<(String, f64)>> {
    let app = open(dir)?;
    let team = largest_team(&app, dir)?;
    let open_match = || app.scouting(team.clone(), LIVE_MATCH.to_string());
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

/// The id of the team with the most match folders.
fn largest_team(app: &Arc<Scout4All>, dir: &Path) -> R<String> {
    app.teams()
        .map_err(|e| e.to_string())?
        .into_iter()
        .filter_map(|t| match t {
            TeamListing::Team { id, .. } => Some(id),
            TeamListing::Unreadable { .. } => None,
        })
        .max_by_key(|id| {
            fs::read_dir(dir.join(id)).map_or(0, |entries| {
                entries
                    .filter_map(Result::ok)
                    .filter(|e| e.path().is_dir())
                    .count()
            })
        })
        .ok_or_else(|| format!("{}: no team", dir.display()))
}

fn open(dir: &Path) -> R<Arc<Scout4All>> {
    fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    Scout4All::new(dir.to_string_lossy().into_owned(), "en".to_string()).map_err(|e| e.to_string())
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

fn write(path: &Path, data: &[u8]) -> R<()> {
    fs::write(path, data).map_err(|e| format!("{}: {e}", path.display()))
}

fn copy(from: &Path, to: &Path) -> R<()> {
    fs::copy(from, to)
        .map(|_| ())
        .map_err(|e| format!("{} → {}: {e}", from.display(), to.display()))
}
