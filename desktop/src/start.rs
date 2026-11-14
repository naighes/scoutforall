//! The start of a set, as in the TUI: who serves first (in the first set
//! and in the tie-break), the players in positions 1 to 6, the setter, the
//! libero and the fallback libero. Each choice goes on to the next one, Esc
//! goes back to the one before; the lineup of the last set is proposed.

use {
    crate::{
        strings::Strings,
        theme::{self, Colors, SIZE_BODY, SIZE_SMALL},
        ui,
    },
    iced::{
        Alignment, Element, Length,
        widget::{Column, button, column, container, row, scrollable, space, text},
    },
    scout4all_core::{
        names,
        shapes::enums::{RoleEnum, TeamSideEnum},
    },
    scout4all_keys::{BACK_KEY, KeyCombination, key},
    scout4all_mobile::{Lineup, LineupPlayer, SetStart},
    std::sync::Arc,
};

/// What is chosen now.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Step {
    Serving,
    /// a position, 1 to 6
    Position(u8),
    Setter,
    Libero,
    FallbackLibero,
}

#[derive(Debug, Clone)]
pub enum Message {
    Serving(TeamSideEnum),
    Choose(String),
    /// no fallback libero
    Skip,
    /// a position on the court clicked: its player to choose (again)
    Position(u8),
}

pub enum Outcome {
    Stay,
    /// back to where the set was started from
    Back,
    /// all chosen: save the set and scout it
    Start,
}

pub struct StartScreen {
    start: Arc<SetStart>,
    /// the team and the match, to scout the set once started
    pub team_id: String,
    pub match_id: String,
    step: Step,
    lineup: Lineup,
    choices: Vec<LineupPlayer>,
    selected: usize,
    error: Option<String>,
    /// where the choices were when a position was clicked: back there once
    /// it's chosen
    resume: Option<Step>,
}

/// The positions as they stand on court, the net on top.
const COURT: [[u8; 3]; 2] = [[4, 3, 2], [5, 6, 1]];

/// The letters of a name on a cell of the court: longer, it's
/// abbreviated ("Ale. Mag.").
const NAME_ROOM: usize = 22;

impl StartScreen {
    pub fn new(start: Arc<SetStart>, team_id: String, match_id: String) -> Self {
        let lineup = start.lineup();
        let step = match lineup.chooses_serving_team {
            true => Step::Serving,
            false => Step::Position(1),
        };
        let mut screen = StartScreen {
            start,
            team_id,
            match_id,
            step,
            lineup,
            choices: vec![],
            selected: 0,
            error: None,
            resume: None,
        };
        screen.go_to(step);
        screen
    }

    /// The match, for the bar on top.
    pub fn title(&self) -> String {
        format!("{} – {}", self.lineup.team, self.lineup.opponent)
    }

    pub fn set_start(&self) -> Arc<SetStart> {
        self.start.clone()
    }

    /// The set could not start: the core says why.
    pub fn failed(&mut self, reason: String) {
        self.error = Some(reason);
    }

    fn go_to(&mut self, step: Step) {
        self.step = step;
        self.lineup = self.start.lineup();
        self.choices = match step {
            Step::Serving => vec![],
            Step::Position(p) => self.start.position_choices(p),
            Step::Setter => self.start.setter_choices(),
            Step::Libero => self.start.libero_choices(),
            Step::FallbackLibero => self.start.fallback_libero_choices(),
        };
        self.selected = self.default_select();
    }

    /// The player proposed for the step (the lineup of the last set), else
    /// the first with the role of the step, else the first.
    fn default_select(&self) -> usize {
        let l = &self.lineup;
        let proposed = match self.step {
            Step::Serving => None,
            Step::Position(p) => l
                .positions
                .get(usize::from(p.saturating_sub(1)))
                .cloned()
                .flatten(),
            Step::Setter => l.setter.clone(),
            Step::Libero => l.libero.clone(),
            Step::FallbackLibero => l.fallback_libero.clone(),
        };
        let role = match self.step {
            Step::Setter => Some(RoleEnum::Setter),
            Step::Libero | Step::FallbackLibero => Some(RoleEnum::Libero),
            Step::Serving | Step::Position(_) => None,
        };
        proposed
            .and_then(|p| self.choices.iter().position(|c| c.id == p.id))
            .or_else(|| role.and_then(|r| self.choices.iter().position(|c| c.role == Some(r))))
            .unwrap_or(0)
    }

