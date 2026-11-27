//! The calendar of the training and the matches of a team, as the
//! calendars of the mail draw it: a week (seven columns under the number of
//! their day, the hours on the left, each event as tall as it lasts, the
//! bands of its microcycles above), a month (a box a day, the days of the
//! months around in grey, the bands of its mesocycles above each row) or a
//! list (the days with something one under the other, the date on the
//! left, for a long period). Sessions and matches are told apart by icon
//! and color. The filter hides what it leaves out in all three; free
//! sessions are marked (their box, or space) to make a microcycle, and the
//! marks stay across views and periods. ← → move the day, ↑ ↓ the event of
//! the day (the week), the week (the month) or the event (the list), PgUp
//! PgDn change the period, Home goes back to today, `w` `m` `l` choose the
//! view, `f` the filter, `p` the period of the list, Enter opens the event
//! or, on a day without any, a new session (in the month it opens the week
//! of the day), `n` a new session, `r` a new series, `c` the microcycle of
//! the marked, ⌫ deletes the session chosen. No dragging.

use scout4all_mobile::clock::ClockExt;
use {
    super::screen::{PERIODS, Period, period_name},
    crate::{
        calendar::{day_of, text_of},
        strings::Strings,
        theme::{self, Colors, SIZE_BODY, SIZE_SMALL, SIZE_TITLE},
        ui,
    },
    chrono::{Datelike, Days, Duration, Months, NaiveDate, NaiveTime},
    iced::{
        Alignment, Background, Element, Length, Padding,
        widget::{Column, Id, Row, column, container, row, rule, scrollable, space, text},
    },
    scout4all_core::{
        training::SessionStatus,
        training::agenda::{FIRST_HOUR, LAST_HOUR},
        training::cycles::Level,
    },
    scout4all_keys::{KeyCombination, key},
    scout4all_mobile::{
        MobileError, Scout4All,
        training::{
            AgendaDay, AgendaEventRecord, AgendaItemRecord, AgendaSpan, CycleRecord, EventPlace,
            minute_of, week_layout,
        },
    },
    std::{collections::HashSet, sync::Arc},
};

const LIST: &str = "training-calendar-list";
/// The hours of the week, scrolled to its first event.
const WEEK: &str = "training-calendar-week";
/// The weeks of the month, the one of the cursor kept in sight.
const MONTH: &str = "training-calendar-month";

/// The most events a month cell writes, the others are counted.
const CELL_EVENTS: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum View {
    Week,
    Month,
    /// the days with something, one under the other
    List,
}

/// What the calendar lets through: everything, the sessions only, or the
/// sessions of a state (the matches have none).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Filter {
    All,
    Sessions,
    Planned,
    ToClose,
    Completed,
    Cancelled,
    NoAttendance,
}

pub const FILTERS: [Filter; 7] = [
    Filter::All,
    Filter::Sessions,
    Filter::Planned,
    Filter::ToClose,
    Filter::Completed,
    Filter::Cancelled,
    Filter::NoAttendance,
];

fn filter_name(s: &Strings, filter: Filter) -> &'static str {
    match filter {
        Filter::All => s.events_all,
        Filter::Sessions => s.sessions,
        Filter::Planned => s.filter_planned,
        Filter::ToClose => s.filter_to_close,
        Filter::Completed => s.filter_completed,
        Filter::Cancelled => s.filter_cancelled,
        Filter::NoAttendance => s.filter_no_attendance,
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    Day(NaiveDate),
    Event(NaiveDate, usize),
    Previous,
    Next,
    Today,
    View(View),
    New,
    /// a new match on the day of the cursor
    NewMatch,
    /// a click on an empty place: a new session on the day, at the minute
    /// of the day when it's of the week
    NewAt(NaiveDate, Option<u32>),
    /// this team or all the teams
    AllTeams,
    /// the next gym of the filter
    NextVenue,
    Filter(Filter),
    /// the next period of the list, round
    NextPeriod,
    /// the box of a free session: marked or no more
    Mark(String),
    /// the marked sessions in a new microcycle
    Create,
}

/// What the calendar asks the screen for.
#[derive(Debug, Clone, PartialEq)]
pub enum Leave {
    Stay,
    Session(String),
    Match(String),
    /// a new session on the day, at the time if one was clicked
    NewSession(NaiveDate, Option<NaiveTime>),
    /// a new match on the day
    NewMatch(NaiveDate),
    /// the marked sessions, for a new microcycle
    Create(Vec<String>),
    /// the session to delete, once asked
    Delete(String),
}

pub struct CalendarTab {
    core: Arc<Scout4All>,
    team_id: String,
    view: View,
    /// the day the cursor is on, and so the period shown
    anchor: NaiveDate,
    today: NaiveDate,
    /// the event of the anchor day the cursor is on
    slot: usize,
    days: Vec<AgendaDay>,
    /// the cycles whose bands are drawn: microcycles in the week,
    /// mesocycles in the month
    bands: Vec<CycleRecord>,
    /// the sessions and matches of every team, not only of this one
    all_teams: bool,
    /// the gym the events are limited to: id and name
    venue: Option<(String, String)>,
    /// the gyms there are: the filter shows only with some
    venues: Vec<(String, String)>,
    filter: Filter,
    /// the days of the list
    period: Period,
    /// the sessions marked for a microcycle: they stay across views
    marked: HashSet<String>,
    /// the first day of the week already scrolled to its first event
    week_scrolled: Option<NaiveDate>,
    /// the sessions of the team in no microcycle: the ones that can be marked
    free: HashSet<String>,
    /// the completed sessions shown whose attendance is written
    recorded: HashSet<String>,
    /// the calendar of a cycle's sheet: its days, a week at a time, and
    /// only its sessions
    window: Option<Window>,
    error: Option<String>,
}

/// The days of a microcycle and the sessions it holds: the sheet draws
/// them as the week of the calendar.
struct Window {
    first: NaiveDate,
    last: NaiveDate,
    sessions: HashSet<String>,
}

/// The Monday of the week of a day (the core's, without panics).
pub use scout4all_core::training::cycles::monday;

/// The weeks of the month of `day`, Monday first, `None` out of the month.
pub fn month_weeks(day: NaiveDate) -> Vec<[Option<NaiveDate>; 7]> {
    crate::calendar::weeks(day, false)
}

/// The first and the last day a view shows around `anchor`.
pub fn period(view: View, anchor: NaiveDate) -> (NaiveDate, NaiveDate) {
    match view {
        View::Week => {
            let first = monday(anchor);
            (first, first + Duration::days(6))
        }
        View::Month | View::List => {
            let first = crate::calendar::month_of(anchor);
            let last = first
                .checked_add_months(Months::new(1))
                .map(|next| next - Duration::days(1))
                .unwrap_or(first);
            (first, last)
        }
    }
}

/// The segments of a row of days a band covers: `(first index, days,
/// cycle)`, the days in a row under the same cycle together.
pub fn segments<'a>(
    days: &[Option<NaiveDate>],
    cycles: &'a [CycleRecord],
) -> Vec<(usize, usize, Option<&'a CycleRecord>)> {
    let cover = |day: &Option<NaiveDate>| {
        day.and_then(|d| {
            cycles.iter().find(|c| {
                day_of(&c.first_day).is_some_and(|f| f <= d)
                    && day_of(&c.last_day).is_some_and(|l| d <= l)
            })
        })
    };
    let mut out: Vec<(usize, usize, Option<&CycleRecord>)> = vec![];
    for (i, day) in days.iter().enumerate() {
        let now = cover(day);
        match out.last_mut() {
            Some((_, n, last)) if last.map(|c| &c.id) == now.map(|c| &c.id) => *n += 1,
            _ => out.push((i, 1, now)),
        }
    }
    out
}

impl CalendarTab {
    pub fn new(core: Arc<Scout4All>, team_id: String) -> CalendarTab {
        let today = core.clock().today();
        let mut tab = CalendarTab {
            core,
            team_id,
            view: View::Week,
            anchor: today,
            today,
            slot: 0,
            days: vec![],
            bands: vec![],
            all_teams: false,
            venue: None,
            venues: vec![],
            filter: Filter::All,
            period: Period::Any,
            marked: HashSet::new(),
            week_scrolled: None,
            free: HashSet::new(),
            recorded: HashSet::new(),
            window: None,
            error: None,
        };
        tab.reload();
        tab
    }

    /// The week of the calendar over the days of a microcycle, from
    /// `first` to `last` (included) seven at a time, with only `sessions`
    /// and the matches of the team: the sheet of the microcycle.
    pub fn of_window(
        core: Arc<Scout4All>,
        team_id: String,
        first: NaiveDate,
        last: NaiveDate,
        sessions: HashSet<String>,
        today: NaiveDate,
    ) -> CalendarTab {
        let mut tab = CalendarTab::new(core, team_id);
        tab.today = today;
        // today when the microcycle holds it, its first day otherwise
        tab.anchor = match first <= today && today <= last {
            true => today,
            false => first,
        };
        tab.window = Some(Window {
            first,
            last,
            sessions,
        });
        tab.reload();
        tab
    }

