//! The profile of a player in one team, read only: the header (number,
//! name, face and the logo of the team), the numbers of the season and
//! what the registry says of the person.

use {
    crate::{
        notice::{self, Level},
        player_stats::PlayerStats,
        strings::Strings,
        team::Message,
        theme::{self, Colors, SIZE_BODY, SIZE_SMALL},
        ui,
    },
    iced::{
        Alignment, ContentFit, Element, Length, padding,
        widget::{
            Column, Row, button, column, container, image, image::Handle, row, scrollable, space,
            text,
        },
    },
    scout4all_core::shapes::player::DominantHand,
    scout4all_mobile::{Player, PlayerProfile, TeamDetails},
};

/// The header, and the face resting on its bottom edge.
const BAND: f32 = 220.0;
const FACE: f32 = 214.0;
const LOGO: f32 = 54.0;
const NUMBER_SIZE: f32 = 112.0;
const NAME_SIZE: f32 = 36.0;
const METRIC_SIZE: f32 = 34.0;
const FIELD_SIZE: f32 = 19.0;
const METRIC_COLUMNS: usize = 4;
const FIELD_COLUMNS: usize = 3;

/// The profile open, over the page of the team.
pub struct Profile {
    /// the id of the player: the list may change its order
    pub player: String,
    pub numbers: Numbers,
    pub tab: Tab,
    /// read the first time its tab shows
    pub stats: Option<Box<PlayerStats>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Profile,
    Statistics,
}

/// The numbers of the season, worked out off the thread of the screen.
pub enum Numbers {
    Loading,
    Ready(Box<PlayerProfile>),
    Failed(String),
}

/// What the profile shows besides the numbers.
pub struct Shown<'a> {
    pub team: &'a TeamDetails,
    pub player: &'a Player,
    pub photo: Option<&'a Handle>,
    pub logo: Option<&'a ui::TeamLogo>,
    /// the nationality, by name
    pub country: Option<String>,
    pub initials: String,
}

/// A value under its small label.
struct Item {
    label: String,
    value: String,
    detail: Option<String>,
}

impl Item {
    fn new(label: &str, value: impl ToString) -> Item {
        Item {
            label: label.to_uppercase(),
            value: value.to_string(),
            detail: None,
        }
    }

    fn detail(self, detail: String) -> Item {
        Item {
            detail: Some(detail),
            ..self
        }
    }
}

pub fn view<'a>(
    shown: Shown<'a>,
    open: &'a Profile,
    s: &'static Strings,
    c: Colors,
) -> Element<'a, Message> {
    let pill =
        |name: &str, tab: Tab| ui::pill(name, open.tab == tab, Some(Message::ProfileTab(tab)), c);
    let tabs = row![
        pill(s.profile_tab, Tab::Profile),
        pill(s.statistics_tab, Tab::Statistics)
    ]
    .spacing(6);
    let body: Element<'a, Message> = match (open.tab, &open.stats) {
        (Tab::Statistics, Some(stats)) => stats.view(s, c).map(Message::PlayerStats),
        (Tab::Statistics, None) => space::vertical().into(),
        (Tab::Profile, _) => scrollable(details(&shown, &open.numbers, s, c))
            .height(Length::Fill)
            .into(),
    };
    column![header(&shown, c), tabs, body].spacing(14).into()
}

/// The numbers of the season and what the registry says of the person.
fn details<'a>(
    shown: &Shown<'a>,
    numbers: &'a Numbers,
    s: &'static Strings,
    c: Colors,
) -> Element<'a, Message> {
    let ready = match numbers {
        Numbers::Ready(profile) => Some(profile.as_ref()),
        Numbers::Loading | Numbers::Failed(_) => None,
    };
    let season: Element<'a, Message> = match numbers {
        Numbers::Loading => space::vertical().height(0).into(),
        Numbers::Failed(error) => notice::banner(Level::Error, error, c),
        Numbers::Ready(profile) => metrics(profile, s, c),
    };
    let fields = fields(shown.player, ready, shown.country.clone(), s);
    let notes = Some(&shown.player.form.notes)
        .filter(|notes| !notes.is_empty())
        .map(|notes| item(Item::new(s.notes, notes), FIELD_SIZE, c));
    let others = ready
        .filter(|profile| !profile.other_teams.is_empty())
        .map(|profile| other_teams(profile, s, c));
    let any = !fields.is_empty() || notes.is_some() || others.is_some();
    let rule = any.then(|| {
        container(space::horizontal())
            .width(Length::Fill)
            .height(Length::Fixed(1.0))
            .style(move |_| container::Style {
                background: Some(c.border.into()),
                ..container::Style::default()
            })
    });
    Column::new()
        .push(season)
        .push(rule)
        .push(grid(fields, FIELD_COLUMNS, FIELD_SIZE, c))
        .push(notes)
        .push(others)
        .spacing(22)
        .padding([8, 16])
        .into()
}

