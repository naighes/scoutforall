//! The gyms (TODO/021 §13), from the home: the list with the pinned ones
//! first, and the sheet of a gym with the week of what holds it, every team's
//! sessions and matches, the overlaps marked. ↑ ↓ in the list, Enter opens
//! the sheet, `n` makes a gym, `e` edits it, `p` pins it, `o` opens it in the
//! map, ⌫ deletes it (sessions and matches keep it). In the sheet ‹ › (or
//! PgUp PgDn) change the week, Home goes back to this one.

use scout4all_mobile::clock::ClockExt;
use {
    crate::{
        calendar::{day_of, text_of, time_span},
        form::{self, Field, Form, Outcome as FormOutcome, Value},
        strings::Strings,
        theme::{self, Colors, SIZE_BODY, SIZE_SMALL},
        ui,
    },
    chrono::{Duration, NaiveDate},
    iced::{
        Alignment, Element, Length,
        widget::{Column, Id, column, container, row, scrollable, space, text},
    },
    scout4all_keys::{BACK_KEY, KeyCombination, key},
    scout4all_mobile::{
        MobileError, Scout4All,
        training::{AgendaSpan, TrainingField},
        venues::{VenueDay, VenueDraft, VenueRecord},
    },
    std::sync::Arc,
};

const NAME: usize = 0;
const ADDRESS: usize = 1;
const LINK: usize = 2;
const LATITUDE: usize = 3;
const LONGITUDE: usize = 4;
const NOTES: usize = 5;
const PINNED: usize = 6;

/// The form of a gym, new or saved.
pub struct VenueEditor {
    form: Form,
    id: Option<String>,
    /// the coordinates the link filled in, to follow it while it's written
    /// and to leave alone the ones written by hand
    filled: Option<(String, String)>,
}

impl VenueEditor {
    fn new(core: &Scout4All, s: &Strings, id: Option<String>) -> Result<VenueEditor, MobileError> {
        let draft = match &id {
            Some(id) => core.venue_draft(id.clone())?,
            None => core.new_venue_draft(),
        };
        let fields = vec![
            Field::text(s.venue_name, draft.name),
            Field::text(s.venue_address, draft.address),
            Field::text(s.venue_map_link, draft.map_url),
            Field::text(s.venue_latitude, draft.latitude),
            Field::text(s.venue_longitude, draft.longitude),
            Field::lines(s.notes, draft.notes),
            Field::check(s.venue_pinned, draft.pinned, false),
        ];
        Ok(VenueEditor {
            form: Form::new(
                match id {
                    Some(_) => s.edit_venue,
                    None => s.new_venue,
                },
                fields,
            ),
            id,
            filled: None,
        })
    }

    /// The link was written: the coordinates it gives fill the two fields
    /// when they are empty or hold the ones an earlier link filled; the ones
    /// written by hand stay.
    fn follow_link(&mut self, core: &Scout4All) {
        let current = (self.form.text(LATITUDE), self.form.text(LONGITUDE));
        let free = (current.0.trim().is_empty() && current.1.trim().is_empty())
            || self.filled.as_ref() == Some(&current);
        if !free {
            return;
        }
        let found = core
            .venue_link_coordinates(self.form.text(LINK))
            .ok()
            .flatten()
            .map(|c| (c.latitude.to_string(), c.longitude.to_string()));
        let (lat, lon) = found.clone().unwrap_or_default();
        for (at, value) in [(LATITUDE, lat), (LONGITUDE, lon)] {
            if let Some(Value::Text(text)) = self.form.fields.get_mut(at).map(|f| &mut f.value) {
                *text = value;
            }
        }
        self.filled = found;
    }

    fn draft(&self) -> VenueDraft {
        VenueDraft {
            name: self.form.text(NAME),
            address: self.form.text(ADDRESS),
            map_url: self.form.text(LINK),
            latitude: self.form.text(LATITUDE),
            longitude: self.form.text(LONGITUDE),
            notes: self.form.text(NOTES),
            pinned: matches!(
                self.form.fields.get(PINNED).map(|f| &f.value),
                Some(Value::Check { on: true, .. })
            ),
        }
    }

