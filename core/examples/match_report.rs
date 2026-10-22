//! Writes the PDF report (and the data it lays out) of every match found in the
//! data folder to the given output folder, to look at the layout:
//! `cargo run -p scout4all-core --example match_report -- <out dir>`.
// a development tool, not the app: it stops at the first error
#![allow(clippy::expect_used)]
use scout4all_core::providers::fs::{
    match_reader::FileSystemMatchReader, path::get_base_path, set_reader::FileSystemSetReader,
    team_reader::FileSystemTeamReader,
};
use scout4all_core::providers::{match_reader::MatchReader, team_reader::TeamReader};
use scout4all_core::reporting::{model::build_report, pdf::render_report_pdf};
use std::{env, fs, path::PathBuf, sync::Arc};

#[tokio::main]
async fn main() {
    let out: PathBuf = env::args()
        .nth(1)
        .expect("expected the output folder")
        .into();
    fs::create_dir_all(&out).expect("expected the output folder to be created");
    let base = get_base_path().expect("expected the data folder");
    let teams = FileSystemTeamReader::new(&base);
    let matches = FileSystemMatchReader::new(&base, Arc::new(FileSystemSetReader::new(&base)));
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
            let report = build_report(&m).expect("expected the report data");
            let json = serde_json::to_string_pretty(&report).expect("expected the data");
            fs::write(out.join(format!("{}.json", m.id)), json).expect("expected the data");
            let pdf = render_report_pdf(&report).expect("expected the report");
            fs::write(out.join(format!("{}.pdf", m.id)), pdf).expect("expected the report");
            println!("{}", out.join(format!("{}.pdf", m.id)).display());
        }
    }
}
