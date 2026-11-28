//! The sheets of the training: the sheet of a session and the plan of a
//! microcycle, laid out by `training.typ`. Pure: the caller reads the data.

use crate::localization::Labels;
use crate::reporting::logo::Logo;
use crate::training::cycles::{Cycle, Window};
use crate::training::venue::Venue;
use crate::training::{Activity, AttendanceRow, AttendanceStatus, SessionStatus, TrainingSession};
use chrono::{Datelike, NaiveDateTime};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct SheetLabels {
    pub created_with: &'static str,
    pub season: &'static str,
    pub objectives: &'static str,
    pub activities: &'static str,
    pub sessions: &'static str,
    pub participants: &'static str,
    pub notes: &'static str,
    pub location: &'static str,
    pub day: &'static str,
    pub time: &'static str,
    pub cancelled: &'static str,
    pub guest: &'static str,
}

#[derive(Debug, Serialize)]
pub struct Sheet {
    pub labels: SheetLabels,
    /// "Training session" or "Microcycle plan"
    pub kind: &'static str,
    pub team: String,
    pub logo: Option<Logo>,
    pub season: String,
    /// the title of the session or of the microcycle
    pub title: String,
    /// when, and where for a session; the days of a microcycle
    pub subtitle: String,
    pub printed: String,
    pub objectives: String,
    /// the table of the sessions, for a microcycle
    pub table: bool,
    pub sessions: Vec<SheetSession>,
}

#[derive(Debug, Serialize)]
pub struct SheetSession {
    pub day: String,
    pub time: String,
    pub location: String,
    pub title: String,
    pub objectives: String,
    pub cancelled: bool,
    /// what heads its part of the plan of a microcycle; none when it has no
    /// title nor activities, as the table of the sessions says the rest
    pub heading: Option<String>,
    pub activities: Vec<SheetActivity>,
    /// none when not asked, or when never recorded
    pub participants: Option<Vec<Participant>>,
    pub notes: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SheetActivity {
    pub name: String,
    pub description: String,
    /// the planned duration, empty when there is none
    pub minutes: String,
}

/// A line of the attendance: the note on the player is not part of it.
#[derive(Debug, Serialize)]
pub struct Participant {
    pub number: String,
    pub name: String,
    pub status: &'static str,
    pub guest: bool,
}

/// What the sheet of a session shows besides the plan.
#[derive(Debug, Clone, Copy, Default)]
pub struct SessionOptions {
    pub participants: bool,
    pub notes: bool,
}

/// Who and when: the same on every sheet.
pub struct Context<'a> {
    pub team: &'a str,
    pub logo: Option<Logo>,
    pub year: u16,
    pub printed: NaiveDateTime,
    /// the gyms the sessions may hold, to name them
    pub venues: &'a [Venue],
}

impl Context<'_> {
    /// The name of the gym of a session, empty when it has none.
    fn venue_name(&self, session: &TrainingSession) -> String {
        session
            .venue_id
            .and_then(|id| self.venues.iter().find(|v| v.id == id))
            .map(|v| v.name.clone())
            .unwrap_or_default()
    }
}

fn sheet_labels(labels: &Labels) -> SheetLabels {
    SheetLabels {
        created_with: labels.created_with,
        season: labels.sheet_season,
        objectives: labels.sheet_objectives,
        activities: labels.sheet_activities,
        sessions: labels.sheet_sessions,
        participants: labels.sheet_participants,
        notes: labels.sheet_notes,
        location: labels.sheet_location,
        day: labels.sheet_day,
        time: labels.sheet_time,
        cancelled: labels.sheet_cancelled,
        guest: labels.sheet_guest,
    }
}

fn minutes(labels: &Labels, planned: Option<u16>) -> String {
    planned
        .map(|m| labels.sheet_minutes.replace("{}", &m.to_string()))
        .unwrap_or_default()
}

fn weekday(labels: &Labels, at: NaiveDateTime) -> &'static str {
    labels
        .sheet_weekdays
        .get(at.weekday().num_days_from_monday() as usize)
        .copied()
        .unwrap_or_default()
}

fn status_label(labels: &Labels, status: AttendanceStatus) -> &'static str {
    match status {
        AttendanceStatus::Present => labels.sheet_present,
        AttendanceStatus::Absent => labels.sheet_absent,
        AttendanceStatus::Late => labels.sheet_late,
        AttendanceStatus::Unavailable => labels.sheet_unavailable,
    }
}