    /// Saves it; the coordinates the link gives fill the two fields when they
    /// are empty (the ones written win).
    fn save(&mut self, core: &Scout4All) -> Option<String> {
        let mut draft = self.draft();
        if draft.latitude.trim().is_empty() && draft.longitude.trim().is_empty() {
            match core.venue_link_coordinates(draft.map_url.clone()) {
                Ok(Some(found)) => {
                    draft.latitude = found.latitude.to_string();
                    draft.longitude = found.longitude.to_string();
                }
                Ok(None) => {}
                Err(e) => {
                    self.form.focused = LINK;
                    self.form.fail(e.to_string());
                    return None;
                }
            }
        }
        match core.save_venue(self.id.clone(), draft) {
            Ok(saved) => Some(saved.id),
            Err(e) => {
                self.form.focused = match &e {
                    MobileError::InvalidTraining { field, .. } => match field {
                        TrainingField::Coordinates => LATITUDE,
                        TrainingField::MapLink => LINK,
                        _ => NAME,
                    },
                    _ => NAME,
                };
                self.form.fail(e.to_string());
                None
            }
        }
    }
}

enum Mode {
    Browse,
    /// the sheet of the gym at the position of the list, with the week
    Sheet,
    Edit(Box<VenueEditor>),
    /// the gym of the list to delete, once confirmed
    Delete(usize),
}

#[derive(Debug, Clone)]
pub enum Message {
    Open(usize),
    Edit(usize),
    Delete(usize),
    Pin(usize),
    Map(usize),
    New,
    Form(form::Message),
    Confirm,
    Cancel,
    Previous,
    Next,
    Today,
    /// the list left, or the sheet of a gym back to the list
    Close,
}

pub enum Outcome {
    Stay,
    Back,
}

pub struct VenuesScreen {
    core: Arc<Scout4All>,
    /// the team whose training opened the gyms, to go back to
    pub back_to: Option<String>,
    listed: Vec<VenueRecord>,
    selected: usize,
    mode: Mode,
    /// a day of the week the sheet shows
    anchor: NaiveDate,
    week: Vec<VenueDay>,
    error: Option<String>,
}

impl VenuesScreen {
    /// What went wrong doing something: the app tells it over the screen.
    pub fn failure(&self) -> &Option<String> {
        &self.error
    }

    pub fn failure_mut(&mut self) -> &mut Option<String> {
        &mut self.error
    }

    pub fn new(core: Arc<Scout4All>) -> Result<VenuesScreen, MobileError> {
        let listed = core.venues()?;
        let today = core.clock().today();
        Ok(VenuesScreen {
            core,
            back_to: None,
            listed,
            selected: 0,
            mode: Mode::Browse,
            anchor: today,
            week: vec![],
            error: None,
        })
    }

    /// Read again after a change, the same row chosen (or the last).
    fn reload(&mut self) {
        match self.core.venues() {
            Ok(listed) => {
                self.listed = listed;
                self.selected = self.selected.min(self.listed.len().saturating_sub(1));
            }
            Err(e) => self.error = Some(e.to_string()),
        }
        self.load_week();
    }

    fn load_week(&mut self) {
        self.week = self
            .listed
            .get(self.selected)
            .and_then(|v| {
                self.core
                    .venue_agenda(v.id.clone(), AgendaSpan::Week, text_of(self.anchor))
                    .map_err(|e| self.error = Some(e.to_string()))
                    .ok()
            })
            .unwrap_or_default();
    }

    pub fn focus_id(&self) -> Option<Id> {
        match &self.mode {
            Mode::Edit(editor) => Some(editor.form.focus_id()),
            Mode::Browse | Mode::Sheet | Mode::Delete(_) => None,
        }
    }

    fn open(&mut self, index: usize) {
        if index < self.listed.len() {
            self.selected = index;
            self.mode = Mode::Sheet;
            self.load_week();
        }
    }

