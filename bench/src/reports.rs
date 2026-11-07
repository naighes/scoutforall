//! The statistics of every match and team of a data folder, written as
//! text files: taken before and after a change, `diff -r` tells whether a
//! number changed. For each team, a folder with each match's report (the
//! model of the PDF, as JSON) and live statistics (with the trend), the
//! analysis of all its matches for each fundamental, why a match can't be
//! read, and its matches as listed; and the home. The lists come from the
//! index of the matches, written in the folder if missing: run twice, with
//! and without it, they must be the same.

use crate::R;
use scout4all_core::{
    localization::Language,
    providers::{
        match_reader::MatchReader,
        store::{
            match_reader::StoreMatchReader, set_reader::StoreSetReader,
            team_reader::StoreTeamReader,
        },
        team_reader::TeamReader,
    },
    reporting::model::build_report,
    shapes::enums::{EventTypeEnum, LanguageEnum},
    storage::{Store, fs::FsStore},
};
use scout4all_mobile::{AnalysisPeriod, DetailFilter, Scout4All, StatsScope};
use std::{fs, path::Path, sync::Arc};

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
    let store: Arc<dyn Store> = Arc::new(FsStore::new(dir));
    let teams = StoreTeamReader::new(store.clone(), language.clone());
    let matches = StoreMatchReader::new(
        store.clone(),
        Arc::new(StoreSetReader::new(store.clone(), language.clone())),
        language.clone(),
    );
    let app = Scout4All::new(dir.to_string_lossy().into_owned(), "en".to_string())
        .map_err(|e| e.to_string())?;
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
                    let report = build_report(&m, labels).map_err(|e| e.to_string())?;
                    let json = serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?;
                    write(&folder.join(format!("{}.report.json", m.id)), &json)?;
                    let live = app
                        .match_stats(id.clone(), m.id.clone(), StatsScope::Match)
                        .map_err(|e| e.to_string())?;
                    write(
                        &folder.join(format!("{}.live.txt", m.id)),
                        &format!("{live:#?}"),
                    )?;
                    written += 1;
                }
                Err(unreadable) => write(
                    &folder.join(format!("{}.unreadable.txt", unreadable.id)),
                    &format!("{:?}", unreadable.reason),
                )?,
            }
        }
        let listed = app.matches(id.clone()).map_err(|e| e.to_string())?;
        write(&folder.join("matches.txt"), &format!("{listed:#?}"))?;
        for event in FUNDAMENTALS {
            let analysis = app
                .team_analysis(id.clone(), all.clone(), event, DetailFilter::default())
                .map_err(|e| e.to_string())?;
            write(
                &folder.join(format!("analysis_{event}.txt")),
                &format!("{analysis:#?}"),
            )?;
        }
    }
    let home = app.home().map_err(|e| e.to_string())?;
    write(&out.join("home.txt"), &format!("{home:#?}"))?;
    eprintln!("{}: the statistics of {written} matches", out.display());
    Ok(())
}

fn write(path: &Path, text: &str) -> R<()> {
    fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))
}