    /// The days shown: the week of the view, or the seven of the window
    /// the cursor is in.
    fn span(&self) -> (NaiveDate, NaiveDate) {
        match &self.window {
            Some(w) => {
                let weeks = (self.anchor - w.first).num_days().max(0) / 7;
                let from = w.first + Duration::days(weeks * 7);
                (from, (from + Duration::days(6)).min(w.last))
            }
            None => period(self.view, self.anchor),
        }
    }

    /// The days of the window around the cursor, empty ones too, with only
    /// the sessions of the microcycle and the matches of the team.
    fn read_window(&self, w: &Window) -> Result<Vec<AgendaDay>, MobileError> {
        let (from, to) = self.span();
        let busy = self.core.training_agenda_between(
            Some(self.team_id.clone()),
            Some(text_of(from)),
            Some(text_of(to)),
        )?;
        Ok(from
            .iter_days()
            .take_while(|d| *d <= to)
            .map(|day| AgendaDay {
                date: text_of(day),
                events: busy
                    .iter()
                    .find(|b| day_of(&b.date) == Some(day))
                    .map(|b| {
                        b.events
                            .iter()
                            .filter(|e| match &e.item {
                                AgendaItemRecord::Session { id, .. } => w.sessions.contains(id),
                                AgendaItemRecord::Match { .. } => true,
                            })
                            .cloned()
                            .collect()
                    })
                    .unwrap_or_default(),
            })
            .collect())
    }

    /// The hours of the week: the day's in the calendar; in a sheet only
    /// the busy ones and one around them, so an evening session is in
    /// sight without scrolling.
    fn hours_shown(&self) -> (u32, u32) {
        let spans: Vec<(u32, u32)> = self
            .days
            .iter()
            .flat_map(|d| &d.events)
            .filter_map(|e| {
                minute_of(&e.start).map(|m| (m / 60, (m + u32::from(e.minutes)).div_ceil(60)))
            })
            .collect();
        match (&self.window, spans.is_empty()) {
            (Some(_), false) => {
                let first = spans.iter().map(|(f, _)| *f).min().unwrap_or(FIRST_HOUR);
                let last = spans.iter().map(|(_, l)| *l).max().unwrap_or(LAST_HOUR);
                (first.saturating_sub(1), (last + 1).min(24))
            }
            _ => {
                let layout = week_layout(&self.days);
                (layout.first_hour, layout.last_hour)
            }
        }
    }

    /// Whether the window has more than a week: then it moves.
    fn window_weeks(&self) -> bool {
        self.window
            .as_ref()
            .is_some_and(|w| (w.last - w.first).num_days() >= 7)
    }

    /// The tab on a given day (the tests'), instead of the real today.
    #[cfg(test)]
    pub fn at(core: Arc<Scout4All>, team_id: String, today: NaiveDate) -> CalendarTab {
        let mut tab = CalendarTab::new(core, team_id);
        tab.today = today;
        tab.anchor = today;
        tab.reload();
        tab
    }

    #[cfg(test)]
    pub fn view_shown(&self) -> View {
        self.view
    }

    #[cfg(test)]
    pub fn anchor(&self) -> NaiveDate {
        self.anchor
    }

    #[cfg(test)]
    pub fn days(&self) -> &[AgendaDay] {
        &self.days
    }

    #[cfg(test)]
    pub fn bands(&self) -> &[CycleRecord] {
        &self.bands
    }

    /// The ids of the sessions shown, in their order.
    #[cfg(test)]
    pub fn shown_sessions(&self) -> Vec<String> {
        self.days
            .iter()
            .flat_map(|d| &d.events)
            .filter_map(|e| match &e.item {
                AgendaItemRecord::Session { id, .. } => Some(id.clone()),
                AgendaItemRecord::Match { .. } => None,
            })
            .collect()
    }

    #[cfg(test)]
    pub fn filter_shown(&self) -> Filter {
        self.filter
    }

    #[cfg(test)]
    pub fn period_shown(&self) -> Period {
        self.period
    }

    #[cfg(test)]
    pub fn marked_count(&self) -> usize {
        self.marked.len()
    }

    /// What went wrong doing something: told over the screen.
    pub fn failure(&self) -> &Option<String> {
        &self.error
    }

    pub fn failure_mut(&mut self) -> &mut Option<String> {
        &mut self.error
    }

    /// Read again, after a change or a move.
    pub fn reload(&mut self) {
        if let Some(w) = &self.window {
            match self.read_window(w) {
                Ok(days) => {
                    self.days = days;
                    self.error = None;
                }
                Err(e) => {
                    self.days = vec![];
                    self.error = Some(e.to_string());
                }
            }
            self.settle();
            return;
        }
        self.venues = self
            .core
            .venues()
            .unwrap_or_default()
            .into_iter()
            .map(|v| (v.id, v.name))
            .collect();
        // a gym removed meanwhile filters no more
        if self
            .venue
            .as_ref()
            .is_some_and(|(id, _)| !self.venues.iter().any(|(v, _)| v == id))
        {
            self.venue = None;
        }
        let team = (!self.all_teams).then(|| self.team_id.clone());
        let read = match self.view {
            View::List => {
                let days = self.period.days(self.today);
                self.core.training_agenda_between(
                    team,
                    days.map(|(from, _)| text_of(from)),
                    days.map(|(_, to)| text_of(to)),
                )
            }
            View::Week => self
                .core
                .training_agenda(team, AgendaSpan::Week, text_of(self.anchor)),
            View::Month => self
                .core
                .training_agenda(team, AgendaSpan::Month, text_of(self.anchor)),
        };
        let level = match self.view {
            View::Week => Some(Level::Microcycle),
            View::Month => Some(Level::Mesocycle),
            View::List => None,
        };
        let (first, last) = period(self.view, self.anchor);
        let read = read.and_then(|mut days| {
            if let Some((venue, _)) = &self.venue {
                for day in &mut days {
                    day.events
                        .retain(|e| e.venue.as_ref().is_some_and(|v| v.id == *venue));
                }
            }
            let bands = match level {
                None => vec![],
                Some(level) => self
                    .core
                    .cycles(self.team_id.clone(), level)?
                    .into_iter()
                    .filter(|c| {
                        day_of(&c.first_day).is_some_and(|f| f <= last)
                            && day_of(&c.last_day).is_some_and(|l| first <= l)
                    })
                    .collect(),
            };
            let free = self
                .core
                .training_sessions(self.team_id.clone())?
                .into_iter()
                .filter(|r| r.microcycle_id.is_none())
                .map(|r| r.id)
                .collect();
            Ok((days, bands, free))
        });
        match read {
            Ok((days, bands, free)) => {
                self.free = free;
                self.marked.retain(|id| self.free.contains(id));
                self.recorded = self.recorded_of(&days);
                self.days = days;
                self.filter_days();
                self.bands = bands;
                self.error = None;
            }
            Err(e) => {
                self.days = vec![];
                self.bands = vec![];
                self.error = Some(e.to_string());
            }
        }
        self.settle();
    }

    /// The completed sessions of the team among `days` whose attendance is
    /// written: the others are the ones without it.
    fn recorded_of(&self, days: &[AgendaDay]) -> HashSet<String> {
        days.iter()
            .flat_map(|d| &d.events)
            .filter(|e| e.team_id == self.team_id)
            .filter_map(|e| match &e.item {
                AgendaItemRecord::Session {
                    id,
                    status: SessionStatus::Completed,
                    ..
                } => Some(id.clone()),
                _ => None,
            })
            .filter(|id| {
                self.core
                    .session_attendance(id.clone())
                    .is_ok_and(|a| a.recorded)
            })
            .collect()
    }

    /// Whether the filter lets `event` through.
    fn lets(&self, event: &AgendaEventRecord) -> bool {
        let session = match &event.item {
            AgendaItemRecord::Session {
                id,
                status,
                to_close,
                ..
            } => Some((id, *status, *to_close)),
            AgendaItemRecord::Match { .. } => None,
        };
        match (self.filter, session) {
            (Filter::All, _) => true,
            (_, None) => false,
            (Filter::Sessions, Some(_)) => true,
            (Filter::Planned, Some((_, status, _))) => status == SessionStatus::Planned,
            (Filter::ToClose, Some((_, _, to_close))) => to_close,
            (Filter::Completed, Some((_, status, _))) => status == SessionStatus::Completed,
            (Filter::Cancelled, Some((_, status, _))) => status == SessionStatus::Cancelled,
            // another team's attendance is not read: it is not shown
            (Filter::NoAttendance, Some((id, status, _))) => {
                status == SessionStatus::Completed
                    && event.team_id == self.team_id
                    && !self.recorded.contains(id)
            }
        }
    }