    fn edit(&mut self, s: &Strings, index: usize) {
        let Some(v) = self.listed.get(index) else {
            return;
        };
        self.selected = index;
        match VenueEditor::new(&self.core, s, Some(v.id.clone())) {
            Ok(editor) => self.mode = Mode::Edit(Box::new(editor)),
            Err(e) => self.error = Some(e.to_string()),
        }
    }

    fn create(&mut self, s: &Strings) {
        match VenueEditor::new(&self.core, s, None) {
            Ok(editor) => self.mode = Mode::Edit(Box::new(editor)),
            Err(e) => self.error = Some(e.to_string()),
        }
    }

    fn pin(&mut self, index: usize) {
        if let Some(v) = self.listed.get(index) {
            self.error = self
                .core
                .set_venue_pinned(v.id.clone(), !v.pinned)
                .err()
                .map(|e| e.to_string());
            // the pinned go first: the list is read again
            let id = v.id.clone();
            self.reload();
            self.selected = self.listed.iter().position(|v| v.id == id).unwrap_or(0);
            self.load_week();
        }
    }

    fn map(&mut self, s: &Strings, index: usize) {
        let Some(v) = self.listed.get(index) else {
            return;
        };
        self.error = match self.core.venue_web_url(v.id.clone()) {
            Ok(Some(url)) => open::that_detached(url)
                .err()
                .map(|_| s.no_app_to_open.to_string()),
            Ok(None) => None,
            Err(e) => Some(e.to_string()),
        };
    }

    fn ask_deletion(&mut self, index: usize) {
        if index < self.listed.len() {
            self.selected = index;
            self.mode = Mode::Delete(index);
        }
    }

    fn delete(&mut self) {
        let mode = std::mem::replace(&mut self.mode, Mode::Browse);
        if let Some(v) = match mode {
            Mode::Delete(index) => self.listed.get(index),
            _ => None,
        } {
            self.error = self
                .core
                .remove_venue(v.id.clone())
                .err()
                .map(|e| e.to_string());
        }
        self.reload();
    }

    fn edited(&mut self, saved: Option<String>) {
        if let Some(id) = saved {
            self.mode = Mode::Browse;
            self.error = None;
            self.reload();
            self.selected = self.listed.iter().position(|v| v.id == id).unwrap_or(0);
            self.load_week();
        }
    }

    fn form_outcome(&mut self, outcome: FormOutcome) {
        let Mode::Edit(editor) = &mut self.mode else {
            return;
        };
        match outcome {
            FormOutcome::Cancel => self.mode = Mode::Browse,
            FormOutcome::Save => {
                let saved = editor.save(&self.core);
                self.edited(saved);
            }
            FormOutcome::Changed(LINK) => editor.follow_link(&self.core),
            FormOutcome::Stay | FormOutcome::Changed(_) => {}
        }
    }

    fn shift(&mut self, weeks: i64) {
        self.anchor += Duration::weeks(weeks);
        self.load_week();
    }

    fn go_today(&mut self) {
        self.anchor = self.core.clock().today();
        self.load_week();
    }

    pub fn update(&mut self, s: &Strings, message: Message) -> Outcome {
        match (message, &mut self.mode) {
            (Message::Form(m), Mode::Edit(editor)) => {
                let outcome = editor.form.update(m);
                self.form_outcome(outcome);
            }
            (Message::Confirm, Mode::Delete(_)) => self.delete(),
            (Message::Cancel, Mode::Sheet) => self.mode = Mode::Browse,
            (Message::Cancel, _) => self.mode = Mode::Browse,
            (Message::Open(i), Mode::Browse) => self.open(i),
            (Message::Edit(i), Mode::Browse | Mode::Sheet) => self.edit(s, i),
            (Message::Delete(i), Mode::Browse | Mode::Sheet) => self.ask_deletion(i),
            (Message::Pin(i), Mode::Browse | Mode::Sheet) => self.pin(i),
            (Message::Map(i), Mode::Browse | Mode::Sheet) => self.map(s, i),
            (Message::New, Mode::Browse) => self.create(s),
            (Message::Previous, Mode::Sheet) => self.shift(-1),
            (Message::Next, Mode::Sheet) => self.shift(1),
            (Message::Today, Mode::Sheet) => self.go_today(),
            (Message::Close, Mode::Sheet) => self.mode = Mode::Browse,
            (Message::Close, Mode::Browse) => return Outcome::Back,
            (_, _) => {}
        }
        Outcome::Stay
    }