/// The band: from the left the number, the name over role and team, the
/// logo and the face; the line of the accent under it. The face touches
/// the right edge: the photo cuts the shoulders at its sides.
fn header<'a>(shown: &Shown<'a>, c: Colors) -> Element<'a, Message> {
    let Shown {
        team,
        player,
        photo,
        logo,
        ..
    } = *shown;
    let role_and_team = player
        .role
        .iter()
        .cloned()
        .chain([format!("{} {}", team.name, team.year)])
        .collect::<Vec<_>>()
        .join(" · ");
    let face: Element<'a, Message> = match photo {
        Some(photo) => image(photo.clone())
            .width(Length::Fixed(FACE))
            .height(Length::Fixed(FACE))
            .content_fit(ContentFit::Contain)
            .into(),
        None => container(
            text(shown.initials.clone())
                .size(NUMBER_SIZE * 0.7)
                .font(theme::fonts().bold)
                .color(c.muted),
        )
        .center_x(Length::Fixed(FACE))
        .center_y(Length::Fixed(FACE))
        .into(),
    };
    let logo = logo.map(|logo| {
        container(ui::team_logo(logo, LOGO, c))
            .height(Length::Fill)
            .padding([16, 0])
    });
    let who = row![
        text(format!("{:02}", player.number))
            .size(NUMBER_SIZE)
            .font(theme::fonts().bold)
            .color(c.accent),
        column![
            text(player.name.to_uppercase())
                .size(NAME_SIZE)
                .font(theme::fonts().italic),
            text(role_and_team).size(SIZE_BODY).color(c.muted),
        ]
        .spacing(6)
        .width(Length::Fill),
    ]
    .spacing(24)
    .align_y(Alignment::Center);
    let band = Row::new()
        .push(who.width(Length::Fill))
        .push(logo)
        .push(container(face).height(Length::Fill).align_y(Alignment::End))
        .spacing(24)
        .align_y(Alignment::Center)
        .height(Length::Fill);
    let line = container(space::horizontal())
        .width(Length::Fill)
        .height(Length::Fixed(4.0))
        .style(move |_| container::Style {
            background: Some(c.accent.into()),
            ..container::Style::default()
        });
    column![
        container(band)
            .padding(padding::left(24))
            .width(Length::Fill)
            .height(Length::Fixed(BAND))
            .style(move |_| container::Style {
                background: Some(c.button.into()),
                ..container::Style::default()
            }),
        line
    ]
    .into()
}

/// Matches, sets and points once the player has been on court, and the
/// attendance when the team trains.
fn metrics<'a>(profile: &PlayerProfile, s: &'static Strings, c: Colors) -> Element<'a, Message> {
    let attendance = profile.has_training.then(|| {
        let percent = profile
            .attendance_percent
            .map_or_else(|| "—".to_string(), |n| format!("{n}%"));
        Item::new(s.attendance, percent)
    });
    match profile.matches_played {
        0 => Row::new()
            .push(
                container(ui::body(s.no_matches_played).color(c.muted))
                    .width(Length::FillPortion(3)),
            )
            .push(
                attendance
                    .map(|a| container(item(a, METRIC_SIZE, c)).width(Length::FillPortion(1))),
            )
            .spacing(16)
            .align_y(Alignment::Center)
            .into(),
        played => {
            let numbers = [
                Item::new(s.matches, played)
                    .detail(ui::fill(s.out_of, &[profile.matches_scouted.to_string()])),
                Item::new(s.sets, profile.sets_played),
                Item::new(s.points, profile.points).detail(ui::fill(
                    s.points_detail,
                    &[
                        profile.attack_points.to_string(),
                        profile.block_points.to_string(),
                        profile.serve_points.to_string(),
                    ],
                )),
            ];
            grid(
                numbers.into_iter().chain(attendance).collect(),
                METRIC_COLUMNS,
                METRIC_SIZE,
                c,
            )
        }
    }
}