    /// The events the filter leaves out hidden; in the list the days left
    /// empty too.
    fn filter_days(&mut self) {
        let mut days = std::mem::take(&mut self.days);
        for day in &mut days {
            day.events.retain(|e| self.lets(e));
        }
        if self.view == View::List {
            days.retain(|d| !d.events.is_empty());
        }
        self.days = days;
    }

    /// The cursor on what is shown: in the list on a day of it (the first
    /// from the one it was on, or the last), the event within its day.
    fn settle(&mut self) {
        if self.view == View::List && self.day_record(self.anchor).is_none() {
            let dates: Vec<NaiveDate> = self.days.iter().filter_map(|d| day_of(&d.date)).collect();
            let found = dates
                .iter()
                .find(|d| **d >= self.anchor)
                .or(dates.last())
                .copied();
            if let Some(day) = found {
                self.anchor = day;
                self.slot = 0;
            }
        }
        self.slot = self.slot.min(self.events_of(self.anchor).saturating_sub(1));
    }

    /// The events in the order of the list: day and place in the day.
    fn flat(&self) -> Vec<(NaiveDate, usize)> {
        self.days
            .iter()
            .filter_map(|d| day_of(&d.date).map(|date| (date, d.events.len())))
            .flat_map(|(date, n)| (0..n).map(move |i| (date, i)))
            .collect()
    }

    /// The event after (or before) the cursor, in the list.
    fn step_event(&mut self, forward: bool) {
        let flat = self.flat();
        let at = flat
            .iter()
            .position(|(d, i)| *d == self.anchor && *i == self.slot);
        let next = match (at, forward) {
            (None, _) => flat.first(),
            (Some(i), true) => flat.get(i + 1).or(flat.get(i)),
            (Some(i), false) => flat.get(i.saturating_sub(1)),
        };
        if let Some((day, slot)) = next.copied() {
            self.anchor = day;
            self.slot = slot;
        }
    }

    /// The day with something after (or before) the cursor, in the list.
    fn step_day(&mut self, forward: bool) {
        let dates: Vec<NaiveDate> = self.days.iter().filter_map(|d| day_of(&d.date)).collect();
        let next = match forward {
            true => dates.iter().find(|d| **d > self.anchor),
            false => dates.iter().rev().find(|d| **d < self.anchor),
        };
        if let Some(day) = next.copied() {
            self.anchor = day;
            self.slot = 0;
        }
    }

    /// The session of the team at the cursor, if free (in no microcycle).
    fn session_at_cursor(&self) -> Option<String> {
        self.day_record(self.anchor)
            .and_then(|d| d.events.get(self.slot))
            .filter(|e| e.team_id == self.team_id)
            .and_then(|e| match &e.item {
                AgendaItemRecord::Session { id, .. } => Some(id.clone()),
                AgendaItemRecord::Match { .. } => None,
            })
    }

    fn toggle_mark(&mut self, id: &str) {
        let was = self.marked.remove(id);
        if self.free.contains(id) && !was {
            self.marked.insert(id.to_string());
        }
    }

    /// The marked sessions, for a new microcycle; none marked, nothing.
    fn create(&mut self) -> Leave {
        match self.marked.is_empty() {
            true => Leave::Stay,
            false => {
                let mut ids: Vec<String> = self.marked.drain().collect();
                ids.sort();
                Leave::Create(ids)
            }
        }
    }

    fn set_filter(&mut self, filter: Filter) {
        self.filter = filter;
        // what is still to close (or to take the attendance of) is in the
        // past: the list looks at any date for it
        if matches!(filter, Filter::ToClose | Filter::NoAttendance) && self.view == View::List {
            self.period = Period::Any;
        }
        self.slot = 0;
        self.reload();
    }

    fn step_filter(&mut self) {
        if let Some(next) = ui::cycle(&FILTERS, &self.filter, true) {
            self.set_filter(next);
        }
    }

    pub(super) fn set_period(&mut self, period: Period) {
        self.period = period;
        self.slot = 0;
        self.reload();
    }

    fn step_period(&mut self) {
        if let Some(next) = ui::cycle(&PERIODS, &self.period, true) {
            self.set_period(next);
        }
    }

    /// Where the list is, to keep the event chosen in sight, or the month,
    /// to keep the week of the cursor in sight (in a small window the
    /// month doesn't fit); the week scrolls by itself (`take_week_scroll`).
    pub fn scroll(&self) -> Option<(Id, f32)> {
        match self.view {
            View::List => {
                let flat = self.flat();
                let at = flat
                    .iter()
                    .position(|(d, i)| *d == self.anchor && *i == self.slot)?;
                let last = flat.len().saturating_sub(1).max(1);
                Some((Id::new(LIST), at as f32 / last as f32))
            }
            View::Month => {
                let weeks = month_weeks(self.anchor);
                let at = weeks
                    .iter()
                    .position(|week| week.contains(&Some(self.anchor)))?;
                let last = weeks.len().saturating_sub(1).max(1);
                Some((Id::new(MONTH), at as f32 / last as f32))
            }
            View::Week => None,
        }
    }

    /// The week by the hour opens an hour before its first event, as on
    /// Android: once a week shown, the wheel free afterwards. Pixels from
    /// the top of the hours.
    pub fn take_week_scroll(&mut self) -> Option<(Id, f32)> {
        let week = (self.view == View::Week && self.window.is_none())
            .then(|| period(self.view, self.anchor).0);
        let fresh = week != self.week_scrolled;
        self.week_scrolled = week;
        let earliest = self
            .days
            .iter()
            .flat_map(|d| &d.events)
            .filter_map(|e| minute_of(&e.start))
            .min();
        match (fresh, week, earliest) {
            (true, Some(_), Some(minute)) => {
                let (first, _) = self.hours_shown();
                let hours = (minute / 60).saturating_sub(1).saturating_sub(first);
                Some((Id::new(WEEK), hours as f32 * HOUR))
            }
            _ => None,
        }
    }

    /// The week scrolled again when it shows next (its hours are drawn
    /// anew from the top).
    pub fn forget_week_scroll(&mut self) {
        self.week_scrolled = None;
    }

    fn day_record(&self, day: NaiveDate) -> Option<&AgendaDay> {
        self.days.iter().find(|d| day_of(&d.date) == Some(day))
    }

    fn events_of(&self, day: NaiveDate) -> usize {
        self.day_record(day).map_or(0, |d| d.events.len())
    }

    /// Move the cursor to `day`, reading again when the period changes.
    fn go_to(&mut self, day: NaiveDate) {
        let (first, last) = self.span();
        // the window holds the cursor within its days
        let day = match &self.window {
            Some(w) => day.clamp(w.first, w.last),
            None => day,
        };
        self.anchor = day;
        self.slot = 0;
        // the list holds its period whatever the day
        if self.view != View::List && (day < first || day > last) {
            self.reload();
        }
    }

    fn shift(&mut self, forward: bool) {
        if let Some(w) = &self.window {
            let (from, _) = self.span();
            let day = match forward {
                true => from + Duration::days(7),
                false => from - Duration::days(7),
            };
            if w.first <= day && day <= w.last {
                self.anchor = day;
                self.slot = 0;
                self.reload();
            }
            return;
        }
        let day = match (self.view, forward) {
            (View::Week, true) => self.anchor.checked_add_days(Days::new(7)),
            (View::Week, false) => self.anchor.checked_sub_days(Days::new(7)),
            (View::Month, true) => self.anchor.checked_add_months(Months::new(1)),
            (View::Month, false) => self.anchor.checked_sub_months(Months::new(1)),
            (View::List, _) => None,
        };
        if let Some(day) = day {
            self.anchor = day;
            self.slot = 0;
            self.reload();
        }
    }

    fn set_view(&mut self, view: View) {
        if self.view != view {
            self.view = view;
            self.slot = 0;
            self.reload();
        }
    }

    fn go_today(&mut self) {
        self.anchor = self.today;
        self.slot = 0;
        self.reload();
    }

    /// What Enter does on the cursor.
    fn open(&mut self) -> Leave {
        match self.view {
            View::Month => {
                self.set_view(View::Week);
                Leave::Stay
            }
            View::Week | View::List => self.pick(self.anchor, self.slot),
        }
    }

    /// The next gym of the filter: none, then each of them, then none.
    fn next_venue(&mut self) {
        let venues = &self.venues;
        self.venue = match &self.venue {
            None => venues.first(),
            Some((id, _)) => venues
                .iter()
                .position(|(v, _)| v == id)
                .and_then(|at| venues.get(at + 1)),
        }
        .cloned();
        self.slot = 0;
        self.reload();
    }

    fn set_all_teams(&mut self) {
        self.all_teams = !self.all_teams;
        self.slot = 0;
        self.reload();
    }

    fn pick(&self, day: NaiveDate, slot: usize) -> Leave {
        // what another team holds opens from its own screen
        let event = self.day_record(day).and_then(|d| d.events.get(slot));
        if event.is_some_and(|e| e.team_id != self.team_id) {
            return Leave::Stay;
        }
        match event {
            Some(AgendaEventRecord {
                item: AgendaItemRecord::Session { id, .. },
                ..
            }) => Leave::Session(id.clone()),
            Some(AgendaEventRecord {
                item: AgendaItemRecord::Match { id, .. },
                ..
            }) => Leave::Match(id.clone()),
            None => Leave::NewSession(day, None),
        }
    }