    pub fn key(&mut self, s: &Strings, ck: KeyCombination) -> Outcome {
        if let Mode::Edit(editor) = &mut self.mode {
            let outcome = editor.form.key(ck);
            self.form_outcome(outcome);
            return Outcome::Stay;
        }
        if let Mode::Delete(_) = self.mode {
            match ck {
                k if k == key!(enter) => self.delete(),
                k if k == BACK_KEY => self.mode = Mode::Browse,
                _ => {}
            }
            return Outcome::Stay;
        }
        let count = self.listed.len();
        let sheet = matches!(self.mode, Mode::Sheet);
        match ck {
            k if k == BACK_KEY && sheet => self.mode = Mode::Browse,
            k if k == BACK_KEY => return Outcome::Back,
            k if k == key!(down) && count > 0 => {
                self.selected = (self.selected + 1) % count;
                self.load_week();
            }
            k if k == key!(up) && count > 0 => {
                self.selected = self.selected.checked_sub(1).unwrap_or(count - 1);
                self.load_week();
            }
            k if k == key!(enter) && !sheet => self.open(self.selected),
            k if k == key!(n) && !sheet => self.create(s),
            k if k == key!(e) => self.edit(s, self.selected),
            k if k == key!(p) => self.pin(self.selected),
            k if k == key!(o) => self.map(s, self.selected),
            k if k == key!(backspace) || k == key!(delete) => self.ask_deletion(self.selected),
            k if sheet && (k == key!(pageup) || k == key!(left)) => self.shift(-1),
            k if sheet && (k == key!(pagedown) || k == key!(right)) => self.shift(1),
            k if sheet && k == key!(home) => self.go_today(),
            _ => {}
        }
        Outcome::Stay
    }

    pub fn hints(&self, s: &Strings) -> Vec<(String, String)> {
        // what `p` does to the gym chosen
        let pin = match self.listed.get(self.selected).is_some_and(|v| v.pinned) {
            true => s.unpin_venue,
            false => s.pin_venue,
        };
        match &self.mode {
            Mode::Edit(editor) => editor.form.on_field(s, Form::hints(s)),
            Mode::Delete(_) => vec![
                ("Enter".to_string(), s.delete_venue.to_string()),
                ("Esc".to_string(), s.cancel.to_string()),
            ],
            Mode::Sheet => vec![
                ("← →".to_string(), s.week.to_string()),
                ("Home".to_string(), s.today.to_string()),
                ("↑ ↓".to_string(), s.venues.to_string()),
                ("e".to_string(), s.edit.to_string()),
                ("p".to_string(), pin.to_string()),
                ("o".to_string(), s.open_in_map.to_string()),
                ("Esc".to_string(), s.back.to_string()),
            ],
            Mode::Browse => vec![
                ("↑ ↓".to_string(), String::new()),
                ("Enter".to_string(), s.open.to_string()),
                ("n".to_string(), s.new_venue.to_string()),
                ("e".to_string(), s.edit.to_string()),
                ("p".to_string(), pin.to_string()),
                ("o".to_string(), s.open_in_map.to_string()),
                ("⌫".to_string(), s.delete_venue.to_string()),
                ("Esc".to_string(), s.back.to_string()),
            ],
        }
    }

