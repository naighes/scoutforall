//! A month to choose the days of a period from: the days between the two
//! chosen lit, the days with a match marked, today ringed. A click on a day
//! takes it; ‹ › (or PgUp PgDn) change the month.

use {
    crate::{
        strings::Strings,
        theme::{self, Colors, SIZE_SMALL},
    },
    chrono::{Datelike, Months, NaiveDate},
    iced::{
        Alignment, Background, Color, Element, Length, Padding,
        widget::{Column, Row, button, column, container, row, space, text},
    },
};

const CELL: f32 = 38.0;

/// The first day of the month of `day`.
pub fn month_of(day: NaiveDate) -> NaiveDate {
    day.with_day(1).unwrap_or(day)
}

/// The month before (`-1`) or after (`1`) the month of `first`.
pub fn shift(first: NaiveDate, months: i32) -> NaiveDate {
    let m = Months::new(months.unsigned_abs());
    match months < 0 {
        true => first.checked_sub_months(m),
        false => first.checked_add_months(m),
    }
    .map(month_of)
    .unwrap_or(first)
}

/// The days to draw: a week a row, `None` out of the month.
pub fn weeks(first: NaiveDate, from_sunday: bool) -> Vec<[Option<NaiveDate>; 7]> {
    let first = month_of(first);
    let offset = match from_sunday {
        true => first.weekday().num_days_from_sunday(),
        false => first.weekday().num_days_from_monday(),
    } as usize;
    let days: Vec<NaiveDate> = first
        .iter_days()
        .take_while(|d| d.month() == first.month())
        .collect();
    let cells: Vec<Option<NaiveDate>> = std::iter::repeat_n(None, offset)
        .chain(days.into_iter().map(Some))
        .collect();
    cells
        .chunks(7)
        .map(|week| std::array::from_fn(|i| week.get(i).copied().flatten()))
        .collect()
}

/// What the calendar shows.
pub struct Calendar<'a> {
    /// the first day of the month shown
    pub month: NaiveDate,
    pub from: Option<NaiveDate>,
    pub to: Option<NaiveDate>,
    /// the days with a match played
    pub marked: &'a [NaiveDate],
    pub today: NaiveDate,
}