    fn done<T>(&mut self, r: Result<T, scout4all_mobile::MobileError>) -> bool {
        match r {
            Ok(_) => {
                self.error = None;
                true
            }
            Err(e) => {
                self.error = Some(e.to_string());
                false
            }
        }
    }

    fn choose(&mut self, id: String) -> Outcome {
        let (chosen, next) = match (self.step, self.resume) {
            (Step::Serving, _) => return Outcome::Stay,
            // a position clicked: the first one empty, else back where
            // the choices were
            (Step::Position(p), Some(resume)) => {
                let placed = self.start.place(p, Some(id));
                let empty = self
                    .start
                    .lineup()
                    .positions
                    .iter()
                    .position(Option::is_none)
                    .and_then(|i| u8::try_from(i + 1).ok());
                self.resume = None;
                (placed, Some(empty.map_or(resume, Step::Position)))
            }
            (Step::Position(p), None) => (
                self.start.place(p, Some(id)),
                match p {
                    6 => Some(Step::Setter),
                    _ => Some(Step::Position(p + 1)),
                },
            ),
            (Step::Setter, _) => (self.start.choose_setter(Some(id)), Some(Step::Libero)),
            (Step::Libero, _) => {
                let chosen = self.start.choose_libero(Some(id));
                // no fallback libero to choose: the set starts
                let next = match self.start.fallback_libero_choices().is_empty() {
                    true => None,
                    false => Some(Step::FallbackLibero),
                };
                (chosen, next)
            }
            (Step::FallbackLibero, _) => (self.start.choose_fallback_libero(Some(id)), None),
        };
        match (self.done(chosen), next) {
            (false, _) => Outcome::Stay,
            (true, Some(step)) => {
                self.go_to(step);
                Outcome::Stay
            }
            (true, None) => {
                self.lineup = self.start.lineup();
                Outcome::Start
            }
        }
    }

    fn serving(&mut self, side: TeamSideEnum) {
        self.start.choose_serving_team(side);
        self.lineup = self.start.lineup();
    }

    /// The player of `position` to choose, clicked on the court; then
    /// back to the step of now.
    fn reposition(&mut self, position: u8) {
        if self.step == Step::Serving || self.step == Step::Position(position) {
            return;
        }
        self.resume = Some(self.resume.unwrap_or(self.step));
        self.go_to(Step::Position(position));
    }

    /// Back to the choice before, undoing it; from a position clicked,
    /// back where the choices were.
    fn back(&mut self) -> Outcome {
        if let Some(step) = self.resume.take() {
            self.go_to(step);
            return Outcome::Stay;
        }
        let undone = match (self.step, self.lineup.chooses_serving_team) {
            (Step::Serving, _) | (Step::Position(1), false) => return Outcome::Back,
            (Step::Position(1), true) => (Ok(()), Step::Serving),
            (Step::Position(p), _) => (self.start.place(p - 1, None), Step::Position(p - 1)),
            (Step::Setter, _) => (self.start.place(6, None), Step::Position(6)),
            (Step::Libero, _) => (self.start.choose_setter(None), Step::Setter),
            (Step::FallbackLibero, _) => (self.start.choose_libero(None), Step::Libero),
        };
        let (r, step) = undone;
        if self.done(r) {
            self.go_to(step)
        }
        Outcome::Stay
    }

    fn skip(&mut self) -> Outcome {
        match self.step {
            Step::FallbackLibero => {
                let skipped = self.start.choose_fallback_libero(None);
                match self.done(skipped) {
                    true => Outcome::Start,
                    false => Outcome::Stay,
                }
            }
            _ => Outcome::Stay,
        }
    }

    pub fn update(&mut self, message: Message) -> Outcome {
        match message {
            Message::Serving(side) => {
                self.serving(side);
                self.go_to(Step::Position(1));
                Outcome::Stay
            }
            Message::Choose(id) => self.choose(id),
            Message::Skip => self.skip(),
            Message::Position(position) => {
                self.reposition(position);
                Outcome::Stay
            }
        }
    }

