//! Writes the PDF report (and the data it lays out) of every match found in the
//! data folder to the given output folder, to look at the layout:
//! `cargo run -p scout4all-core --example match_report -- <out dir> [en|it|pt|pl|ja] [data dir]`,
//! the data in `~/.scout4all` unless given: its database, so a data folder of
//! the versions before it must be opened by the app first (it migrates).
// a development tool, not the app: it stops at the first error
#![allow(clippy::expect_used)]
use scout4all_core::db;
use scout4all_core::localization::Language;
use scout4all_core::providers::sqlite::{
    Database, matches::SqliteMatchReader, teams::SqliteTeamReader,
};
use scout4all_core::providers::{match_reader::MatchReader, team_reader::TeamReader};
use scout4all_core::reporting::{model::build_report, pdf::render_report_pdf};
use scout4all_core::shapes::enums::LanguageEnum;
use scout4all_core::storage::layout;
use std::{env, fs, path::PathBuf, str::FromStr};

#[tokio::main]
async fn main() {
    let out: PathBuf = env::args()
        .nth(1)
        .expect("expected the output folder")
        .into();
    fs::create_dir_all(&out).expect("expected the output folder to be created");
    let language = Language::new(
        env::args()
            .nth(2)
            .map(|s| LanguageEnum::from_str(&s).expect("expected en, it, pt, pl or ja"))
            .unwrap_or(LanguageEnum::En),
    );
    let labels = language.labels();
    let base: PathBuf = env::args()
        .nth(3)
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|home| home.join(".scout4all")))
        .expect("expected the data folder");
    let db = Database::new(
        db::open(&base.join(layout::DATABASE)).expect("expected the database of the data folder"),
    );
    let teams = SqliteTeamReader::new(db.clone());
    let matches = SqliteMatchReader::new(db, language);
    for team in teams
        .read_all()
        .await
        .expect("expected the teams")
        .into_iter()
        .flatten()
    {
        for m in matches
            .read_all(&team)
            .await
            .expect("expected the matches")
            .into_iter()
            .flatten()
        {
            let report = build_report(&m, labels).expect("expected the report data");
            let json = serde_json::to_string_pretty(&report).expect("expected the data");
            fs::write(out.join(format!("{}.json", m.id)), json).expect("expected the data");
            let pdf = render_report_pdf(&report).expect("expected the report");
            fs::write(out.join(format!("{}.pdf", m.id)), pdf).expect("expected the report");
            println!("{}", out.join(format!("{}.pdf", m.id)).display());
        }
    }
}
