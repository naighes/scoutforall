//! The statistics of every match and team of a data folder, written as
//! text files: taken before and after a change, `diff -r` tells whether a
//! number changed. For each team, a folder with each match's report (the
//! model of the PDF, as JSON) and live statistics (with the trend), the
//! analysis of all its matches for each fundamental and of some of them
//! (the last 3, the last 10, October 2026), why a match can't be read, and
//! its matches as listed; and the home. Everything is read from the
//! database. The folder is opened as the apps open it: a folder of before
//! the database is migrated first (its backup zip beside it), so run it on
//! a copy. The matches migrated are named by their id of before
//! (`legacy_id`), in the names of the files and in the text, so the reports
//! compare with those taken before the database (`diff -r`).

use crate::R;
use scout4all_core::{
    db,
    localization::Language,
    providers::{
        match_reader::MatchReader,
        sqlite::{Database, matches::SqliteMatchReader, teams::SqliteTeamReader},
        team_reader::TeamReader,
    },
    reporting::model::build_report,
    shapes::enums::{EventTypeEnum, LanguageEnum},
    storage::layout,
};
use scout4all_mobile::{AnalysisPeriod, DetailFilter, Scout4All, StatsScope};
use std::{fs, path::Path};

/// The fundamentals of the analysis of a team.
const FUNDAMENTALS: [EventTypeEnum; 5] = [
    EventTypeEnum::S,
    EventTypeEnum::P,
    EventTypeEnum::A,
    EventTypeEnum::B,
    EventTypeEnum::D,
];

pub fn write_reports(dir: &Path, out: &Path) -> R<()> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    let language = Language::new(LanguageEnum::En);
    let labels = language.labels();
    // first: it migrates a folder of before the database
    let app = Scout4All::new(dir.to_string_lossy().into_owned(), "en".to_string())
        .map_err(|e| e.to_string())?;
    if let Some(notice) = app.startup_notice() {
        eprintln!("{}: {notice}", dir.display());
    }
    let connection = db::open(&dir.join(layout::DATABASE)).map_err(|e| e.to_string())?;
    let names = Names::read(&connection)?;
    let database = Database::new(connection);
    let teams = SqliteTeamReader::new(database.clone());
    let matches = SqliteMatchReader::new(database, language.clone());
    let all = AnalysisPeriod::Between {
        from: "1900-01-01".to_string(),
        to: "2999-12-31".to_string(),
    };
    let mut written = 0;
    for team in runtime
        .block_on(teams.read_all())
        .map_err(|e| e.to_string())?
        .into_iter()
        .flatten()
    {
        let id = team.id.to_string();
        let folder = out.join(&id);
        fs::create_dir_all(&folder).map_err(|e| format!("{}: {e}", folder.display()))?;
        for listed in runtime
            .block_on(matches.read_all(&team))
            .map_err(|e| e.to_string())?
        {
            match listed {
                Ok(m) => {
                    let name = names.of(&m.id);
                    let report = build_report(&m, labels).map_err(|e| e.to_string())?;
                    let json = serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?;
                    names.write(&folder.join(format!("{name}.report.json")), &json)?;
                    let live = app
                        .match_stats(id.clone(), m.id.clone(), StatsScope::Match)
                        .map_err(|e| e.to_string())?;
                    names.write(
                        &folder.join(format!("{name}.live.txt")),
                        &format!("{live:#?}"),
                    )?;
                    written += 1;
                }
                Err(unreadable) => names.write(
                    &folder.join(format!("{}.unreadable.txt", names.of(&unreadable.id))),
                    &format!("{:?}", unreadable.reason),
                )?,
            }
        }
        let listed = app.matches(id.clone()).map_err(|e| e.to_string())?;
        names.write(&folder.join("matches.txt"), &format!("{listed:#?}"))?;
        for event in FUNDAMENTALS {
            let analysis = app
                .team_analysis(id.clone(), all.clone(), event, DetailFilter::default())
                .map_err(|e| e.to_string())?;
            names.write(
                &folder.join(format!("analysis_{event}.txt")),
                &format!("{analysis:#?}"),
            )?;
        }
        // the periods choosing some matches: the last ones, a month
        for (name, period) in [
            ("last_10", AnalysisPeriod::LastMatches { count: 10 }),
            ("last_3", AnalysisPeriod::LastMatches { count: 3 }),
            (
                "october",
                AnalysisPeriod::Between {
                    from: "2026-10-01".to_string(),
                    to: "2026-10-31".to_string(),
                },
            ),
        ] {
            let analysis = app
                .team_analysis(
                    id.clone(),
                    period,
                    EventTypeEnum::A,
                    DetailFilter::default(),
                )
                .map_err(|e| e.to_string())?;
            names.write(
                &folder.join(format!("analysis_{name}.txt")),
                &format!("{analysis:#?}"),
            )?;
        }
    }
    let home = app.home().map_err(|e| e.to_string())?;
    names.write(&out.join("home.txt"), &format!("{home:#?}"))?;
    eprintln!("{}: the statistics of {written} matches", out.display());
    Ok(())
}

/// The id of before the database of each match migrated (or imported
/// from an archive of then), by its id.
struct Names(Vec<(String, String)>);

impl Names {
    fn read(connection: &rusqlite::Connection) -> R<Self> {
        let mut statement = connection
            .prepare("SELECT id, legacy_id FROM matches WHERE legacy_id IS NOT NULL")
            .map_err(|e| e.to_string())?;
        let rows = statement
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<(String, String)>, _>>()
            .map_err(|e| e.to_string())?;
        Ok(Self(rows))
    }

    /// The name of the match: its id of before, else its id.
    fn of(&self, id: &str) -> String {
        self.0
            .iter()
            .find(|(uuid, _)| uuid == id)
            .map_or_else(|| id.to_string(), |(_, legacy)| legacy.clone())
    }

    /// `text` with the ids of the matches named as before.
    fn write(&self, path: &Path, text: &str) -> R<()> {
        let named = self
            .0
            .iter()
            .filter(|(uuid, _)| text.contains(uuid.as_str()))
            .fold(text.to_string(), |text, (uuid, legacy)| {
                text.replace(uuid.as_str(), legacy)
            });
        fs::write(path, named).map_err(|e| format!("{}: {e}", path.display()))
    }
}