    pub fn key(&mut self, ck: KeyCombination) -> Outcome {
        let count = self.choices.len();
        match (self.step, ck) {
            (_, k) if k == BACK_KEY => self.back(),
            (Step::Serving, k) if k == key!(left) || k == key!(right) => {
                self.serving(match self.lineup.serving_team {
                    Some(TeamSideEnum::Us) => TeamSideEnum::Them,
                    Some(TeamSideEnum::Them) | None => TeamSideEnum::Us,
                });
                Outcome::Stay
            }
            (Step::Serving, k) if k == key!(enter) => match self.lineup.serving_team {
                Some(_) => {
                    self.go_to(Step::Position(1));
                    Outcome::Stay
                }
                None => Outcome::Stay,
            },
            (Step::Serving, _) => Outcome::Stay,
            (_, k) if k == key!(down) && count > 0 => {
                self.selected = (self.selected + 1) % count;
                Outcome::Stay
            }
            (_, k) if k == key!(up) && count > 0 => {
                self.selected = self.selected.checked_sub(1).unwrap_or(count - 1);
                Outcome::Stay
            }
            (_, k) if k == key!(enter) => match self.choices.get(self.selected) {
                Some(p) => self.choose(p.id.clone()),
                None => Outcome::Stay,
            },
            (_, k) if k == key!(tab) => self.skip(),
            _ => Outcome::Stay,
        }
    }

    /// The keys of the screen, for the bar at the bottom.
    pub fn hints(&self, s: &Strings) -> Vec<(String, String)> {
        let moves = match self.step {
            Step::Serving => "← →",
            _ => "↑ ↓",
        };
        let skip = match self.step {
            Step::FallbackLibero => vec![("Tab".to_string(), s.no_fallback_libero.to_string())],
            _ => vec![],
        };
        [
            vec![
                (moves.to_string(), String::new()),
                ("Enter".to_string(), s.choose.to_string()),
            ],
            skip,
            vec![("Esc".to_string(), s.back.to_string())],
        ]
        .concat()
    }

    fn step_title(&self, s: &Strings) -> String {
        match self.step {
            Step::Serving => s.serving_first.to_string(),
            Step::Position(p) => format!("{} {p}", s.position),
            Step::Setter => s.setter.to_string(),
            Step::Libero => s.libero.to_string(),
            Step::FallbackLibero => s.fallback_libero.to_string(),
        }
    }