    pub fn update(&mut self, message: Message) -> Leave {
        match message {
            Message::Day(day) => {
                self.go_to(day);
                Leave::Stay
            }
            Message::Event(day, slot) => {
                self.go_to(day);
                self.slot = slot;
                self.pick(day, slot)
            }
            Message::Previous => {
                self.shift(false);
                Leave::Stay
            }
            Message::Next => {
                self.shift(true);
                Leave::Stay
            }
            Message::Today => {
                self.go_today();
                Leave::Stay
            }
            Message::View(view) => {
                self.set_view(view);
                Leave::Stay
            }
            Message::New => Leave::NewSession(self.anchor, None),
            Message::NewMatch => Leave::NewMatch(self.anchor),
            Message::NewAt(day, minute) => {
                self.go_to(day);
                Leave::NewSession(
                    day,
                    minute.and_then(|m| NaiveTime::from_hms_opt(m / 60, m % 60, 0)),
                )
            }
            Message::AllTeams => {
                self.set_all_teams();
                Leave::Stay
            }
            Message::NextVenue => {
                self.next_venue();
                Leave::Stay
            }
            Message::Filter(filter) => {
                self.set_filter(filter);
                Leave::Stay
            }
            Message::NextPeriod => {
                self.step_period();
                Leave::Stay
            }
            Message::Mark(id) => {
                self.toggle_mark(&id);
                Leave::Stay
            }
            Message::Create => self.create(),
        }
    }

    pub fn key(&mut self, ck: KeyCombination) -> Leave {
        let by_days = |tab: &mut CalendarTab, days: i64| {
            let next = match days < 0 {
                true => tab.anchor.checked_sub_days(Days::new(days.unsigned_abs())),
                false => tab.anchor.checked_add_days(Days::new(days.unsigned_abs())),
            };
            if let Some(day) = next {
                tab.go_to(day);
            }
        };
        let list = self.view == View::List;
        match ck {
            k if k == key!(left) && list => self.step_day(false),
            k if k == key!(right) && list => self.step_day(true),
            k if k == key!(up) && list => self.step_event(false),
            k if k == key!(down) && list => self.step_event(true),
            k if k == key!(left) => by_days(self, -1),
            k if k == key!(right) => by_days(self, 1),
            k if k == key!(up) && self.view == View::Month => by_days(self, -7),
            k if k == key!(down) && self.view == View::Month => by_days(self, 7),
            k if k == key!(up) => self.slot = self.slot.saturating_sub(1),
            k if k == key!(down) => {
                self.slot = (self.slot + 1).min(self.events_of(self.anchor).saturating_sub(1))
            }
            k if k == key!(pagedown) => self.shift(true),
            k if k == key!(pageup) => self.shift(false),
            k if k == key!(home) => self.go_today(),
            k if k == key!(w) => self.set_view(View::Week),
            k if k == key!(m) => self.set_view(View::Month),
            k if k == key!(l) => self.set_view(View::List),
            k if k == key!(f) => self.step_filter(),
            k if k == key!(p) && list => self.step_period(),
            k if k == key!(space) && self.view != View::Month => {
                if let Some(id) = self.session_at_cursor() {
                    self.toggle_mark(&id);
                }
            }
            k if k == key!(c) => return self.create(),
            k if (k == key!(backspace) || k == key!(delete)) && self.view != View::Month => {
                if let Some(id) = self.session_at_cursor() {
                    return Leave::Delete(id);
                }
            }
            k if k == key!(enter) => return self.open(),
            k if k == key!(n) => return Leave::NewSession(self.anchor, None),
            k if k == key!(g) => return Leave::NewMatch(self.anchor),
            k if k == key!(t) => self.set_all_teams(),
            k if k == key!(v) => self.next_venue(),
            _ => {}
        }
        Leave::Stay
    }

    /// The day of the cursor: where a new session goes.
    pub fn cursor_day(&self) -> NaiveDate {
        self.anchor
    }

    /// The cursor of `other` (the same window read before a change) kept.
    pub fn keep_cursor(&mut self, other: &CalendarTab) {
        self.anchor = other.anchor;
        self.slot = other.slot;
        self.reload();
    }

    /// The keys of the week of a microcycle's sheet: ← → the day, ↑ ↓ the
    /// event, PgUp PgDn its weeks, Enter opens; `None` for the others.
    pub fn window_key(&mut self, ck: KeyCombination) -> Option<Leave> {
        let by_day = |tab: &mut CalendarTab, forward: bool| {
            let next = match forward {
                true => tab.anchor.checked_add_days(Days::new(1)),
                false => tab.anchor.checked_sub_days(Days::new(1)),
            };
            if let Some(day) = next {
                tab.go_to(day);
            }
        };
        match ck {
            k if k == key!(left) => by_day(self, false),
            k if k == key!(right) => by_day(self, true),
            k if k == key!(up) => self.slot = self.slot.saturating_sub(1),
            k if k == key!(down) => {
                self.slot = (self.slot + 1).min(self.events_of(self.anchor).saturating_sub(1))
            }
            k if k == key!(pagedown) => self.shift(true),
            k if k == key!(pageup) => self.shift(false),
            k if k == key!(enter) => return Some(self.pick(self.anchor, self.slot)),
            _ => return None,
        }
        Some(Leave::Stay)
    }

    /// The hints of `window_key`.
    pub fn window_hints(&self, s: &Strings) -> Vec<(String, String)> {
        let mut hints = vec![
            ("← →".to_string(), String::new()),
            ("↑ ↓".to_string(), String::new()),
        ];
        if self.window_weeks() {
            hints.push(("PgUp PgDn".to_string(), s.week.to_string()));
        }
        hints.push(("Enter".to_string(), s.open.to_string()));
        hints
    }

    /// The sheet's calendar: the week of the window, with the arrows
    /// between its weeks when it holds more than one.
    pub fn window_view<'a>(&'a self, s: &'static Strings, c: Colors) -> Element<'a, Message> {
        let body = self.week(s, c);
        match self.window_weeks() {
            false => body,
            true => {
                let (from, to) = self.span();
                let moves = row![
                    ui::tip(
                        ui::pill("‹", false, Some(Message::Previous), c),
                        s.previous_week,
                        c
                    ),
                    text(format!("{} – {}", s.short_date(from), s.short_date(to)))
                        .size(SIZE_BODY)
                        .font(theme::fonts().bold),
                    ui::tip(ui::pill("›", false, Some(Message::Next), c), s.next_week, c),
                ]
                .spacing(6)
                .align_y(Alignment::Center);
                column![moves, body].spacing(10).height(Length::Fill).into()
            }
        }
    }

    pub fn hints(&self, s: &Strings) -> Vec<(String, String)> {
        let mut hints = vec![
            ("← →".to_string(), String::new()),
            (
                "↑ ↓".to_string(),
                match self.view {
                    View::Week | View::List => String::new(),
                    View::Month => s.week.to_string(),
                },
            ),
        ];
        if self.view != View::List {
            hints.push(("PgUp PgDn".to_string(), String::new()));
            hints.push(("Home".to_string(), s.today.to_string()));
        }
        hints.extend([
            (
                "w m l".to_string(),
                format!("{} / {} / {}", s.week, s.calendar_month, s.calendar_list),
            ),
            ("f".to_string(), s.filter.to_string()),
        ]);
        if self.view == View::List {
            hints.push(("p".to_string(), s.period.to_string()));
        }
        if self.view != View::Month {
            hints.push(("space".to_string(), s.select_row.to_string()));
        }
        if !self.marked.is_empty() {
            hints.push(("c".to_string(), s.create_microcycle.to_string()));
        }
        hints.extend([
            ("Enter".to_string(), s.open.to_string()),
            ("n".to_string(), s.new_session.to_string()),
            ("g".to_string(), s.new_match.to_string()),
            ("t".to_string(), s.all_teams.to_string()),
        ]);
        if !self.venues.is_empty() {
            hints.push(("v".to_string(), s.venues.to_string()));
        }
        if self.view != View::Month {
            hints.push(("⌫".to_string(), s.delete.to_string()));
        }
        hints.push(("x".to_string(), s.export_training.to_string()));
        hints.push(("⇥".to_string(), s.programming.to_string()));
        hints.push(("Esc".to_string(), s.back.to_string()));
        hints
    }

    /// What the cursor shows, for the scroll of the view: none, the
    /// calendar fits in its panel.
    fn title(&self, s: &Strings) -> String {
        let (first, last) = period(self.view, self.anchor);
        match self.view {
            View::Week => format!("{} – {}", s.short_date(first), s.short_date(last)),
            View::List => period_name(s, self.period).to_string(),
            View::Month => format!(
                "{} {}",
                s.months
                    .get(first.month0() as usize)
                    .copied()
                    .unwrap_or_default(),
                first.year()
            ),
        }
    }

