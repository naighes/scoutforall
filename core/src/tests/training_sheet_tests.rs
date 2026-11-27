//! The sheets of the training: what they show (`session_sheet`,
//! `microcycle_sheet`) and that the template lays them out in every
//! language.
#[cfg(test)]
mod tests {
    use crate::localization::labels;
    use crate::reporting::pdf::render_training_pdf;
    use crate::reporting::training::{Context, SessionOptions, microcycle_sheet, session_sheet};
    use crate::shapes::enums::LanguageEnum;
    use crate::tests::logos::{image_count, logo};
    use crate::training::cycles::{Cycle, Level, Span, Window};
    use crate::training::venue::Venue;
    use crate::training::{
        Activity, ActivityStatus, AttendanceRow, AttendanceStatus, SessionStatus, TrainingSession,
    };
    use chrono::{NaiveDate, NaiveDateTime};
    use uuid::Uuid;

    fn at(day: u32, hour: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 11, day)
            .and_then(|d| d.and_hms_opt(hour, 30, 0))
            .unwrap_or_default()
    }

    const GYM: Uuid = Uuid::from_u128(7);

    fn session(day: u32, status: SessionStatus) -> TrainingSession {
        TrainingSession {
            id: Uuid::new_v4(),
            team_id: Uuid::nil(),
            microcycle_id: None,
            series_id: None,
            start: at(day, 18),
            duration_min: 90,
            actual_duration_min: None,
            venue_id: Some(GYM),
            title: "Ricezione".into(),
            objectives: "serve\nreception".into(),
            notes: "bring the balls".into(),
            final_remarks: "SECRET remarks".into(),
            status,
        }
    }

    fn activity(name: &str, minutes: Option<u16>) -> Activity {
        Activity {
            id: Uuid::new_v4(),
            session_id: Uuid::nil(),
            position: 0,
            name: name.into(),
            description: "three\nlines".into(),
            planned_min: minutes,
            actual_min: None,
            notes: "SECRET activity notes".into(),
            status: ActivityStatus::Planned,
        }
    }

    fn row(name: &str, status: AttendanceStatus, guest: bool) -> AttendanceRow {
        AttendanceRow {
            person_id: Uuid::new_v4(),
            name: name.into(),
            number: Some(7),
            status,
            note: "SECRET player note".into(),
            guest,
            makeup: false,
        }
    }

    fn ctx() -> Context<'static> {
        Context {
            team: "東京 Volley",
            logo: None,
            year: 2026,
            printed: at(22, 9),
            venues: Box::leak(Box::new(vec![Venue {
                id: GYM,
                name: "Palestra Łódź".into(),
                address: String::new(),
                map_url: String::new(),
                coordinates: None,
                notes: String::new(),
                pinned: false,
                deleted: false,
            }])),
        }
    }

    fn json(sheet: &impl serde::Serialize) -> String {
        serde_json::to_string(sheet).unwrap_or_default()
    }

    /// Participants and notes only when asked; never the notes on the
    /// players, the notes on the activities nor the final remarks.
    #[test]
    fn the_session_sheet_shows_what_is_asked() {
        let l = labels(LanguageEnum::En);
        let s = session(23, SessionStatus::Planned);
        let a = [activity("Serve", Some(20)), activity("Block", None)];
        let rows = [row("Anna", AttendanceStatus::Late, false)];
        let plain = session_sheet(l, &ctx(), &s, &a, Some(&rows), SessionOptions::default());
        let full = session_sheet(
            l,
            &ctx(),
            &s,
            &a,
            Some(&rows),
            SessionOptions {
                participants: true,
                notes: true,
            },
        );
        assert!(plain.sessions[0].participants.is_none() && plain.sessions[0].notes.is_none());
        assert_eq!(full.sessions[0].notes.as_deref(), Some("bring the balls"));
        let people = full.sessions[0].participants.as_deref().unwrap_or_default();
        assert_eq!(
            (people[0].name.as_str(), people[0].status),
            ("Anna", "Late")
        );
        assert_eq!(full.sessions[0].activities[0].minutes, "20 min");
        assert_eq!(full.sessions[0].activities[1].minutes, "");
        assert_eq!(full.subtitle, "Mon 23/11/2026 · 18:30 · Palestra Łódź");
        for sheet in [&plain, &full] {
            let text = json(sheet);
            assert!(!text.contains("SECRET"), "{text}");
        }
    }

    /// A session without recorded attendance has nothing to show.
    #[test]
    fn no_attendance_no_participants() {
        let l = labels(LanguageEnum::En);
        let s = session(23, SessionStatus::Planned);
        let sheet = session_sheet(
            l,
            &ctx(),
            &s,
            &[],
            None,
            SessionOptions {
                participants: true,
                notes: false,
            },
        );
        assert!(sheet.sessions[0].participants.is_none());
    }

    #[test]
    fn the_microcycle_plan_marks_the_cancelled_sessions() {
        let l = labels(LanguageEnum::It);
        let start = NaiveDate::from_ymd_opt(2026, 11, 23).unwrap_or_default();
        let cycle = Cycle {
            id: Uuid::new_v4(),
            team_id: Uuid::nil(),
            level: Level::Microcycle,
            parent_id: None,
            title: "Settimana 1".into(),
            span: Span::Weeks(1),
            window: Window::of(Level::Microcycle, start, Span::Weeks(1))
                .unwrap_or(Window { start, end: start }),
            objectives: "battuta".into(),
            notes: "SECRET cycle notes".into(),
        };
        let sessions = [
            (
                session(23, SessionStatus::Planned),
                vec![activity("Muro", Some(30))],
            ),
            (session(25, SessionStatus::Cancelled), vec![]),
        ];
        let sheet = microcycle_sheet(l, &ctx(), &cycle, &sessions);
        assert!(sheet.table);
        assert_eq!(sheet.subtitle, "2026-11-23 – 2026-11-29");
        assert_eq!(sheet.sessions[0].day, "Lun 23/11");
        assert!(!sheet.sessions[0].cancelled && sheet.sessions[1].cancelled);
        assert!(
            sheet
                .sessions
                .iter()
                .all(|s| s.participants.is_none() && s.notes.is_none())
        );
        assert!(!json(&sheet).contains("SECRET"));
    }

    /// Both sheets in every language, with long and CJK names.
    #[test]
    fn every_language_renders() {
        let start = NaiveDate::from_ymd_opt(2026, 11, 23).unwrap_or_default();
        let cycle = Cycle {
            id: Uuid::new_v4(),
            team_id: Uuid::nil(),
            level: Level::Microcycle,
            parent_id: None,
            title: "週 1".into(),
            span: Span::Weeks(1),
            window: Window { start, end: start },
            objectives: "サーブ".into(),
            notes: String::new(),
        };
        for language in LanguageEnum::ALL {
            let l = labels(language);
            let s = session(23, SessionStatus::Planned);
            let many: Vec<Activity> = (0..40)
                .map(|i| activity(&format!("東京 {i}"), Some(5)))
                .collect();
            let rows: Vec<AttendanceRow> = (0..30)
                .map(|i| {
                    row(
                        &format!("Łukasz 東京 {i}"),
                        AttendanceStatus::Present,
                        i % 5 == 0,
                    )
                })
                .collect();
            let sheet = session_sheet(
                l,
                &ctx(),
                &s,
                &many,
                Some(&rows),
                SessionOptions {
                    participants: true,
                    notes: true,
                },
            );
            let pdf = render_training_pdf(&sheet)
                .unwrap_or_else(|e| panic!("expected the session sheet in {language}: {e}"));
            assert!(pdf.starts_with(b"%PDF"));
            let sessions = [
                (session(23, SessionStatus::Planned), many),
                (session(25, SessionStatus::Cancelled), vec![]),
                (
                    session(26, SessionStatus::Completed),
                    vec![activity("Muro", None)],
                ),
            ];
            let plan = microcycle_sheet(l, &ctx(), &cycle, &sessions);
            render_training_pdf(&plan)
                .unwrap_or_else(|e| panic!("expected the microcycle plan in {language}: {e}"));
        }
    }

    #[test]
    fn the_sheets_show_the_logo_of_the_team() {
        let l = labels(LanguageEnum::En);
        let s = session(23, SessionStatus::Planned);
        let with = Context {
            logo: Some(logo([200, 0, 0, 255])),
            ..ctx()
        };
        let images = |ctx: &Context| {
            let sheet = session_sheet(l, ctx, &s, &[], None, SessionOptions::default());
            image_count(&render_training_pdf(&sheet).expect("expected the sheet"))
        };
        assert_eq!(images(&ctx()), 0);
        assert!(images(&with) > 0);
    }
}
