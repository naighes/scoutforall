use crate::errors::{AppError, IOError};
use crate::reporting::model::{Report, build_report};
use crate::reporting::typst_world::ReportWorld;
use crate::shapes::r#match::MatchEntry;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::{env, fs};
use typst::WorldExt;
use typst::foundations::{Dict, Value};
use typst_layout::PagedDocument;
use typst_pdf::PdfOptions;
use uuid::Uuid;

/// The layout and styling of the report: it lays out the `Report` passed
/// as `sys.inputs.report`.
const TEMPLATE: &str = include_str!("report.typ");

pub fn open_match_pdf(m: &MatchEntry) -> Result<(), AppError> {
    let pdf = render_match_pdf(m)?;
    let mut path: PathBuf = env::temp_dir();
    let uid = Uuid::new_v4().to_string();
    path.push(format!("{}_{}.pdf", m.id, uid));
    fs::write(&path, pdf).map_err(|e| AppError::IO(IOError::from(e)))?;
    open_with_system_viewer(&path)
}

pub fn render_match_pdf(m: &MatchEntry) -> Result<Vec<u8>, AppError> {
    render_report_pdf(&build_report(m)?)
}

pub fn render_report_pdf(report: &Report) -> Result<Vec<u8>, AppError> {
    let world = ReportWorld::new(TEMPLATE, report_inputs(report)?);
    let document = typst::compile::<PagedDocument>(&world)
        .output
        .map_err(|e| typst_error(&world, "error compiling typst", &e))?;
    typst_pdf::pdf(&document, &PdfOptions::default())
        .map_err(|e| typst_error(&world, "error exporting PDF", &e))
}

/// The report as the typst value of `sys.inputs.report`.
fn report_inputs(report: &Report) -> Result<Dict, AppError> {
    let msg = |e: serde_json::Error| {
        AppError::IO(IOError::Msg(format!("error encoding the report: {e}")))
    };
    let json = serde_json::to_value(report).map_err(msg)?;
    let value = Value::deserialize(json).map_err(msg)?;
    Ok(Dict::from_iter([("report".into(), value)]))
}

/// The diagnostics, with the line of the template they refer to.
fn typst_error(
    world: &ReportWorld,
    context: &str,
    diagnostics: &[typst::diag::SourceDiagnostic],
) -> AppError {
    let details: Vec<String> = diagnostics
        .iter()
        .map(|d| {
            let line = world
                .range(d.span)
                .and_then(|range| world.main_source().lines().byte_to_line(range.start));
            match line {
                Some(line) => format!("line {}: {}", line + 1, d.message),
                None => d.message.to_string(),
            }
        })
        .collect();
    AppError::IO(IOError::Msg(format!("{}: {}", context, details.join("; "))))
}

fn open_with_system_viewer(path: &Path) -> Result<(), AppError> {
    // the empty argument is the window title: without it `start` would treat
    // a quoted path (e.g. containing spaces) as the title
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut c = Command::new("cmd");
        c.args(["/C", "start", ""]).arg(path);
        c
    };

    #[cfg(target_os = "macos")]
    let mut command = {
        let mut c = Command::new("open");
        c.arg(path);
        c
    };

    #[cfg(target_os = "linux")]
    let mut command = {
        let mut c = Command::new("xdg-open");
        c.arg(path);
        c
    };

    #[allow(clippy::zombie_processes)]
    command
        .spawn()
        .map(|_| ())
        .map_err(|e| AppError::IO(IOError::from(e)))
}