    pub fn view<'a>(&'a self, s: &'static Strings, c: Colors) -> Element<'a, Message> {
        // the views, a separator, then whose and where
        let mut bar: Vec<Element<'a, Message>> = [
            (View::Week, theme::icons::VIEW_WEEK, s.week),
            (View::Month, theme::icons::VIEW_MONTH, s.calendar_month),
            (View::List, theme::icons::VIEW_LIST, s.calendar_list),
        ]
        .into_iter()
        .map(|(view, icon, label)| {
            ui::bar_item(icon, label, self.view == view, Message::View(view), c)
        })
        .collect();
        bar.push(
            container(rule::vertical(1))
                .height(Length::Fixed(34.0))
                .padding([0, 6])
                .into(),
        );
        let (whose_icon, whose) = match self.all_teams {
            true => (theme::icons::ALL_TEAMS, s.all_teams),
            false => (theme::icons::THIS_TEAM, s.this_team),
        };
        bar.push(ui::bar_item(
            whose_icon,
            whose,
            self.all_teams,
            Message::AllTeams,
            c,
        ));
        // without gyms there is nothing to filter by
        if !self.venues.is_empty() {
            bar.push(ui::bar_item(
                theme::icons::GYM,
                self.venue.as_ref().map_or(s.all_venues, |(_, name)| name),
                self.venue.is_some(),
                Message::NextVenue,
                c,
            ));
        }
        let mut controls: Vec<Element<'a, Message>> = Vec::new();
        // the list holds a period of its own, at its head: nothing to move
        let moves = match self.view {
            View::Week => Some((s.previous_week, s.next_week)),
            View::Month => Some((s.previous_month, s.next_month)),
            View::List => None,
        };
        if let Some((previous, next)) = moves {
            controls.extend([
                ui::tip(
                    ui::pill("‹", false, Some(Message::Previous), c),
                    previous,
                    c,
                ),
                text(self.title(s))
                    .size(SIZE_BODY)
                    .font(theme::fonts().bold)
                    .into(),
                ui::tip(ui::pill("›", false, Some(Message::Next), c), next, c),
            ]);
            let (first, last) = period(self.view, self.anchor);
            if first > self.today || last < self.today {
                controls.push(ui::pill(s.today, false, Some(Message::Today), c));
            }
        }
        controls.push(space::horizontal().width(Length::Fill).into());
        controls.push(ui::pill(s.new_session, false, Some(Message::New), c));
        controls.push(ui::pill(s.new_match, false, Some(Message::NewMatch), c));
        // the filter in every view (a row that goes on below when it's
        // narrow); the microcycle only once sessions are marked
        let mut choices: Vec<Element<'a, Message>> = FILTERS
            .iter()
            .map(|f| {
                ui::pill(
                    filter_name(s, *f),
                    *f == self.filter,
                    Some(Message::Filter(*f)),
                    c,
                )
            })
            .collect();
        if !self.marked.is_empty() {
            choices.push(ui::pill(
                &format!("{} ({})", s.create_microcycle, self.marked.len()),
                true,
                Some(Message::Create),
                c,
            ));
        }
        let body: Element<'a, Message> = match self.view {
            View::Week => self.week(s, c),
            View::Month => scrollable(self.month(s, c))
                .id(Id::new(MONTH))
                .height(Length::Fill)
                .into(),
            View::List => column![self.list_head(s, c), self.list(s, c)]
                .spacing(10)
                .height(Length::Fill)
                .into(),
        };
        column![
            Row::with_children(bar)
                .spacing(4)
                .align_y(Alignment::Center)
                .wrap(),
            Row::with_children(controls)
                .spacing(6)
                .align_y(Alignment::Center),
            Row::with_children(choices)
                .spacing(6)
                .align_y(Alignment::Center)
                .wrap(),
            body
        ]
        .spacing(10)
        .height(Length::Fill)
        .into()
    }

    /// The period is not a view: it heads the list it bounds, with its
    /// count. One pill that goes round the periods.
    fn list_head<'a>(&'a self, s: &'static Strings, c: Colors) -> Element<'a, Message> {
        let sessions = self
            .days
            .iter()
            .flat_map(|d| &d.events)
            .filter(|e| matches!(e.item, AgendaItemRecord::Session { .. }))
            .count();
        row![
            ui::tip(
                ui::pill(
                    period_name(s, self.period),
                    self.period != Period::Any,
                    Some(Message::NextPeriod),
                    c,
                ),
                s.period,
                c,
            ),
            text(s.count(s.sessions_of, sessions))
                .size(SIZE_SMALL)
                .color(c.muted)
        ]
        .spacing(12)
        .align_y(Alignment::Center)
        .into()
    }

    /// The list as the agenda of the calendars of the mail: the days with
    /// something one under the other, the date on the left where the week
    /// has the hours, the events as in the week, the free sessions with
    /// their box.
    fn list<'a>(&'a self, s: &'static Strings, c: Colors) -> Element<'a, Message> {
        if self.days.is_empty() {
            return text(match self.filter {
                Filter::All => s.nothing_planned,
                _ => s.no_sessions,
            })
            .size(SIZE_SMALL)
            .color(c.muted)
            .into();
        }
        let days = self.days.iter().filter_map(|day| {
            let date = day_of(&day.date)?;
            let lit = date == self.today;
            let color = match lit {
                true => c.accent,
                false => c.text,
            };
            let head = button_day(
                column![
                    text(date.day().to_string())
                        .size(SIZE_TITLE + 6.0)
                        .font(theme::fonts().bold)
                        .color(color),
                    text(format!(
                        "{} {}{}",
                        s.weekday(date),
                        s.months
                            .get(date.month0() as usize)
                            .and_then(|m| m.get(..3).or(Some(m)))
                            .unwrap_or_default(),
                        match date.year() == self.today.year() {
                            true => String::new(),
                            false => format!(" {}", date.year()),
                        }
                    ))
                    .size(SIZE_SMALL)
                    .color(match lit {
                        true => c.accent,
                        false => c.muted,
                    }),
                ]
                .spacing(0),
                date == self.anchor,
                Message::Day(date),
                c,
            );
            let events = Column::with_children(day.events.iter().enumerate().map(|(i, e)| {
                let told = crate::agenda::Told {
                    with_team: self.all_teams,
                    // the end of a session; a match is told by its start
                    times: matches!(e.item, AgendaItemRecord::Session { .. })
                        .then(|| crate::calendar::time_span(&e.start, e.minutes)),
                    unrecorded: self.unrecorded(e),
                };
                let chosen = date == self.anchor && i == self.slot;
                let boxed =
                    crate::agenda::event_box(e, s, c, told, chosen, Message::Event(date, i));
                // a place for the mark in every row: they stay aligned
                row![
                    boxed,
                    container(match self.mark_of(e) {
                        Some((marked, toggle)) => mark_button(marked, toggle, LIST_MARK_SIZE, c),
                        None => space::horizontal().width(0).into(),
                    })
                    .width(Length::Fixed(LIST_MARK))
                ]
                .align_y(Alignment::Center)
                .into()
            }))
            .spacing(3)
            .width(Length::Fill);
            let background = match lit {
                true => mix(c.panel, c.accent, 0.07),
                false => c.panel,
            };
            Some(
                container(row![head, events].spacing(10).align_y(Alignment::Center))
                    .padding(Padding::from([6, 8]))
                    .width(Length::Fill)
                    .style(move |_| container::Style {
                        background: Some(Background::Color(background)),
                        ..container::Style::default()
                    })
                    .into(),
            )
        });
        scrollable(Column::with_children(days).spacing(1))
            .id(Id::new(LIST))
            .height(Length::Fill)
            .into()
    }

    /// A completed session of the team whose attendance is not written.
    fn unrecorded(&self, event: &AgendaEventRecord) -> bool {
        match &event.item {
            AgendaItemRecord::Session {
                id,
                status: SessionStatus::Completed,
                ..
            } => event.team_id == self.team_id && !self.recorded.contains(id),
            _ => false,
        }
    }

    /// The box of a free session of the team: marked or not, and its click.
    fn mark_of(&self, event: &AgendaEventRecord) -> Option<(bool, Message)> {
        match &event.item {
            AgendaItemRecord::Session { id, .. }
                if event.team_id == self.team_id && self.free.contains(id) =>
            {
                Some((self.marked.contains(id), Message::Mark(id.clone())))
            }
            _ => None,
        }
    }

    /// The band of a row of days.
    fn band<'a>(
        &'a self,
        days: &[Option<NaiveDate>],
        s: &'static Strings,
        c: Colors,
    ) -> Element<'a, Message> {
        let label = match self.view {
            View::Week => s.microcycle,
            View::Month | View::List => s.mesocycle,
        };
        Row::with_children(segments(days, &self.bands).into_iter().map(
            |(_, count, cycle)| -> Element<'a, Message> {
                let portion = Length::FillPortion(count as u16);
                match cycle {
                    None => space::horizontal().width(portion).into(),
                    Some(cycle) => container(
                        text(match cycle.title.is_empty() {
                            true => label.to_string(),
                            false => cycle.title.clone(),
                        })
                        .size(SIZE_SMALL)
                        .color(c.on_accent),
                    )
                    .width(portion)
                    .padding(Padding::from([1, 8]))
                    .style(move |_| container::Style {
                        background: Some(Background::Color(c.accent)),
                        border: iced::border::rounded(6),
                        ..container::Style::default()
                    })
                    .into(),
                }
            },
        ))
        .spacing(2)
        .into()
    }

    /// The week as the calendars of the mail draw it: the days side by
    /// side under their number, the hours on the left, each event as tall
    /// as it lasts, the ones at the same time side by side.
    fn week<'a>(&'a self, s: &'static Strings, c: Colors) -> Element<'a, Message> {
        let dates: Vec<Option<NaiveDate>> = self.days.iter().map(|d| day_of(&d.date)).collect();
        let (first, last) = self.hours_shown();
        let gutter = || space::horizontal().width(Length::Fixed(GUTTER));
        let heads = Row::with_children(
            std::iter::once(gutter().into())
                .chain(dates.iter().map(|date| self.day_head(*date, s, c))),
        )
        .spacing(1);
        // the sheet of a microcycle is the band itself
        let band: Element<'a, Message> = match self.window {
            Some(_) => space::vertical().height(0).into(),
            None => row![gutter(), self.band(&dates, s, c)].into(),
        };
        let labels = Column::with_children((first..last).map(|hour| {
            container(text(format!("{hour}:00")).size(SIZE_SMALL).color(c.muted))
                .width(Length::Fixed(GUTTER))
                .height(Length::Fixed(HOUR))
                .padding(Padding::ZERO.right(8))
                .align_x(Alignment::End)
                .into()
        }));
        // the lines between the hours and the days: the gaps between the
        // cells, over the color of the borders
        let places = week_layout(&self.days).days;
        let columns = container(
            Row::with_children(
                self.days
                    .iter()
                    .zip(places)
                    .map(|(day, places)| self.day_column(day, places, first, last, s, c)),
            )
            .spacing(1),
        )
        .padding(1)
        .style(move |_| container::Style {
            background: Some(Background::Color(c.border)),
            ..container::Style::default()
        });
        column![
            heads,
            band,
            scrollable(row![labels, columns])
                .id(Id::new(WEEK))
                .height(Length::Fill)
        ]
        .spacing(6)
        .height(Length::Fill)
        .into()
    }

    /// The head of a day of the week: its number, large, and its name;
    /// today in the accent with a bar above, the day of the cursor lit.
    fn day_head<'a>(
        &'a self,
        date: Option<NaiveDate>,
        s: &'static Strings,
        c: Colors,
    ) -> Element<'a, Message> {
        let Some(date) = date else {
            return space::horizontal().width(Length::FillPortion(1)).into();
        };
        let lit = date == self.today;
        let color = match lit {
            true => c.accent,
            false => c.text,
        };
        let bar = container(space::vertical())
            .width(Length::Fill)
            .height(Length::Fixed(3.0))
            .style(move |_| container::Style {
                background: lit.then_some(Background::Color(c.accent)),
                ..container::Style::default()
            });
        let head = column![
            text(date.day().to_string())
                .size(SIZE_TITLE + 6.0)
                .font(theme::fonts().bold)
                .color(color),
            text(s.weekday(date)).size(SIZE_SMALL).color(match lit {
                true => c.accent,
                false => c.muted,
            }),
        ]
        .spacing(0);
        container(column![
            bar,
            ui::list_row(head, date == self.anchor, Some(Message::Day(date)), c)
        ])
        .width(Length::FillPortion(1))
        .into()
    }

    /// A day of the week: an hour a cell (a click opens a new session at
    /// its half hour), today
    /// on a background of its own; over them the events, where their hours
    /// are.
    fn day_column<'a>(
        &'a self,
        day: &'a AgendaDay,
        places: Vec<EventPlace>,
        first: u32,
        last: u32,
        s: &'static Strings,
        c: Colors,
    ) -> Element<'a, Message> {
        let date = day_of(&day.date);
        let background = match date == Some(self.today) {
            true => mix(c.panel, c.accent, 0.07),
            false => c.panel,
        };
        // half an hour a button: a click opens a new session there
        let half = |minute: u32| -> Element<'a, Message> {
            iced::widget::button(space::vertical())
                .width(Length::Fill)
                .height(Length::Fixed((HOUR - 1.0) / 2.0))
                .padding(0)
                .on_press_maybe(date.map(|d| Message::NewAt(d, Some(minute))))
                .style(move |_, status| iced::widget::button::Style {
                    background: Some(Background::Color(match status {
                        iced::widget::button::Status::Hovered => mix(background, c.accent, 0.12),
                        _ => background,
                    })),
                    ..iced::widget::button::Style::default()
                })
                .into()
        };
        let grid = Column::with_children(
            (first..last).map(|hour| column![half(hour * 60), half(hour * 60 + 30)].into()),
        )
        .spacing(1);
        let here = date == Some(self.anchor);
        let layers = day.events.iter().zip(places).enumerate().map(
            |(i, (event, place))| -> Element<'a, Message> {
                let lane = place.lane as usize;
                let count = place.lanes as usize;
                let top = (place.start.saturating_sub(first * 60) as f32 / 60.0 * HOUR).max(0.0);
                let tall = (f32::from(event.minutes) / 60.0 * HOUR).max(MIN_EVENT);
                let card = event_card(
                    event,
                    s,
                    c,
                    Look {
                        compact: tall < TWO_LINES,
                        with_team: self.all_teams,
                        chosen: here && i == self.slot,
                    },
                    date.map(|d| Message::Event(d, i)),
                    self.mark_of(event),
                    tall,
                );
                let portion = |n: usize| Length::FillPortion(u16::try_from(n).unwrap_or(1));
                let mut parts: Vec<Element<'a, Message>> = vec![];
                if lane > 0 {
                    parts.push(space::horizontal().width(portion(lane)).into());
                }
                parts.push(
                    column![space::vertical().height(Length::Fixed(top)), card]
                        .width(portion(1))
                        .into(),
                );
                if count > lane + 1 {
                    parts.push(space::horizontal().width(portion(count - lane - 1)).into());
                }
                Row::with_children(parts)
                    .spacing(2)
                    .padding(Padding::from([0, 2]))
                    .into()
            },
        );
        iced::widget::Stack::with_children(std::iter::once(grid.into()).chain(layers))
            .width(Length::FillPortion(1))
            .into()
    }

    /// The month as the calendars of the mail draw it: a box a day, its
    /// number at the top (today in a circle of the accent), its first
    /// events below, the days of the months around in grey.
    fn month<'a>(&'a self, s: &'static Strings, c: Colors) -> Element<'a, Message> {
        let names = Row::with_children(s.weekdays.iter().map(|n| {
            text(n.to_string())
                .size(SIZE_SMALL)
                .color(c.muted)
                .width(Length::FillPortion(1))
                .into()
        }))
        .spacing(1)
        .padding(Padding::from([0, 8]));
        let weeks = Column::with_children(month_weeks(self.anchor).into_iter().map(|week| {
            let cells = Row::with_children(
                whole_week(&week)
                    .into_iter()
                    .map(|day| self.cell(day, week.contains(&Some(day)), s, c)),
            )
            .spacing(1);
            column![self.band(&week, s, c), cells].spacing(2).into()
        }))
        .spacing(1);
        column![names, weeks].spacing(6).into()
    }

    /// A day of the month: the number and the first events, the others
    /// counted; a day of the months around only its number, in grey.
    fn cell<'a>(
        &'a self,
        day: NaiveDate,
        in_month: bool,
        s: &'static Strings,
        c: Colors,
    ) -> Element<'a, Message> {
        let events: &[AgendaEventRecord] = match in_month {
            true => self
                .day_record(day)
                .map(|d| d.events.as_slice())
                .unwrap_or_default(),
            false => &[],
        };
        let number = text(match (day.day(), in_month) {
            (1, _) | (_, false) => format!(
                "{} {}",
                day.day(),
                s.months
                    .get(day.month0() as usize)
                    .and_then(|m| m.get(..3).or(Some(m)))
                    .unwrap_or_default()
            ),
            (d, true) => d.to_string(),
        })
        .size(SIZE_SMALL)
        .font(theme::fonts().bold);
        let number: Element<'a, Message> = match (day == self.today, in_month) {
            (true, _) => container(number.color(c.on_accent))
                .padding(Padding::from([1, 7]))
                .style(move |_| container::Style {
                    background: Some(Background::Color(c.accent)),
                    border: iced::border::rounded(10),
                    ..container::Style::default()
                })
                .into(),
            (false, true) => number.color(c.text).into(),
            (false, false) => number.color(c.muted).into(),
        };
        let mut lines: Vec<Element<'a, Message>> = vec![number];
        lines.extend(events.iter().take(CELL_EVENTS).enumerate().map(|(i, e)| {
            event_card(
                e,
                s,
                c,
                Look {
                    compact: true,
                    with_team: self.all_teams,
                    chosen: false,
                },
                Some(Message::Event(day, i)),
                self.mark_of(e),
                MONTH_EVENT,
            )
        }));
        if events.len() > CELL_EVENTS {
            lines.push(
                text(format!("+{}", events.len() - CELL_EVENTS))
                    .size(SIZE_SMALL)
                    .color(c.muted)
                    .into(),
            );
        }
        let chosen = day == self.anchor;
        let background = match in_month {
            true => c.panel,
            false => c.background,
        };
        iced::widget::button(Column::with_children(lines).spacing(2))
            .width(Length::FillPortion(1))
            .height(Length::Fixed(CELL_HEIGHT))
            .padding(Padding::from([4, 6]))
            .on_press(Message::NewAt(day, None))
            .style(move |_, _| iced::widget::button::Style {
                background: Some(Background::Color(background)),
                text_color: c.text,
                border: match chosen {
                    true => iced::border::color(c.accent).width(2),
                    false => iced::border::color(c.border).width(1),
                },
                ..iced::widget::button::Style::default()
            })
            .into()
    }
}