    pub fn view<'a>(&'a self, s: &'static Strings, c: Colors) -> Element<'a, Message> {
        let body: Element<'a, Message> = match &self.mode {
            Mode::Edit(editor) => editor.form.view(s, c, Message::Form, None),
            Mode::Delete(index) => ui::confirm_deletion_with(
                s,
                &ui::fill(
                    s.delete_venue_text,
                    &[self
                        .listed
                        .get(*index)
                        .map(|v| v.name.clone())
                        .unwrap_or_default()],
                ),
                vec![],
                s.delete_venue,
                Message::Confirm,
                Message::Cancel,
                c,
            ),
            Mode::Browse => self.list(s, c),
            Mode::Sheet => self.sheet(s, c),
        };
        body
    }

    /// How wide the list is at most.
    const WIDTH: f32 = 900.0;

    fn list<'a>(&'a self, s: &'static Strings, c: Colors) -> Element<'a, Message> {
        let rows = self.listed.iter().enumerate().map(|(i, v)| {
            row![
                ui::box_row(
                    ui::initials(v.initials.clone(), c),
                    row![
                        Column::new()
                            .push(
                                text(v.name.clone())
                                    .size(SIZE_BODY)
                                    .font(theme::fonts().bold)
                            )
                            // no empty line: the name stays in the middle
                            .push((!v.address.is_empty()).then(|| {
                                text(v.address.clone()).size(SIZE_SMALL).color(c.muted)
                            }))
                            .spacing(2)
                            .width(Length::Fill),
                        text(match v.pinned {
                            true => "★",
                            false => "",
                        })
                        .size(SIZE_BODY)
                        .color(c.accent),
                    ]
                    .spacing(10)
                    .align_y(Alignment::Center),
                    i == self.selected,
                    Message::Open(i),
                    c,
                ),
                ui::tip(
                    ui::icon_button(theme::icons::EDIT, s.edit_venue, Message::Edit(i), false, c),
                    s.edit_venue,
                    c,
                ),
                ui::tip(
                    ui::icon_button(
                        theme::icons::DELETE,
                        s.delete_venue,
                        Message::Delete(i),
                        false,
                        c,
                    ),
                    s.delete_venue,
                    c,
                ),
            ]
            .spacing(4)
            .align_y(Alignment::Center)
            .into()
        });
        let list: Element<'a, Message> =
            match self.listed.is_empty() {
                true => ui::body(s.no_venues).color(c.muted).into(),
                // the scrollbar beside the rows, not over their bins
                false => scrollable(Column::with_children(rows).spacing(4).padding(
                    iced::Padding {
                        right: 14.0,
                        ..iced::Padding::ZERO
                    },
                ))
                .height(Length::Fill)
                .into(),
            };
        container(
            container(ui::panel_with(
                s.venues,
                vec![
                    ui::pill(s.new_venue, false, Some(Message::New), c),
                    ui::pill(s.close, false, Some(Message::Close), c),
                ],
                container(list).height(Length::Fill),
                c,
            ))
            .max_width(Self::WIDTH),
        )
        .center_x(Length::Fill)
        .height(Length::Fill)
        .into()
    }

    /// The sheet of the gym chosen: what it is, and its week.
    fn sheet<'a>(&'a self, s: &'static Strings, c: Colors) -> Element<'a, Message> {
        let Some(venue) = self.listed.get(self.selected) else {
            return space::vertical().into();
        };
        let bold = theme::fonts().bold;
        let mut facts: Vec<Element<'a, Message>> = vec![];
        for line in [&venue.address, &venue.notes] {
            if !line.is_empty() {
                facts.push(text(line.clone()).size(SIZE_SMALL).color(c.muted).into());
            }
        }
        if let (Some(lat), Some(lon)) = (venue.latitude, venue.longitude) {
            facts.push(
                text(format!("{lat:.5}, {lon:.5}"))
                    .size(SIZE_SMALL)
                    .color(c.muted)
                    .into(),
            );
        }
        let today = self.core.clock().today();
        let days = self.week.iter().map(|day| {
            let date = day_of(&day.date);
            let lit = date == Some(today);
            let heading = date.map_or_else(String::new, |d| {
                format!("{} {}", s.weekday(d), s.short_date(d))
            });
            let items: Vec<Element<'a, Message>> = match day.occupations.is_empty() {
                true => vec![text("–").size(SIZE_SMALL).color(c.muted).into()],
                false => day
                    .occupations
                    .iter()
                    .map(|o| {
                        let teams = o
                            .teams
                            .iter()
                            .map(|t| t.name.clone())
                            .collect::<Vec<_>>()
                            .join(" – ");
                        let color = match o.overlapping {
                            true => c.danger,
                            false => c.text,
                        };
                        row![
                            ui::icon(
                                match o.is_match {
                                    true => theme::icons::VOLLEYBALL,
                                    false => theme::icons::CALENDAR,
                                },
                                16.0,
                                color
                            ),
                            text(format!("{} · {teams}", time_span(&o.start, o.minutes)))
                                .size(SIZE_BODY)
                                .color(color),
                            text(match o.overlapping {
                                true => s.venue_busy_mark,
                                false => "",
                            })
                            .size(SIZE_SMALL)
                            .color(c.danger),
                        ]
                        .spacing(8)
                        .align_y(Alignment::Center)
                        .into()
                    })
                    .collect(),
            };
            let line = row![
                text(heading)
                    .size(SIZE_BODY)
                    .font(bold)
                    .color(match lit {
                        true => c.accent,
                        false => c.text,
                    })
                    .width(Length::Fixed(90.0)),
                Column::with_children(items).spacing(2),
            ]
            .spacing(10);
            // today lit, as in the agendas
            match lit {
                true => crate::agenda::today_box(line, c),
                false => line.into(),
            }
        });
        let actions = vec![
            ui::pill(s.today, false, Some(Message::Today), c),
            ui::tip(
                ui::pill("‹", false, Some(Message::Previous), c),
                s.previous_week,
                c,
            ),
            ui::tip(ui::pill("›", false, Some(Message::Next), c), s.next_week, c),
            ui::pill(s.open_in_map, false, Some(Message::Map(self.selected)), c),
            ui::pill(
                match venue.pinned {
                    true => s.unpin_venue,
                    false => s.pin_venue,
                },
                venue.pinned,
                Some(Message::Pin(self.selected)),
                c,
            ),
            ui::pill(s.edit, false, Some(Message::Edit(self.selected)), c),
            ui::pill(s.close, false, Some(Message::Close), c),
        ];
        let content = column![
            Column::with_children(facts).spacing(2),
            scrollable(Column::with_children(days).spacing(8)).height(Length::Fill),
        ]
        .spacing(10);
        container(
            container(ui::panel_with(&venue.name, actions, content, c)).max_width(Self::WIDTH),
        )
        .center_x(Length::Fill)
        .height(Length::Fill)
        .into()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::unreachable)]