fn sheet_activities(labels: &Labels, activities: &[Activity]) -> Vec<SheetActivity> {
    activities
        .iter()
        .map(|a| SheetActivity {
            name: a.name.clone(),
            description: a.description.clone(),
            minutes: minutes(labels, a.planned_min),
        })
        .collect()
}

fn sheet_session(
    labels: &Labels,
    ctx: &Context,
    session: &TrainingSession,
    activities: &[Activity],
    participants: Option<Vec<Participant>>,
    notes: Option<String>,
) -> SheetSession {
    let day = format!(
        "{} {}",
        weekday(labels, session.start),
        session.start.format("%d/%m")
    );
    let time = session.start.format("%H:%M").to_string();
    let heading = match (session.title.as_str(), activities) {
        ("", []) => None,
        (title, _) => Some(once_joined(
            [format!("{day} {time}"), title.to_string()]
                .into_iter()
                .filter(|part| !part.is_empty()),
        )),
    };
    SheetSession {
        day,
        time,
        heading,
        location: ctx.venue_name(session),
        title: session.title.clone(),
        objectives: session.objectives.clone(),
        cancelled: session.status == SessionStatus::Cancelled,
        activities: sheet_activities(labels, activities),
        participants,
        notes,
    }
}

fn participants(labels: &Labels, rows: &[AttendanceRow]) -> Vec<Participant> {
    rows.iter()
        .map(|r| Participant {
            number: r.number.map(|n| n.to_string()).unwrap_or_default(),
            name: r.name.clone(),
            status: status_label(labels, r.status),
            guest: r.guest,
        })
        .collect()
}

fn base(labels: &Labels, ctx: &Context, kind: &'static str) -> Sheet {
    Sheet {
        labels: sheet_labels(labels),
        kind,
        team: ctx.team.to_string(),
        logo: ctx.logo.clone(),
        season: ctx.year.to_string(),
        title: String::new(),
        subtitle: String::new(),
        printed: labels
            .sheet_printed
            .replace("{}", &ctx.printed.format("%d/%m/%Y %H:%M").to_string()),
        objectives: String::new(),
        table: false,
        sessions: Vec::new(),
    }
}

/// The sheet of a session. `attendance` is the saved one: `None` when it was
/// never recorded, so there is nothing to show. The notes on the players
/// and the final remarks never go in.
pub fn session_sheet(
    labels: &Labels,
    ctx: &Context,
    session: &TrainingSession,
    activities: &[Activity],
    attendance: Option<&[AttendanceRow]>,
    options: SessionOptions,
) -> Sheet {
    let venue = ctx.venue_name(session);
    let place = [venue.as_str()].into_iter().filter(|l| !l.is_empty());
    let subtitle = once_joined(
        std::iter::once(format!(
            "{} {} · {}",
            weekday(labels, session.start),
            session.start.format("%d/%m/%Y"),
            session.start.format("%H:%M")
        ))
        .chain(place.map(str::to_string)),
    );
    let people = attendance
        .filter(|_| options.participants)
        .map(|rows| participants(labels, rows));
    let notes = Some(session.notes.clone()).filter(|n| options.notes && !n.is_empty());
    Sheet {
        title: session.title.clone(),
        subtitle,
        objectives: session.objectives.clone(),
        sessions: vec![sheet_session(
            labels, ctx, session, activities, people, notes,
        )],
        ..base(labels, ctx, labels.sheet_session)
    }
}

fn once_joined(parts: impl Iterator<Item = String>) -> String {
    parts.collect::<Vec<_>>().join(" · ")
}

/// The first and last day of a window, written as the dates of the sheets.
fn days(window: Window) -> String {
    let last = window.end.pred_opt().unwrap_or(window.start);
    format!(
        "{} – {}",
        window.start.format("%d/%m/%Y"),
        last.format("%d/%m/%Y")
    )
}

/// The plan of a microcycle: its window, objectives and sessions (by
/// start, as given), each with its activities. Neither attendance nor
/// notes.
pub fn microcycle_sheet(
    labels: &Labels,
    ctx: &Context,
    cycle: &Cycle,
    sessions: &[(TrainingSession, Vec<Activity>)],
) -> Sheet {
    Sheet {
        title: cycle.title.clone(),
        subtitle: days(cycle.window),
        objectives: cycle.objectives.clone(),
        table: true,
        sessions: sessions
            .iter()
            .map(|(s, a)| sheet_session(labels, ctx, s, a, None, None))
            .collect(),
        ..base(labels, ctx, labels.sheet_microcycle)
    }
}