impl Calendar<'_> {
    pub fn view<'a, M: Clone + 'a>(
        &self,
        s: &'static Strings,
        c: Colors,
        on_day: impl Fn(NaiveDate) -> M,
        on_month: impl Fn(i32) -> M,
    ) -> Element<'a, M> {
        let arrow = |label: &'static str, months: i32| {
            button(text(label).size(SIZE_SMALL).align_x(Alignment::Center))
                .padding(Padding::from([2, 10]))
                .on_press(on_month(months))
                .style(move |_, status| button::Style {
                    background: matches!(status, button::Status::Hovered)
                        .then_some(Background::Color(c.button)),
                    text_color: c.accent,
                    border: iced::border::rounded(6),
                    ..button::Style::default()
                })
        };
        let month_name = s
            .months
            .get(self.month.month0() as usize)
            .copied()
            .unwrap_or_default();
        let header = row![
            arrow("‹", -1),
            text(format!("{month_name} {}", self.month.year()))
                .size(SIZE_SMALL)
                .font(theme::fonts().bold)
                .color(c.text)
                .width(Length::Fill)
                .align_x(Alignment::Center),
            arrow("›", 1),
        ]
        .align_y(Alignment::Center)
        .width(Length::Fixed(CELL * 7.0));
        let names: Vec<&str> = match s.week_from_sunday {
            true => std::iter::once(s.weekdays[6])
                .chain(s.weekdays[..6].iter().copied())
                .collect(),
            false => s.weekdays.to_vec(),
        };
        let weekdays = Row::with_children(names.into_iter().map(|n| {
            text(n.to_string())
                .size(SIZE_SMALL)
                .color(c.muted)
                .width(Length::Fixed(CELL))
                .align_x(Alignment::Center)
                .into()
        }));
        let (low, high) = match (self.from, self.to) {
            (Some(a), Some(b)) => (Some(a.min(b)), Some(a.max(b))),
            (a, b) => (a.or(b), b.or(a)),
        };
        let weeks = Column::with_children(weeks(self.month, s.week_from_sunday).into_iter().map(
            |week| {
                Row::with_children(week.into_iter().map(|day| match day {
                    None => space::horizontal().width(Length::Fixed(CELL)).into(),
                    Some(day) => {
                        let end = Some(day) == low || Some(day) == high;
                        let inside = low.zip(high).is_some_and(|(l, h)| l < day && day < h);
                        let marked = self.marked.contains(&day);
                        let today = day == self.today;
                        self.day(day, end, inside, marked, today, c, on_day(day))
                    }
                }))
                .into()
            },
        ));
        column![header, weekdays, weeks].spacing(4).into()
    }

    #[allow(clippy::too_many_arguments)]
    fn day<'a, M: Clone + 'a>(
        &self,
        day: NaiveDate,
        end: bool,
        inside: bool,
        marked: bool,
        today: bool,
        c: Colors,
        message: M,
    ) -> Element<'a, M> {
        let number = text(day.day().to_string())
            .size(SIZE_SMALL)
            .font(match marked {
                true => theme::fonts().bold,
                false => theme::fonts().regular,
            })
            .color(match (end, marked) {
                (true, _) => c.on_accent,
                (false, true) => c.accent,
                (false, false) => c.text,
            });
        // the days with a match: a dot under the number
        let dot =
            container(space::horizontal().width(4).height(4)).style(move |_| container::Style {
                background: marked.then_some(Background::Color(match end {
                    true => c.on_accent,
                    false => c.accent,
                })),
                border: iced::border::rounded(2),
                ..container::Style::default()
            });
        let content = column![number, dot]
            .spacing(1)
            .align_x(Alignment::Center)
            .width(Length::Fill);
        button(content)
            .width(Length::Fixed(CELL))
            .height(Length::Fixed(CELL - 4.0))
            .padding(Padding::from([5, 0]))
            .on_press(message)
            .style(move |_, status| {
                let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
                button::Style {
                    background: match (end, inside, hovered) {
                        (true, _, _) => Some(Background::Color(c.accent)),
                        (false, true, _) => Some(Background::Color(Color {
                            a: 0.22,
                            ..c.accent
                        })),
                        (false, false, true) => Some(Background::Color(c.button)),
                        (false, false, false) => None,
                    },
                    border: iced::border::rounded(8)
                        .color(match today {
                            true => c.muted,
                            false => Color::TRANSPARENT,
                        })
                        .width(1),
                    ..button::Style::default()
                }
            })
            .into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap_or_default()
    }

    /// November 2026 starts on a Sunday: the first row has it last from
    /// Monday, alone first from Sunday.
    #[test]
    fn the_weeks_of_a_month() {
        let monday = weeks(day(2026, 11, 17), false);
        assert_eq!(monday.len(), 6);
        assert_eq!(monday[0][6], Some(day(2026, 11, 1)));
        assert_eq!(monday[0][5], None);
        assert_eq!(monday[5][0], Some(day(2026, 11, 30)));
        let sunday = weeks(day(2026, 11, 17), true);
        assert_eq!(sunday.len(), 5);
        assert_eq!(sunday[0][0], Some(day(2026, 11, 1)));
        assert_eq!(sunday[4][1], Some(day(2026, 11, 30)));
        assert_eq!(sunday[4][2], None);
    }

    #[test]
    fn the_months_before_and_after() {
        assert_eq!(shift(day(2026, 1, 1), -1), day(2025, 12, 1));
        assert_eq!(shift(day(2026, 12, 1), 1), day(2027, 1, 1));
        assert_eq!(month_of(day(2026, 3, 31)), day(2026, 3, 1));
    }
}
