//! The choice of the period of the statistics, of a team or of a player:
//! the last month, the season, the last matches or two days, written or
//! picked on a calendar.

use {
    crate::{
        calendar::{self, Calendar},
        form::Mask,
        keymap,
        strings::Strings,
        theme::{self, Colors, SIZE_BODY, SIZE_SMALL},
        ui,
    },
    chrono::NaiveDate,
    iced::{
        Alignment, Element, Length,
        widget::{button, column, container, row, space, text},
    },
    scout4all_keys::{KeyCombination, key},
    scout4all_mobile::{AnalysisPeriod, CompetitionChoice},
    uuid::Uuid,
};

/// How the period is told.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    LastMonth,
    Season,
    LastMatches,
    Between,
}

pub const KINDS: [Kind; 4] = [
    Kind::LastMonth,
    Kind::Season,
    Kind::LastMatches,
    Kind::Between,
];

/// The last matches offered first, as in the coach's "the last 4".
const LAST_MATCHES: u32 = 4;

/// Under the name of the filter, past its column.
const INDENT: f32 = 148.0;

#[derive(Debug, Clone)]
pub enum Message {
    Kind(Kind),
    Last(u32),
    /// a day picked on the calendar
    Day(NaiveDate),
    /// the calendar a month back (-1) or on (1)
    Month(i32),
    /// the day to write: the last of the period (true) or the first
    Editing(bool),
    /// a digit written on that day, or the last one taken back
    Digit(u8),
    Erase,
}

/// The two days of a period, as written, and the month of the calendar.
#[derive(Debug, Clone)]
struct Days {
    from: String,
    to: String,
    /// the last day is written (or picked) next, not the first
    editing_to: bool,
    /// the first day of the month shown
    month: NaiveDate,
}

impl Days {
    /// A day written whole: chrono would take "2026-10-2" too.
    fn parse(text: &str) -> Option<NaiveDate> {
        (text.len() == 10)
            .then(|| NaiveDate::parse_from_str(text, "%Y-%m-%d").ok())
            .flatten()
    }

    fn period(&self) -> Option<AnalysisPeriod> {
        Self::parse(&self.from)
            .zip(Self::parse(&self.to))
            .map(|(from, to)| AnalysisPeriod::Between {
                from: from.to_string(),
                to: to.to_string(),
            })
    }

    fn editing(&mut self) -> &mut String {
        match self.editing_to {
            true => &mut self.to,
            false => &mut self.from,
        }
    }

    /// A digit written on the day being written: the first day done, the
    /// last one next.
    fn type_digit(&mut self, digit: u8) {
        let text = Mask::Date.apply(&format!("{}{digit}", self.editing()));
        let done = Self::parse(&text);
        *self.editing() = text;
        if let (Some(day), false) = (done, self.editing_to) {
            self.month = calendar::month_of(day);
            self.editing_to = true;
        }
    }

    /// The last digit written taken back, with the separator before it.
    fn erase(&mut self) {
        let mut digits: String = self
            .editing()
            .chars()
            .filter(char::is_ascii_digit)
            .collect();
        digits.pop();
        *self.editing() = Mask::Date.apply(&digits);
    }

    /// A day picked: the first, then the last (the two the right way round).
    fn pick(&mut self, day: NaiveDate) {
        let text = day.format("%Y-%m-%d").to_string();
        match (self.editing_to, Self::parse(&self.from)) {
            (true, Some(from)) if day < from => {
                self.to = self.from.clone();
                self.from = text;
            }
            (true, _) => self.to = text,
            (false, _) => self.from = text,
        }
        self.editing_to = !self.editing_to;
    }
}

/// The period chosen, and what was written to tell it.
#[derive(Debug, Clone)]
pub struct PeriodChoice {
    kind: Kind,
    last: u32,
    days: Days,
    today: NaiveDate,
}

impl PeriodChoice {
    pub fn new(kind: Kind, today: NaiveDate) -> PeriodChoice {
        PeriodChoice {
            kind,
            last: LAST_MATCHES,
            days: Days {
                from: String::new(),
                to: String::new(),
                editing_to: false,
                month: calendar::month_of(today),
            },
            today,
        }
    }

    #[cfg(test)]
    pub fn kind(&self) -> Kind {
        self.kind
    }