    pub fn view<'a>(&'a self, s: &'static Strings, c: Colors) -> Element<'a, Message> {
        let error: Element<'a, Message> = match &self.error {
            Some(e) => ui::body(e.clone()).color(c.danger).into(),
            None => space::vertical().height(0).into(),
        };
        let left = container(ui::panel(
            Some(&self.step_title(s)),
            self.choices_view(s, c),
            c,
        ));
        // who serves first alone: the lineup comes after
        let body: Element<'a, Message> = match self.step {
            Step::Serving => container(container(left).max_width(520))
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(Alignment::Center)
                .align_y(Alignment::Center)
                .into(),
            _ => row![
                left.width(Length::Fixed(400.0)).height(Length::Fill),
                self.lineup_view(s, c)
            ]
            .spacing(14)
            .height(Length::Fill)
            .into(),
        };
        column![error, body].spacing(10).into()
    }

    fn choices_view<'a>(&'a self, s: &'static Strings, c: Colors) -> Element<'a, Message> {
        let bold = theme::fonts().bold;
        match self.step {
            Step::Serving => {
                let side = |name: &str, side: TeamSideEnum| {
                    let chosen = self.lineup.serving_team == Some(side);
                    container(ui::list_row(
                        container(text(name.to_string()).size(theme::SIZE_TITLE).font(bold))
                            .width(Length::Fill)
                            .align_x(Alignment::Center)
                            .padding(18),
                        chosen,
                        Some(Message::Serving(side)),
                        c,
                    ))
                    .style(move |_| container::Style {
                        border: iced::Border {
                            color: c.border,
                            width: 1.0,
                            radius: 8.0.into(),
                        },
                        ..container::Style::default()
                    })
                };
                // the coin toss, said as on the court
                column![
                    container(ui::icon(theme::icons::COIN, 56.0, c.accent))
                        .width(Length::Fill)
                        .align_x(Alignment::Center),
                    container(
                        ui::body(s.serving_hint)
                            .color(c.muted)
                            .align_x(Alignment::Center)
                    )
                    .width(Length::Fill)
                    .align_x(Alignment::Center),
                    space::vertical().height(4),
                    side(&self.lineup.team, TeamSideEnum::Us),
                    side(&self.lineup.opponent, TeamSideEnum::Them),
                ]
                .spacing(12)
                .into()
            }
            _ if self.choices.is_empty() => ui::body(s.no_available_players).color(c.muted).into(),
            _ => {
                let rows = self.choices.iter().enumerate().map(|(i, p)| {
                    ui::list_row(
                        row![
                            text(p.number.to_string())
                                .size(SIZE_BODY)
                                .font(bold)
                                .width(Length::Fixed(32.0)),
                            ui::name(p.name.clone()),
                            text(p.role_name.clone().unwrap_or_default())
                                .size(SIZE_SMALL)
                                .color(c.muted),
                        ]
                        .spacing(6)
                        .align_y(Alignment::Center),
                        i == self.selected,
                        Some(Message::Choose(p.id.clone())),
                        c,
                    )
                });
                // the set can start: Continue (or Tab) with no second
                // libero, above the players, no need to look for it
                let skip: Element<'a, Message> = match self.step {
                    Step::FallbackLibero => container(
                        row![
                            ui::body(s.no_fallback_libero)
                                .color(c.muted)
                                .width(Length::Fill),
                            ui::action(
                                theme::icons::FORWARD_ARROW,
                                s.go_on,
                                Message::Skip,
                                true,
                                c
                            ),
                        ]
                        .spacing(10)
                        .align_y(Alignment::Center),
                    )
                    .padding(iced::Padding::from([0, 4]))
                    .into(),
                    _ => space::vertical().height(0).into(),
                };
                column![
                    skip,
                    scrollable(Column::with_children(rows).spacing(3)).height(Length::Fill)
                ]
                .spacing(12)
                .height(Length::Fill)
                .into()
            }
        }
    }

    fn lineup_view<'a>(&'a self, s: &'static Strings, c: Colors) -> Element<'a, Message> {
        let l = &self.lineup;
        let bold = theme::fonts().bold;
        let setter = l.setter.as_ref().map(|p| p.id.clone());
        let cell = |position: u8| {
            let player = l
                .positions
                .get(usize::from(position.saturating_sub(1)))
                .cloned()
                .flatten();
            let current = self.step == Step::Position(position);
            let content: Element<'a, Message> = match player {
                Some(p) => column![
                    text(p.number.to_string())
                        .size(30)
                        .font(bold)
                        .color(c.accent),
                    ui::body(match setter.as_deref() == Some(p.id.as_str()) {
                        true => format!("▲ {}", names::fitted(&p.name, NAME_ROOM)),
                        false => names::fitted(&p.name, NAME_ROOM),
                    }),
                ]
                .align_x(Alignment::Center)
                .into(),
                None => text("–").size(30).color(c.muted).into(),
            };
            // a click chooses its player (again)
            button(
                container(
                    column![
                        text(format!("P{position}")).size(SIZE_SMALL).color(c.muted),
                        content
                    ]
                    .spacing(4)
                    .align_x(Alignment::Center),
                )
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(Alignment::Center)
                .align_y(Alignment::Center),
            )
            .padding(0)
            .width(Length::Fill)
            .height(Length::Fill)
            .on_press(Message::Position(position))
            .style(move |_, status| button::Style {
                background: Some(
                    match status {
                        button::Status::Hovered | button::Status::Pressed => {
                            iced::Color { a: 0.8, ..c.court }
                        }
                        _ => c.court,
                    }
                    .into(),
                ),
                text_color: c.text,
                border: iced::Border {
                    color: match current {
                        true => c.accent,
                        false => c.court_lines,
                    },
                    width: match current {
                        true => 3.0,
                        false => 1.0,
                    },
                    radius: 6.0.into(),
                },
                ..button::Style::default()
            })
            .into()
        };
        let court =
            column(COURT.map(|line| row(line.map(cell)).spacing(8).height(Length::Fill).into()))
                .spacing(8)
                .width(Length::Fill)
                .height(Length::Fill);
        let named = |p: &Option<LineupPlayer>| {
            p.as_ref()
                .map_or_else(|| "–".to_string(), |p| format!("{} {}", p.number, p.name))
        };
        let serving = match l.serving_team {
            Some(TeamSideEnum::Us) => l.team.clone(),
            Some(TeamSideEnum::Them) => l.opponent.clone(),
            None => "–".to_string(),
        };
        let line = |label: &str, value: String, current: bool| {
            row![
                text(label.to_string())
                    .size(SIZE_SMALL)
                    .color(c.muted)
                    .width(Length::Fixed(190.0)),
                text(value).size(SIZE_BODY).font(bold).color(match current {
                    true => c.accent,
                    false => c.text,
                }),
            ]
            .align_y(Alignment::Center)
        };
        let fallback: Element<'a, Message> = match l.allows_fallback_libero {
            true => line(
                s.fallback_libero,
                named(&l.fallback_libero),
                self.step == Step::FallbackLibero,
            )
            .into(),
            false => space::vertical().height(0).into(),
        };
        let header = row![
            text(l.team.clone()).size(SIZE_BODY).font(bold),
            text(format!("– {}", l.opponent))
                .size(SIZE_BODY)
                .color(c.muted),
        ]
        .spacing(6);
        ui::panel(
            Some(&format!("{} · {} {}", s.lineup, s.set_n, l.set_number)),
            column![
                container(header)
                    .width(Length::Fill)
                    .align_x(Alignment::Center),
                court,
                column![
                    line(s.serving_first, serving, self.step == Step::Serving),
                    line(s.setter, named(&l.setter), self.step == Step::Setter),
                    line(s.libero, named(&l.libero), self.step == Step::Libero),
                    fallback,
                ]
                .spacing(4),
            ]
            .spacing(12),
            c,
        )
    }
}