/// The height of an hour in the week.
const HOUR: f32 = 44.0;

/// The width of the hours on the left of the week.
const GUTTER: f32 = 52.0;

/// The least height of an event of the week: a short one still readable.
const MIN_EVENT: f32 = 22.0;

/// The height from which an event of the week has room for its details.
const TWO_LINES: f32 = 40.0;

/// The height of an event in a day of the month.
const MONTH_EVENT: f32 = 20.0;

/// The height of a day of the month.
const CELL_HEIGHT: f32 = 112.0;

/// The width of the place of the mark beside an event of the list.
const LIST_MARK: f32 = 30.0;
/// The size of that mark: as large as the rows ask.
const LIST_MARK_SIZE: f32 = 22.0;

/// The width of the date on the left of the list.
const LIST_DATE: f32 = 96.0;

/// How an event is drawn.
struct Look {
    /// one line: the details left out
    compact: bool,
    /// the name of its team (the calendar of all the teams)
    with_team: bool,
    /// the one of the cursor
    chosen: bool,
}

/// The date of a day of the list: a click chooses it, the one of the
/// cursor lit.
fn button_day<'a>(
    content: impl Into<Element<'a, Message>>,
    chosen: bool,
    message: Message,
    c: Colors,
) -> Element<'a, Message> {
    container(ui::list_row(content, chosen, Some(message), c))
        .width(Length::Fixed(LIST_DATE))
        .into()
}

