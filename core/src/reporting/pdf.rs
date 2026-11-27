use crate::errors::{AppError, IOError};
use crate::localization::Labels;
use crate::reporting::logo::MatchLogos;
use crate::reporting::model::{Report, build_report, build_report_of_both};
use crate::reporting::training::Sheet;
use crate::reporting::typst_world::ReportWorld;
use crate::shapes::r#match::MatchEntry;
use serde::{Deserialize, Serialize};
use typst::WorldExt;
use typst::foundations::{Dict, Value};
use typst_layout::PagedDocument;
use typst_pdf::PdfOptions;

/// The layout and styling of the report: it lays out the `Report` passed
/// as `sys.inputs.report`.
const TEMPLATE: &str = include_str!("report.typ");

pub fn render_match_pdf(
    m: &MatchEntry,
    labels: &Labels,
    logos: MatchLogos,
) -> Result<Vec<u8>, AppError> {
    render_report_pdf(&build_report(m, labels)?.with_logos(logos))
}

/// The report of a match scouted on both sides: `b` is the match as its
/// team sees it.
pub fn render_both_pdf(
    a: &MatchEntry,
    b: &MatchEntry,
    labels: &Labels,
    logos: MatchLogos,
) -> Result<Vec<u8>, AppError> {
    render_report_pdf(&build_report_of_both(a, b, labels)?.with_logos(logos))
}

/// The layout of the sheets of the training.
const TRAINING_TEMPLATE: &str = include_str!("training.typ");

/// The sheet of a session or the plan of a microcycle, as a PDF.
pub fn render_training_pdf(sheet: &Sheet) -> Result<Vec<u8>, AppError> {
    render(TRAINING_TEMPLATE, "sheet", sheet)
}

pub fn render_report_pdf(report: &Report) -> Result<Vec<u8>, AppError> {
    render(TEMPLATE, "report", report)
}

/// What both templates start with.
const LOGO_TEMPLATE: &str = include_str!("logo.typ");

fn render(template: &str, key: &str, data: &impl Serialize) -> Result<Vec<u8>, AppError> {
    let world = ReportWorld::new(
        &[LOGO_TEMPLATE, template].concat(),
        report_inputs(key, data)?,
    );
    let document = typst::compile::<PagedDocument>(&world)
        .output
        .map_err(|e| typst_error(&world, "error compiling typst", &e))?;
    typst_pdf::pdf(&document, &PdfOptions::default())
        .map_err(|e| typst_error(&world, "error exporting PDF", &e))
}

/// The data as the typst value of `sys.inputs.<key>`.
fn report_inputs(key: &str, data: &impl Serialize) -> Result<Dict, AppError> {
    let msg = |e: serde_json::Error| {
        AppError::IO(IOError::Msg(format!("error encoding the report: {e}")))
    };
    let json = serde_json::to_value(data).map_err(msg)?;
    let value = Value::deserialize(json).map_err(msg)?;
    Ok(Dict::from_iter([(key.into(), value)]))
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
                Some(line) => format!(
                    "line {}: {}",
                    (line + 1).saturating_sub(LOGO_TEMPLATE.lines().count()),
                    d.message
                ),
                None => d.message.to_string(),
            }
        })
        .collect();
    AppError::IO(IOError::Msg(format!("{}: {}", context, details.join("; "))))
}