#[cfg(test)]
mod tests {
    use {
        super::*,
        chrono::DateTime,
        scout4all_core::{
            match_form::MatchForm,
            player_form::PlayerForm,
            shapes::{
                country::CountryCode,
                enums::{GenderEnum, TeamClassificationEnum},
            },
            team_form::TeamForm,
        },
        scout4all_mobile::{NextSetKind, Scout4All},
    };

    /// A team of seven, the seventh a libero, and a new match.
    fn new_match(app: &Scout4All) -> (String, String) {
        let team = app
            .save_team(
                None,
                TeamForm {
                    name: "Cortona".into(),
                    classification: Some(TeamClassificationEnum::Regional),
                    gender: Some(GenderEnum::Women),
                    country: CountryCode::known("IT"),
                    year: "2026".into(),
                },
            )
            .unwrap();
        for n in 1..=7 {
            let role = match n {
                1 => RoleEnum::Setter,
                7 => RoleEnum::Libero,
                _ => RoleEnum::OutsideHitter,
            };
            let player = PlayerForm {
                name: format!("Anna P{n}"),
                role: Some(role),
                number: n.to_string(),
                birth_date: String::new(),
                height_cm: String::new(),
                notes: String::new(),
                person: None,
            };
            app.save_player(team.clone(), None, player, false).unwrap();
        }
        let form = MatchForm {
            opponent: "Arezzo".into(),
            date: DateTime::parse_from_rfc3339("2026-10-24T18:00:00+02:00").ok(),
            home: true,
            max_substitutions: "6".into(),
            ..MatchForm::default()
        };
        let m = app.save_match(team.clone(), None, form).unwrap();
        (team, m)
    }

    fn press(screen: &mut StartScreen, ck: KeyCombination) -> Outcome {
        screen.key(ck)
    }