/// The box of a free session in its event: a click marks it (the event
/// itself opens the session).
fn mark_button<'a>(marked: bool, message: Message, size: f32, c: Colors) -> Element<'a, Message> {
    iced::widget::button(
        text(match marked {
            true => "☑",
            false => "☐",
        })
        .size(size),
    )
    .padding(Padding::from([0, 2]))
    .on_press(message)
    .style(move |_, status| iced::widget::button::Style {
        text_color: match (marked, status) {
            (true, _)
            | (
                false,
                iced::widget::button::Status::Hovered | iced::widget::button::Status::Pressed,
            ) => c.accent,
            (false, _) => c.muted,
        },
        ..iced::widget::button::Style::default()
    })
    .into()
}

/// `a` with a part `amount` of `b`: a background lit, opaque.
fn mix(a: iced::Color, b: iced::Color, amount: f32) -> iced::Color {
    iced::Color {
        r: a.r + (b.r - a.r) * amount,
        g: a.g + (b.g - a.g) * amount,
        b: a.b + (b.b - a.b) * amount,
        a: 1.0,
    }
}

/// The seven days of a row of the month, the ones of the months around
/// too.
fn whole_week(week: &[Option<NaiveDate>; 7]) -> [NaiveDate; 7] {
    let monday = week
        .iter()
        .enumerate()
        .find_map(|(i, d)| d.map(|d| d - Duration::days(i as i64)))
        .unwrap_or_default();
    std::array::from_fn(|i| monday + Duration::days(i as i64))
}

/// An event as a box: a bar of its color on the left (a session and a
/// match apart by it and by the icon), on a light background of it; the
/// one of the cursor with a border of the accent.
#[allow(clippy::too_many_arguments)]
fn event_card<'a>(
    event: &AgendaEventRecord,
    s: &'static Strings,
    c: Colors,
    look: Look,
    message: Option<Message>,
    mark: Option<(bool, Message)>,
    height: f32,
) -> Element<'a, Message> {
    let chosen = look.chosen;
    let color = match event.item {
        AgendaItemRecord::Session { .. } => c.accent,
        AgendaItemRecord::Match { .. } => c.break_point,
    };
    let stripe = container(space::vertical())
        .width(Length::Fixed(3.0))
        .height(Length::Fill)
        .style(move |_| container::Style {
            background: Some(Background::Color(color)),
            ..container::Style::default()
        });
    let body = container(event_text(event, s, c, &look))
        .padding(Padding::from([2, 6]))
        .width(Length::Fill)
        .clip(true);
    let inside: Element<'a, Message> = match mark {
        Some((marked, toggle)) => row![
            stripe,
            container(mark_button(marked, toggle, SIZE_SMALL, c))
                .padding(Padding::ZERO.left(2))
                .height(Length::Fill)
                .align_y(Alignment::Center),
            body
        ]
        .height(Length::Fill)
        .into(),
        None => row![stripe, body].height(Length::Fill).into(),
    };
    iced::widget::button(inside)
        .width(Length::Fill)
        .height(Length::Fixed(height))
        .padding(0)
        .on_press_maybe(message)
        .style(move |_, status| {
            let hovered = matches!(
                status,
                iced::widget::button::Status::Hovered | iced::widget::button::Status::Pressed
            );
            iced::widget::button::Style {
                background: Some(Background::Color(mix(
                    c.panel,
                    color,
                    match hovered {
                        true => 0.32,
                        false => 0.2,
                    },
                ))),
                text_color: c.text,
                border: match chosen {
                    true => iced::border::color(c.accent).width(2).rounded(4),
                    false => iced::border::rounded(4),
                },
                ..iced::widget::button::Style::default()
            }
        })
        .into()
}