mod tests {
    use {
        super::*,
        crate::strings::strings,
        scout4all_core::{
            shapes::{
                country::CountryCode,
                enums::{GenderEnum, LanguageEnum, TeamClassificationEnum},
            },
            team_form::TeamForm,
        },
        scout4all_mobile::training::SessionDraft,
    };

    fn app(dir: &tempfile::TempDir) -> Arc<Scout4All> {
        Scout4All::for_tests(dir.path().to_string_lossy().into_owned(), "it".into()).unwrap()
    }

    #[test]
    fn a_failure_of_the_page_is_told_over_it() {
        let dir = tempfile::tempdir().unwrap();
        let core = scout4all_mobile::Scout4All::for_tests(
            dir.path().to_string_lossy().into_owned(),
            "en".into(),
        )
        .unwrap();
        let mut screen = VenuesScreen::new(core).unwrap();
        screen.error = Some("not saved".into());
        crate::assert_told_over(crate::Screen::Venues(Box::new(screen)), "not saved");
    }

    /// The week of a gym starts on the day of the clock of the core, and
    /// comes back to it: not the computer's.
    #[test]
    fn the_week_of_a_gym_is_on_the_clock_of_the_core() {
        let dir = tempfile::tempdir().unwrap();
        let day = NaiveDate::from_ymd_opt(2041, 3, 9).unwrap();
        let clock = scout4all_mobile::clock::FixedClock::at(
            day.and_hms_opt(0, 30, 0).unwrap(),
            chrono::FixedOffset::east_opt(3600).unwrap(),
        );
        let core = Scout4All::with_clock(
            dir.path().to_string_lossy().into_owned(),
            "it".into(),
            Arc::new(clock),
        )
        .unwrap();
        let mut screen = VenuesScreen::new(core).unwrap();
        assert_eq!(screen.anchor, day);
        screen.shift(2);
        screen.go_today();
        assert_eq!(screen.anchor, day);
    }