    /// The period of the kind chosen: none with two days not written yet,
    /// the one before stays.
    pub fn period(&self) -> Option<AnalysisPeriod> {
        match self.kind {
            Kind::LastMonth => Some(AnalysisPeriod::LastMonth),
            Kind::Season => Some(AnalysisPeriod::Season),
            Kind::LastMatches => Some(AnalysisPeriod::LastMatches { count: self.last }),
            Kind::Between => self.days.period(),
        }
    }

    /// `shown`: the first and last day of the period shown, to start the
    /// two days from.
    pub fn update(&mut self, message: Message, shown: Option<(&str, &str)>) {
        match message {
            Message::Kind(kind) => {
                self.kind = kind;
                if let (Kind::Between, true, Some((from, to))) =
                    (kind, self.days.from.is_empty(), shown)
                {
                    self.days.from = from.to_string();
                    self.days.to = to.to_string();
                    self.days.editing_to = false;
                    if let Some(to) = Days::parse(&self.days.to) {
                        self.days.month = calendar::month_of(to);
                    }
                }
            }
            Message::Last(last) => {
                self.last = last.clamp(1, 99);
                self.kind = Kind::LastMatches;
            }
            Message::Day(day) => {
                self.days.pick(day);
                self.kind = Kind::Between;
            }
            Message::Month(months) => self.days.month = calendar::shift(self.days.month, months),
            Message::Editing(to) => self.days.editing_to = to,
            Message::Digit(digit) => self.days.type_digit(digit),
            Message::Erase => self.days.erase(),
        }
    }

    /// The kind before or after, as ← → ask.
    pub fn cycle(&self, forward: bool) -> Option<Message> {
        ui::cycle(&KINDS, &self.kind, forward).map(Message::Kind)
    }

    /// What a key writes on the period, on its row.
    pub fn key(&self, ck: KeyCombination) -> Option<Message> {
        let letter = ck.as_letter();
        match (self.kind, keymap::digit(&ck)) {
            (Kind::LastMatches, _) if matches!(letter, Some('+' | '=')) => {
                Some(Message::Last(self.last + 1))
            }
            (Kind::LastMatches, _) if letter == Some('-') => {
                Some(Message::Last(self.last.saturating_sub(1)))
            }
            (Kind::Between, Some(digit)) => Some(Message::Digit(digit)),
            (Kind::Between, None) if ck == key!(backspace) => Some(Message::Erase),
            (Kind::Between, None) if ck == key!(space) => {
                Some(Message::Editing(!self.days.editing_to))
            }
            (Kind::Between, None) if ck == key!(pageup) => Some(Message::Month(-1)),
            (Kind::Between, None) if ck == key!(pagedown) => Some(Message::Month(1)),
            _ => None,
        }
    }

    /// The keys of the period beyond ← →.
    pub fn hints(&self, s: &Strings) -> Vec<(String, String)> {
        match self.kind {
            Kind::LastMatches => vec![(
                "+ −".to_string(),
                ui::fill(s.last_matches, &["N".to_string()]),
            )],
            Kind::Between => vec![
                ("0–9 ⌫".to_string(), s.date.to_string()),
                ("␣".to_string(), s.from_to.to_string()),
                ("PgUp PgDn".to_string(), s.calendar_month.to_string()),
            ],
            Kind::LastMonth | Kind::Season => vec![],
        }
    }