    #[test]
    fn the_set_starts_from_the_keys_and_esc_undoes() {
        let dir = tempfile::tempdir().unwrap();
        let app = Scout4All::new(dir.path().to_string_lossy().into_owned(), "it".into()).unwrap();
        let (team, m) = new_match(&app);
        let start = app.set_start(team.clone(), m.clone()).unwrap();
        let mut screen = StartScreen::new(start, team.clone(), m.clone());
        // the first set: who serves first, Enter only once chosen
        assert_eq!(screen.step, Step::Serving);
        press(&mut screen, key!(enter));
        assert_eq!(screen.step, Step::Serving);
        press(&mut screen, key!(right));
        assert_eq!(screen.lineup.serving_team, Some(TeamSideEnum::Us));
        press(&mut screen, key!(enter));
        assert_eq!(screen.step, Step::Position(1));
        // the six positions, the first player each time
        for _ in 1..=6 {
            press(&mut screen, key!(enter));
        }
        assert_eq!(screen.step, Step::Setter);
        // Esc undoes the last position
        press(&mut screen, BACK_KEY);
        assert_eq!(screen.step, Step::Position(6));
        assert_eq!(screen.lineup.positions.get(5).cloned().flatten(), None);
        press(&mut screen, key!(enter));
        // the setter proposed by role, the libero the only one left
        assert_eq!(
            screen.choices.get(screen.selected).map(|p| p.number),
            Some(1)
        );
        press(&mut screen, key!(enter));
        assert_eq!(screen.step, Step::Libero);
        assert_eq!(
            screen.choices.iter().map(|p| p.number).collect::<Vec<_>>(),
            vec![7]
        );
        // seven players: no fallback libero, the set starts
        assert!(matches!(press(&mut screen, key!(enter)), Outcome::Start));
        assert!(screen.lineup.complete);
        app.start_set(screen.set_start()).unwrap();
        let details = app.match_details(team, m).unwrap();
        assert_eq!(details.next, NextSetKind::Resume { set_number: 1 });
    }

    #[test]
    fn a_position_clicked_is_chosen_again_then_the_choices_go_on() {
        let dir = tempfile::tempdir().unwrap();
        let app = Scout4All::new(dir.path().to_string_lossy().into_owned(), "it".into()).unwrap();
        let (team, m) = new_match(&app);
        let start = app.set_start(team.clone(), m.clone()).unwrap();
        let mut screen = StartScreen::new(start, team, m);
        press(&mut screen, key!(right));
        press(&mut screen, key!(enter));
        for _ in 1..=6 {
            press(&mut screen, key!(enter));
        }
        assert_eq!(screen.step, Step::Setter);
        // P3 clicked: its player chosen again, then the setter again
        screen.update(Message::Position(3));
        assert_eq!(screen.step, Step::Position(3));
        let other = screen
            .choices
            .iter()
            .find(|p| p.number == 7)
            .map(|p| p.id.clone())
            .unwrap();
        screen.update(Message::Choose(other));
        assert_eq!(screen.step, Step::Setter);
        assert_eq!(
            screen
                .lineup
                .positions
                .get(2)
                .cloned()
                .flatten()
                .map(|p| p.number),
            Some(7)
        );
        // clicked, then Esc: back to the setter, nothing changed
        screen.update(Message::Position(1));
        press(&mut screen, BACK_KEY);
        assert_eq!(screen.step, Step::Setter);
        // no position clicked while who serves is chosen
        let mut screen = StartScreen::new(
            app.set_start(screen.team_id.clone(), screen.match_id.clone())
                .unwrap(),
            screen.team_id.clone(),
            screen.match_id.clone(),
        );
        screen.update(Message::Position(2));
        assert_eq!(screen.step, Step::Serving);
    }

    #[test]
    fn esc_on_the_first_choice_goes_back() {
        let dir = tempfile::tempdir().unwrap();
        let app = Scout4All::new(dir.path().to_string_lossy().into_owned(), "en".into()).unwrap();
        let (team, m) = new_match(&app);
        let start = app.set_start(team.clone(), m.clone()).unwrap();
        let mut screen = StartScreen::new(start, team, m);
        press(&mut screen, key!(left));
        press(&mut screen, key!(enter));
        assert_eq!(screen.step, Step::Position(1));
        press(&mut screen, BACK_KEY);
        assert_eq!(screen.step, Step::Serving);
        assert!(matches!(press(&mut screen, BACK_KEY), Outcome::Back));
    }
}