    fn venue(core: &Scout4All, name: &str) -> String {
        core.save_venue(
            None,
            VenueDraft {
                name: name.into(),
                ..VenueDraft::default()
            },
        )
        .unwrap()
        .id
    }

    /// `n` opens the form, a name makes the gym; `p` pins it (it goes first,
    /// on every device); Enter opens its sheet, Esc comes back; ⌫ asks and
    /// Enter deletes it.
    #[test]
    fn the_gyms_are_made_pinned_opened_and_deleted() {
        let s = strings(LanguageEnum::It);
        let dir = tempfile::tempdir().unwrap();
        let core = app(&dir);
        venue(&core, "Montecchio");
        let mut screen = VenuesScreen::new(core.clone()).unwrap();
        screen.key(s, key!(n));
        assert!(matches!(screen.mode, Mode::Edit(_)));
        // no name: told, nothing made
        screen.update(s, Message::Form(form::Message::Save));
        assert!(matches!(&screen.mode, Mode::Edit(e) if e.form.told().is_some()));
        screen.update(
            s,
            Message::Form(form::Message::Input(NAME, "Terontola".into())),
        );
        screen.update(s, Message::Form(form::Message::Save));
        assert!(matches!(screen.mode, Mode::Browse));
        let names = |screen: &VenuesScreen| -> Vec<String> {
            screen.listed.iter().map(|v| v.name.clone()).collect()
        };
        assert_eq!(names(&screen), vec!["Montecchio", "Terontola"]);
        assert_eq!(
            screen
                .listed
                .iter()
                .map(|v| v.initials.as_str())
                .collect::<Vec<_>>(),
            vec!["M", "T"]
        );
        // the key of the pin says what it does to the one chosen
        let pin_hint = |screen: &VenuesScreen| -> String {
            screen
                .hints(s)
                .into_iter()
                .find(|(k, _)| k == "p")
                .map(|(_, what)| what)
                .unwrap_or_default()
        };
        // the new one is on it; pinned, it goes first
        assert_eq!(screen.listed[screen.selected].name, "Terontola");
        assert_eq!(pin_hint(&screen), s.pin_venue);
        screen.key(s, key!(p));
        assert_eq!(pin_hint(&screen), s.unpin_venue);
        assert_eq!(names(&screen), vec!["Terontola", "Montecchio"]);
        assert_eq!(screen.listed[screen.selected].name, "Terontola");
        assert!(core.venues().unwrap()[0].pinned);
        // the sheet
        screen.key(s, key!(enter));
        assert!(matches!(screen.mode, Mode::Sheet));
        assert_eq!(screen.week.len(), 7);
        assert!(matches!(screen.key(s, BACK_KEY), Outcome::Stay));
        assert!(matches!(screen.mode, Mode::Browse));
        // Close: the sheet back to the list, the list left
        screen.key(s, key!(enter));
        assert!(matches!(screen.update(s, Message::Close), Outcome::Stay));
        assert!(matches!(screen.mode, Mode::Browse));
        // delete: asked first, then gone
        screen.key(s, key!(backspace));
        assert!(matches!(screen.mode, Mode::Delete(0)));
        screen.key(s, BACK_KEY);
        assert_eq!(names(&screen).len(), 2);
        screen.key(s, key!(backspace));
        screen.key(s, key!(enter));
        assert_eq!(names(&screen), vec!["Montecchio"]);
        assert!(matches!(screen.update(s, Message::Close), Outcome::Back));
        assert!(matches!(screen.key(s, BACK_KEY), Outcome::Back));
    }