/// What's known of the person, the empty ones left out.
fn fields(
    player: &Player,
    profile: Option<&PlayerProfile>,
    country: Option<String>,
    s: &'static Strings,
) -> Vec<Item> {
    let form = &player.form;
    let birth = Some(&form.birth_date)
        .filter(|date| !date.is_empty())
        .map(|date| {
            let day = s.day(date);
            let value = match profile.and_then(|p| p.age) {
                Some(age) => format!("{day} ({age})"),
                None => day,
            };
            Item::new(s.born, value)
        });
    let written = |label: &str, value: &str| {
        Some(value)
            .filter(|value| !value.is_empty())
            .map(|value| Item::new(label, value))
    };
    let hand = form.dominant_hand.map(|hand| {
        let name = match hand {
            DominantHand::Right => s.right,
            DominantHand::Left => s.left,
        };
        Item::new(s.dominant_hand, name)
    });
    [
        birth,
        country.map(|name| Item::new(s.nationality, name)),
        written(s.height_of, &form.height_cm).map(|height| Item {
            value: format!("{} cm", height.value),
            ..height
        }),
        hand,
        written(s.federation_id, &form.federation_id),
    ]
    .into_iter()
    .flatten()
    .collect()
}

fn item<'a>(item: Item, size: f32, c: Colors) -> Element<'a, Message> {
    Column::new()
        .push(
            text(item.label)
                .size(SIZE_SMALL)
                .font(theme::fonts().italic)
                .color(c.accent),
        )
        .push(text(item.value).size(size).font(theme::fonts().bold))
        .push(
            item.detail
                .map(|detail| text(detail).size(SIZE_SMALL).color(c.muted)),
        )
        .spacing(4)
        .into()
}

/// The items in `columns` columns of the same width, row by row.
fn grid<'a>(items: Vec<Item>, columns: usize, size: f32, c: Colors) -> Element<'a, Message> {
    let mut items = items.into_iter().peekable();
    let rows = std::iter::from_fn(|| {
        items.peek()?;
        let cells = (0..columns).map(|_| {
            let cell: Element<'a, Message> = match items.next() {
                Some(next) => item(next, size, c),
                None => space::horizontal().into(),
            };
            container(cell).width(Length::FillPortion(1)).into()
        });
        Some(Row::with_children(cells).spacing(16).into())
    });
    Column::with_children(rows).spacing(18).into()
}

/// The other teams of the season, each opening the profile there.
fn other_teams<'a>(
    profile: &PlayerProfile,
    s: &'static Strings,
    c: Colors,
) -> Element<'a, Message> {
    let teams = profile.other_teams.iter().map(|team| {
        button(text(format!("{} · {} {}", team.number, team.name, team.year)).size(SIZE_BODY))
            .padding([6, 12])
            .on_press(Message::OtherTeam(team.id.clone()))
            .style(move |_, status| {
                let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
                button::Style {
                    background: hovered.then(|| c.button.into()),
                    text_color: c.text,
                    border: iced::Border {
                        color: c.border,
                        width: 1.0,
                        radius: 8.0.into(),
                    },
                    ..button::Style::default()
                }
            })
            .into()
    });
    column![
        text(s.also_plays_in.to_uppercase())
            .size(SIZE_SMALL)
            .font(theme::fonts().italic)
            .color(c.accent),
        Row::with_children(teams).spacing(8).wrap()
    ]
    .spacing(8)
    .into()
}

#[cfg(test)]
mod tests {
    use {
        crate::{strings::strings, ui},
        scout4all_core::shapes::enums::LanguageEnum,
    };

    #[test]
    fn the_details_of_the_numbers_are_filled_in_every_language() {
        use LanguageEnum::*;
        for language in [En, It, Pt, Pl, Ja] {
            let s = strings(language);
            let values = ["18".to_string(), "21".to_string(), "17".to_string()];
            for (template, count) in [(s.out_of, 1), (s.points_detail, 3)] {
                let filled = ui::fill(template, &values);
                assert!(!filled.contains('{'), "{filled}");
                assert!(
                    values.iter().take(count).all(|v| filled.contains(v)),
                    "{filled}"
                );
            }
        }
    }
}
