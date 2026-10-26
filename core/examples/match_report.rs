//! Writes the PDF report (and the data it lays out) of every match found in the
//! data folder to the given output folder, to look at the layout:
//! `cargo run -p scout4all-core --example match_report -- <out dir> [en|it|pt|pl|ja] [data dir]`,
//! the data in `~/.scout4all` unless given.
// a development tool, not the app: it stops at the first error
#![allow(clippy::expect_used)]
use scout4all_core::localization::Language;
use scout4all_core::providers::store::{
    match_reader::StoreMatchReader, set_reader::StoreSetReader, team_reader::StoreTeamReader,
};
use scout4all_core::providers::{match_reader::MatchReader, team_reader::TeamReader};
use scout4all_core::reporting::{model::build_report, pdf::render_report_pdf};
use scout4all_core::shapes::enums::LanguageEnum;
use scout4all_core::storage::{Store, fs::FsStore};
use std::{env, fs, path::PathBuf, str::FromStr, sync::Arc};

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
    let store: Arc<dyn Store> = Arc::new(FsStore::new(&base));
    let teams = StoreTeamReader::new(Arc::clone(&store), language.clone());
    let matches = StoreMatchReader::new(
        Arc::clone(&store),
        Arc::new(StoreSetReader::new(store, language.clone())),
        language,
    );
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