    /// The row of the filter: the kinds, then `chosen` (what the period
    /// holds) and, between two days, the calendar with the days `played`.
    pub fn view<'a, M: Clone + 'a>(
        &'a self,
        on: bool,
        chosen: Element<'a, M>,
        played: &'a [NaiveDate],
        wrap: fn(Message) -> M,
        s: &'static Strings,
        c: Colors,
    ) -> Element<'a, M> {
        let pill = |label: String, kind: Kind, message: Message| {
            ui::pill(&label, self.kind == kind, Some(wrap(message)), c)
        };
        let step = |label: &'static str, last: u32| {
            button(text(label).size(SIZE_SMALL).align_x(Alignment::Center))
                .padding([2, 8])
                .on_press(wrap(Message::Last(last)))
                .style(move |_, status| button::Style {
                    background: matches!(status, button::Status::Hovered)
                        .then_some(iced::Background::Color(c.button)),
                    text_color: c.accent,
                    border: iced::border::rounded(6).color(c.border).width(1),
                    ..button::Style::default()
                })
        };
        let last = row![
            pill(
                ui::fill(s.last_matches, &[self.last.to_string()]),
                Kind::LastMatches,
                Message::Last(self.last),
            ),
            step("−", self.last.saturating_sub(1)),
            step("+", self.last + 1),
        ]
        .spacing(4)
        .align_y(Alignment::Center);
        let pills: Vec<Element<'a, M>> = vec![
            pill(
                s.last_month.to_string(),
                Kind::LastMonth,
                Message::Kind(Kind::LastMonth),
            ),
            pill(
                s.season_period.to_string(),
                Kind::Season,
                Message::Kind(Kind::Season),
            ),
            last.into(),
            pill(
                s.between.to_string(),
                Kind::Between,
                Message::Kind(Kind::Between),
            ),
        ];
        let between: Element<'a, M> = match self.kind {
            Kind::Between => self.days_view(played, wrap, s, c),
            Kind::LastMonth | Kind::Season | Kind::LastMatches => {
                space::vertical().height(0).into()
            }
        };
        column![
            ui::choice_line(s.period, on, pills, c),
            row![space::horizontal().width(INDENT), chosen],
            between
        ]
        .spacing(6)
        .into()
    }

    /// The two days, as written (the one being written lit), and the
    /// calendar.
    fn days_view<'a, M: Clone + 'a>(
        &'a self,
        played: &'a [NaiveDate],
        wrap: fn(Message) -> M,
        s: &'static Strings,
        c: Colors,
    ) -> Element<'a, M> {
        let field = |label: &'static str, value: &str, editing: bool, to: bool| {
            let shown = match value.is_empty() {
                true => s.date_placeholder.to_string(),
                false => value.to_string(),
            };
            let caret = match editing {
                true => "▏",
                false => "",
            };
            button(
                column![
                    text(label).size(SIZE_SMALL).color(c.muted),
                    text(format!("{shown}{caret}"))
                        .size(SIZE_BODY)
                        .font(theme::fonts().bold)
                        .color(match value.is_empty() {
                            true => c.muted,
                            false => c.text,
                        }),
                ]
                .spacing(2),
            )
            .padding([6, 10])
            .width(Length::Fixed(140.0))
            .on_press(wrap(Message::Editing(to)))
            .style(move |_, _| button::Style {
                border: iced::border::rounded(8)
                    .color(match editing {
                        true => c.accent,
                        false => c.border,
                    })
                    .width(match editing {
                        true => 2,
                        false => 1,
                    }),
                ..button::Style::default()
            })
        };
        let fields = row![
            field(s.from_day, &self.days.from, !self.days.editing_to, false),
            field(s.to_day, &self.days.to, self.days.editing_to, true),
        ]
        .spacing(8);
        let calendar: Element<'a, M> = Calendar {
            month: self.days.month,
            from: Days::parse(&self.days.from),
            to: Days::parse(&self.days.to),
            marked: played,
            today: self.today,
        }
        .view(s, c, Message::Day, Message::Month)
        .map(wrap);
        column![
            fields,
            text(s.period_hint).size(SIZE_SMALL).color(c.muted),
            container(calendar).padding([4, 0])
        ]
        .spacing(6)
        .padding(iced::Padding {
            left: INDENT,
            ..iced::Padding::ZERO
        })
        .into()
    }
}

/// The row of the competitions: all of them or one.
pub fn competition_line<'a, M: Clone + 'a>(
    on: bool,
    competitions: &[CompetitionChoice],
    chosen: Option<Uuid>,
    wrap: fn(Option<Uuid>) -> M,
    s: &'static Strings,
    c: Colors,
) -> Element<'a, M> {
    let pills = std::iter::once((None, s.filter_all.to_string()))
        .chain(competitions.iter().map(|competition| {
            let label = match &competition.season {
                Some(season) => format!("{} {season}", competition.name),
                None => competition.name.clone(),
            };
            (Some(competition.id), label)
        }))
        .map(|(id, label)| ui::pill(&label, chosen == id, Some(wrap(id)), c))
        .collect();
    ui::choice_line(s.competition, on, pills, c)
}

#[cfg(test)]
impl PeriodChoice {
    /// The two days as written.
    pub fn written(&self) -> (&str, &str) {
        (&self.days.from, &self.days.to)
    }

    /// Both days to write again, the first first.
    pub fn forget_days(&mut self) {
        self.days.from.clear();
        self.days.to.clear();
        self.days.editing_to = false;
    }
}

/// The days a calendar marks, from their texts.
pub fn played_days(days: &[String]) -> Vec<NaiveDate> {
    days.iter().filter_map(|d| Days::parse(d)).collect()
}