    /// A link gives the coordinates when none are written; a short link is
    /// refused with its message; the coordinates written win.
    #[test]
    fn the_link_fills_the_coordinates() {
        let s = strings(LanguageEnum::It);
        let dir = tempfile::tempdir().unwrap();
        let core = app(&dir);
        let mut screen = VenuesScreen::new(core.clone()).unwrap();
        screen.key(s, key!(n));
        let form_of = |screen: &mut VenuesScreen, field: usize, text: &str| {
            screen.update(s, Message::Form(form::Message::Input(field, text.into())));
        };
        form_of(&mut screen, NAME, "Cortona");
        form_of(&mut screen, LINK, "https://maps.app.goo.gl/abc");
        screen.update(s, Message::Form(form::Message::Save));
        assert!(matches!(&screen.mode, Mode::Edit(e) if e.form.told().is_some()));
        form_of(
            &mut screen,
            LINK,
            "https://www.google.com/maps/place/Cortona/@43.2755,11.9856,17z",
        );
        screen.update(s, Message::Form(form::Message::Save));
        assert!(matches!(screen.mode, Mode::Browse));
        let saved = &core.venues().unwrap()[0];
        assert_eq!(saved.latitude, Some(43.2755));
        assert_eq!(saved.longitude, Some(11.9856));
    }

    /// The coordinates appear in the form as the link is written, follow
    /// a changed link, and leave alone the ones written by hand.
    #[test]
    fn the_coordinates_appear_as_the_link_is_written() {
        let s = strings(LanguageEnum::It);
        let dir = tempfile::tempdir().unwrap();
        let mut screen = VenuesScreen::new(app(&dir)).unwrap();
        screen.key(s, key!(n));
        let write = |screen: &mut VenuesScreen, field: usize, text: &str| {
            screen.update(s, Message::Form(form::Message::Input(field, text.into())));
        };
        let coords = |screen: &VenuesScreen| match &screen.mode {
            Mode::Edit(e) => (e.form.text(LATITUDE), e.form.text(LONGITUDE)),
            _ => panic!("not editing"),
        };
        let google = |at: &str| format!("https://www.google.com/maps/place/X/@{at},17z");
        write(&mut screen, LINK, &google("43.2755,11.9856"));
        assert_eq!(coords(&screen), ("43.2755".into(), "11.9856".into()));
        write(&mut screen, LINK, &google("44.5,12.25"));
        assert_eq!(coords(&screen), ("44.5".into(), "12.25".into()));
        write(&mut screen, LATITUDE, "45");
        write(&mut screen, LINK, &google("46,13"));
        assert_eq!(coords(&screen), ("45".into(), "12.25".into()));
    }

    /// The week of a gym tells every team that holds it, and marks the
    /// ones that overlap.
    #[test]
    fn the_week_marks_the_overlaps() {
        let s = strings(LanguageEnum::It);
        let dir = tempfile::tempdir().unwrap();
        let core = app(&dir);
        let a = core.tutorial_team().unwrap();
        let b = core
            .save_team(
                None,
                TeamForm {
                    name: "Vigili".to_string(),
                    classification: Some(TeamClassificationEnum::Regional),
                    gender: Some(GenderEnum::Men),
                    country: CountryCode::known("IT"),
                    ..core.new_team_form(2026)
                },
            )
            .unwrap();
        let gym = venue(&core, "Terontola");
        let today = core.clock().today();
        for (team, time) in [(&a, "18:00"), (&b, "18:30")] {
            core.save_session(
                team.clone(),
                None,
                SessionDraft {
                    start: Some(format!("{today}T{time}")),
                    duration: "90".into(),
                    venue: Some(gym.clone()),
                    ..core.new_session_draft()
                },
            )
            .unwrap();
        }
        let mut screen = VenuesScreen::new(core).unwrap();
        screen.key(s, key!(enter));
        let held: Vec<_> = screen
            .week
            .iter()
            .flat_map(|d| d.occupations.iter())
            .collect();
        assert_eq!(held.len(), 2);
        assert!(held.iter().all(|o| o.overlapping));
    }
}