/// An event as a line: the icon (a session and a match apart by it and by
/// its color), the time, the title; the week adds the place and the score.
fn event_text<'a>(
    event: &AgendaEventRecord,
    s: &'static Strings,
    c: Colors,
    look: &Look,
) -> Element<'a, Message> {
    let (compact, with_team) = (look.compact, look.with_team);
    let time = event.start.get(11..16).unwrap_or_default().to_string();
    let (icon, color, title, detail) = match &event.item {
        AgendaItemRecord::Session {
            title,
            status,
            to_close,
            ..
        } => {
            let mut flags = vec![];
            if *status == SessionStatus::Cancelled {
                flags.push(s.cancelled.to_string());
            }
            if *to_close {
                flags.push(s.to_close.to_string());
            }
            (
                theme::icons::CALENDAR,
                c.accent,
                title.clone(),
                [event
                    .venue
                    .as_ref()
                    .map(|v| crate::venue_choice::venue_label(s, &v.name, v.deleted))
                    .unwrap_or_default()]
                .into_iter()
                .chain(flags)
                .filter(|d| !d.is_empty())
                .collect::<Vec<_>>(),
            )
        }
        AgendaItemRecord::Match {
            opponent,
            ours,
            theirs,
            ..
        } => (
            theme::icons::VOLLEYBALL,
            c.break_point,
            format!("– {opponent}"),
            match (ours, theirs) {
                (Some(o), Some(t)) => vec![format!("{o}–{t}")],
                _ => vec![],
            },
        ),
    };
    let (cancelled, to_close) = match &event.item {
        AgendaItemRecord::Session {
            status, to_close, ..
        } => (*status == SessionStatus::Cancelled, *to_close),
        AgendaItemRecord::Match { .. } => (false, false),
    };
    let marks = match (compact, cancelled, to_close) {
        (false, ..) => String::new(),
        (true, true, true) => " × !".to_string(),
        (true, true, false) => " ×".to_string(),
        (true, false, true) => " !".to_string(),
        (true, false, false) => String::new(),
    };
    let title = crate::agenda::heading(event, title, with_team);
    // another team holds the gym then: the error color and an icon
    let shared = event.shared_venue && !cancelled;
    // one line clipped, not wrapped under where it can't be seen
    let first = text(format!("{time} {title}{marks}"))
        .wrapping(iced::widget::text::Wrapping::None)
        .size(SIZE_SMALL)
        .font(theme::fonts().bold)
        .color(match (cancelled, shared) {
            (true, _) => c.muted,
            (false, true) => c.danger,
            (false, false) => c.text,
        });
    let lines: Element<'a, Message> = match compact {
        true => first.into(),
        false => column![
            first,
            text(detail.join(" · ")).size(SIZE_SMALL).color(c.muted)
        ]
        .spacing(1)
        .into(),
    };
    let mut line = row![ui::icon(icon, 14.0, color), lines]
        .spacing(5)
        .align_y(Alignment::Center);
    if shared {
        line = line.push(ui::tip(
            ui::icon(theme::icons::WARNING, 14.0, c.danger),
            s.venue_busy_mark,
            c,
        ));
    }
    line.into()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use {
        super::*, crate::strings::strings, scout4all_core::shapes::enums::LanguageEnum,
        scout4all_mobile::venues::VenueDraft,
    };

    /// A row of the month has all its days, the ones around too.
    #[test]
    fn a_row_of_the_month_has_its_seven_days() {
        let day = |m: u32, d: u32| NaiveDate::from_ymd_opt(2026, m, d).unwrap();
        let first = month_weeks(day(11, 1)).into_iter().next().unwrap();
        assert_eq!(whole_week(&first)[0], day(10, 26));
        assert_eq!(whole_week(&first)[6], day(11, 1));
    }

    fn app(dir: &tempfile::TempDir) -> Arc<Scout4All> {
        Scout4All::for_tests(dir.path().to_string_lossy().into_owned(), "it".into()).unwrap()
    }

    fn session(core: &Scout4All, team: &str, start: &str) -> String {
        let draft = scout4all_mobile::training::SessionDraft {
            start: Some(start.to_string()),
            duration: "90".into(),
            ..core.new_session_draft()
        };
        core.save_session(team.to_string(), None, draft).unwrap().id
    }

    fn day(text: &str) -> NaiveDate {
        text.parse().unwrap()
    }

    /// The filter hides what it leaves out in the week, the month and the
    /// list alike; the list has only the days with something, also once
    /// the filter empties some.
    #[test]
    fn the_filter_hides_in_every_view_and_the_list_skips_empty_days() {
        let dir = tempfile::tempdir().unwrap();
        let core = app(&dir);
        let team = core.tutorial_team().unwrap();
        let planned = session(&core, &team, "2026-03-10T18:00");
        let cancelled = session(&core, &team, "2026-03-12T18:00");
        core.set_session_status(cancelled.clone(), SessionStatus::Cancelled)
            .unwrap();
        let mut tab = CalendarTab::at(core.clone(), team, day("2026-03-11"));
        assert_eq!(
            tab.shown_sessions(),
            vec![planned.clone(), cancelled.clone()]
        );
        tab.update(Message::Filter(Filter::Cancelled));
        assert_eq!(tab.shown_sessions(), vec![cancelled.clone()]);
        // the week keeps its seven days, the events hidden
        assert_eq!(tab.days().len(), 7);
        tab.key(key!(m));
        assert_eq!(tab.shown_sessions(), vec![cancelled.clone()]);
        assert_eq!(tab.days().len(), 31);
        tab.key(key!(l));
        assert_eq!(tab.shown_sessions(), vec![cancelled.clone()]);
        let dates: Vec<String> = tab.days().iter().map(|d| d.date.clone()).collect();
        assert_eq!(dates, vec!["2026-03-12".to_string()]);
        tab.update(Message::Filter(Filter::All));
        let dates: Vec<String> = tab.days().iter().map(|d| d.date.clone()).collect();
        assert_eq!(
            dates,
            vec!["2026-03-10".to_string(), "2026-03-12".to_string()]
        );
        assert!(tab.days().iter().all(|d| !d.events.is_empty()));
        // in the list ↑ ↓ go from event to event across the days
        assert_eq!(tab.anchor(), day("2026-03-12"));
        tab.key(key!(up));
        assert_eq!(tab.anchor(), day("2026-03-10"));
        assert_eq!(tab.key(key!(enter)), Leave::Session(planned));
    }

    /// The marks stay changing view and period; only free sessions are
    /// marked; `c` hands them over and clears them; ⌫ asks to delete the
    /// session chosen.
    #[test]
    fn the_marks_stay_across_views_and_periods() {
        let dir = tempfile::tempdir().unwrap();
        let core = app(&dir);
        let team = core.tutorial_team().unwrap();
        let first = session(&core, &team, "2026-03-10T18:00");
        let second = session(&core, &team, "2026-04-14T18:00");
        let mut tab = CalendarTab::at(core.clone(), team, day("2026-03-10"));
        tab.key(key!(space));
        assert_eq!(tab.marked_count(), 1);
        tab.key(key!(m));
        tab.key(key!(pagedown));
        tab.update(Message::Mark(second.clone()));
        tab.key(key!(l));
        tab.key(key!(p));
        assert_eq!(tab.marked_count(), 2);
        // something not free is not marked
        tab.update(Message::Mark("nessuna".into()));
        assert_eq!(tab.marked_count(), 2);
        tab.key(key!(w));
        let mut both = vec![first.clone(), second];
        both.sort();
        assert_eq!(tab.key(key!(c)), Leave::Create(both));
        assert_eq!(tab.marked_count(), 0);
        assert_eq!(tab.key(key!(c)), Leave::Stay);
        tab.key(key!(home));
        assert_eq!(tab.key(key!(backspace)), Leave::Delete(first));
    }

    /// The week by the hour scrolls to an hour before its first event once
    /// shown: not again while it stays, again on another week or back from
    /// the month; nothing for a week without events.
    #[test]
    fn the_week_scrolls_to_an_hour_before_its_first_event_once() {
        let dir = tempfile::tempdir().unwrap();
        let core = app(&dir);
        let team = core.tutorial_team().unwrap();
        session(&core, &team, "2026-03-10T18:00");
        session(&core, &team, "2026-03-12T16:30");
        let mut tab = CalendarTab::at(core.clone(), team, day("2026-03-11"));
        let (first, _) = tab.hours_shown();
        let at_15 = (15 - first) as f32 * HOUR;
        assert_eq!(tab.take_week_scroll(), Some((Id::new(WEEK), at_15)));
        assert_eq!(tab.take_week_scroll(), None);
        tab.key(key!(right));
        assert_eq!(tab.take_week_scroll(), None);
        tab.key(key!(m));
        assert_eq!(tab.take_week_scroll(), None);
        // the month keeps the week of the 12th in sight: the third of six
        assert_eq!(tab.scroll(), Some((Id::new(MONTH), 0.4)));
        tab.key(key!(w));
        assert_eq!(tab.take_week_scroll(), Some((Id::new(WEEK), at_15)));
        // a week without events stays at its first hour
        tab.key(key!(pagedown));
        assert_eq!(tab.take_week_scroll(), None);
    }

    /// What is to close is in the past: in the list the filter looks at
    /// any date, whatever the period was.
    #[test]
    fn to_close_widens_the_period_of_the_list() {
        let dir = tempfile::tempdir().unwrap();
        let core = app(&dir);
        let team = core.tutorial_team().unwrap();
        let old = session(&core, &team, "2026-01-05T18:00");
        let mut tab = CalendarTab::at(core.clone(), team, day("2026-03-11"));
        tab.key(key!(l));
        tab.set_period(Period::Month);
        assert!(tab.shown_sessions().is_empty());
        tab.update(Message::Filter(Filter::ToClose));
        assert_eq!(tab.period_shown(), Period::Any);
        assert_eq!(tab.shown_sessions(), vec![old]);
    }

    fn offers_venues(tab: &CalendarTab) -> bool {
        let s = strings(LanguageEnum::It);
        tab.hints(s).iter().any(|(k, _)| k == "v")
    }

    /// Without gyms there is no filter by gym (a button that did nothing);
    /// with one, `v` goes from all the gyms to it and back; a gym removed
    /// meanwhile filters no more.
    #[test]
    fn the_gym_filter_is_there_only_with_gyms() {
        let dir = tempfile::tempdir().unwrap();
        let core =
            Scout4All::for_tests(dir.path().to_string_lossy().into_owned(), "it".into()).unwrap();
        let team = core.tutorial_team().unwrap();
        let mut tab = CalendarTab::new(core.clone(), team);
        assert!(!offers_venues(&tab));
        tab.key(key!(v));
        assert!(tab.venue.is_none());
        let gym = core
            .save_venue(
                None,
                VenueDraft {
                    name: "Terontola".into(),
                    ..VenueDraft::default()
                },
            )
            .unwrap()
            .id;
        tab.reload();
        assert!(offers_venues(&tab));
        tab.key(key!(v));
        assert_eq!(tab.venue.as_ref().map(|(id, _)| id), Some(&gym));
        tab.key(key!(v));
        assert!(tab.venue.is_none());
        tab.key(key!(v));
        core.remove_venue(gym).unwrap();
        tab.reload();
        assert!(tab.venue.is_none());
        assert!(!offers_venues(&tab));
    }
}
