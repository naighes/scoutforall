//! The scouting of a set: the choices of the step with their keys on the
//! left, the court in the middle with the last events, the live statistics
//! on the right. The keyboard chooses as in the TUI (the event, the player
//! by its digit, the evaluation), the mouse as an alternative.

use {
    crate::{
        charts::{self, AkselTrend, Trend},
        court::{Content, Court, Touch},
        keymap,
        strings::Strings,
        theme::{self, Colors, grade_color},
    },
    iced::{
        Alignment, Color, Element, Length, Padding,
        widget::{Column, button, canvas, column, container, row, scrollable, space, text},
    },
    scout4all_core::{
        scouting::{PlayerChoice, Progress, ScoutingError, ScoutingSession, ScoutingStep},
        shapes::{
            court_zone::{CourtCell, CourtSide, CourtTarget, CourtZoneEnum, Direction, TargetKind},
            enums::{EvalEnum, EventTypeEnum, TeamSideEnum},
            flags::BlockersEnum,
            technique::TechniqueEnum,
        },
    },
    scout4all_keys::{
        BACK_KEY, BLOCKERS_KEY, KeyCombination, STATS_KEY, ScoutingKeys, ScoutingOptionEnum,
        UNDO_KEY, ZONE_KEY, key,
    },
    scout4all_mobile::{
        BlockersChoice, EvalChoice, EventChoice, LiveStats, MobileError, Scouting, ScoutingState,
        StatsScope, TechniqueChoice,
    },
    std::sync::Arc,
    tokio::runtime::Runtime,
};

/// The digit that chooses the team, for our fault, as in the TUI.
const TEAM_KEY: u8 = 0;

#[derive(Debug, Clone)]
pub enum Message {
    Event(EventTypeEnum),
    Option(ScoutingOptionEnum),
    Player(String),
    Team,
    Replacement(String),
    Eval(EvalEnum),
    Technique(TechniqueEnum),
    Blockers(Option<BlockersEnum>),
    Court(Touch),
    Undo,
    Scope(bool),
    Recent(bool),
    Chart(ChartLib),
    Series(usize),
    Dismiss,
}

/// The library the trend is drawn with, to compare them in the prototype.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChartLib {
    Canvas,
    Aksel,
}

/// What the screen does after a message or a key.
pub enum Outcome {
    Stay,
    /// back home: Esc, or the set over
    Leave,
}

/// What the screen shows, read once after each choice: the view only
/// draws it.
struct Snapshot {
    state: ScoutingState,
    events: Vec<EventChoice>,
    players: Vec<PlayerChoice>,
    replacements: Vec<PlayerChoice>,
    evals: Vec<EvalChoice>,
    techniques: Vec<TechniqueChoice>,
    blockers: Vec<BlockersChoice>,
    target: Vec<Vec<scout4all_mobile::CourtCellChoice>>,
    target_ours: bool,
    target_told: Option<String>,
    stats: Option<LiveStats>,
}

pub struct ScoutingScreen {
    scouting: Arc<Scouting>,
    keys: ScoutingKeys,
    snapshot: Snapshot,
    /// the cursor of the arrows on the court where the ball went
    target_cursor: Option<CourtCell>,
    net_cursor: Option<CourtZoneEnum>,
    /// an error, or that the set is over: any key closes it
    notice: Option<(String, bool)>,
    set_over: bool,
    show_stats: bool,
    match_scope: bool,
    recent: bool,
    chart: ChartLib,
    series: usize,
    aksel_state: iced_aksel::State<&'static str, f64>,
    aksel_data: AkselTrend,
    /// how long reading the snapshot took last, in milliseconds
    pub refresh_ms: u128,
}

impl ScoutingScreen {
    pub fn new(scouting: Arc<Scouting>, keys: ScoutingKeys, colors: Colors) -> Self {
        let snapshot = read(&scouting, false);
        let trend = Trend::of_stats(snapshot.stats.as_ref(), 0, false);
        Self {
            scouting,
            keys,
            snapshot,
            target_cursor: None,
            net_cursor: None,
            notice: None,
            set_over: false,
            show_stats: true,
            match_scope: false,
            recent: false,
            chart: ChartLib::Canvas,
            series: 0,
            aksel_state: charts::aksel_state(&trend),
            aksel_data: AkselTrend { trend, colors },
            refresh_ms: 0,
        }
    }

    fn refresh(&mut self, colors: Colors) {
        let started = std::time::Instant::now();
        self.snapshot = read(&self.scouting, self.match_scope);
        let trend = Trend::of_stats(self.snapshot.stats.as_ref(), self.series, self.recent);
        self.aksel_state = charts::aksel_state(&trend);
        self.aksel_data = AkselTrend { trend, colors };
        self.refresh_ms = started.elapsed().as_millis();
    }

    /// Runs a choice on the session of the core, as the TUI does.
    fn with<R>(&self, f: impl FnOnce(&mut ScoutingSession, &Runtime) -> R) -> R {
        self.scouting.with_session(f)
    }

    /// Tells how the choice went: the error, or that the set is over.
    fn show(&mut self, progress: Result<Progress, String>) {
        match progress {
            Ok(Progress::SetOver) => {
                self.set_over = true;
                self.notice = Some((String::new(), false));
            }
            Ok(Progress::Choosing | Progress::Saved) => {}
            Err(e) => self.notice = Some((e, true)),
        }
    }

    fn core<T>(&self, r: Result<T, ScoutingError>) -> Result<T, String> {
        r.map_err(|e| self.scouting.message(&e))
    }

    pub fn update(&mut self, message: Message, colors: Colors) -> Outcome {
        let mobile = |r: Result<Progress, MobileError>| r.map_err(|e| e.to_string());
        let outcome = match message {
            Message::Dismiss => return self.dismiss(colors),
            Message::Event(e) => {
                let r = mobile(self.scouting.choose_event(e));
                self.show(r);
                Outcome::Stay
            }
            Message::Option(o) => {
                self.option(o);
                Outcome::Stay
            }
            Message::Player(id) => {
                let r = mobile(self.scouting.choose_player(id));
                self.show(r);
                Outcome::Stay
            }
            Message::Team => {
                let r = mobile(self.scouting.choose_team());
                self.show(r);
                Outcome::Stay
            }
            Message::Replacement(id) => {
                let r = mobile(self.scouting.choose_replacement(id));
                self.show(r);
                Outcome::Stay
            }
            Message::Eval(e) => {
                let r = mobile(self.scouting.choose_eval(e));
                self.show(r);
                Outcome::Stay
            }
            Message::Technique(t) => {
                let r = mobile(
                    self.scouting
                        .choose_technique(t)
                        .map(|_| Progress::Choosing),
                );
                self.show(r);
                Outcome::Stay
            }
            Message::Blockers(b) => {
                let r = mobile(self.scouting.choose_blockers(b).map(|_| Progress::Choosing));
                self.show(r);
                Outcome::Stay
            }
            Message::Court(Touch::Player(id)) => {
                let r = mobile(self.scouting.choose_player(id));
                self.show(r);
                Outcome::Stay
            }
            Message::Court(Touch::Cell(zone, subzone)) => {
                let r = mobile(
                    self.scouting
                        .touch_target(zone, subzone)
                        .map(|_| Progress::Choosing),
                );
                self.show(r);
                Outcome::Stay
            }
            Message::Undo => {
                let r = mobile(self.scouting.undo().map(|_| Progress::Choosing));
                self.target_cursor = None;
                self.net_cursor = None;
                self.show(r);
                Outcome::Stay
            }
            Message::Scope(m) => {
                self.match_scope = m;
                Outcome::Stay
            }
            Message::Recent(r) => {
                self.recent = r;
                Outcome::Stay
            }
            Message::Chart(c) => {
                self.chart = c;
                Outcome::Stay
            }
            Message::Series(s) => {
                self.series = s;
                Outcome::Stay
            }
        };
        self.refresh(colors);
        outcome
    }

    fn dismiss(&mut self, colors: Colors) -> Outcome {
        self.notice = None;
        match self.set_over {
            true => Outcome::Leave,
            false => {
                self.refresh(colors);
                Outcome::Stay
            }
        }
    }

    fn option(&mut self, option: ScoutingOptionEnum) {
        match option {
            ScoutingOptionEnum::InvertedP1 => self.with(|s, _| {
                let inverted = s.inverted_p1();
                s.set_inverted_p1(s.inverted_p1_applies() != inverted);
            }),
            ScoutingOptionEnum::Reception => {
                let told = self.with(|s, _| s.next_reception().map(|_| Progress::Choosing));
                let told = self.core(told);
                self.show(told);
            }
        }
    }

    /// A key pressed in the window.
    pub fn key(&mut self, ck: KeyCombination, colors: Colors) -> Outcome {
        if self.notice.is_some() {
            return self.dismiss(colors);
        }
        let step = self.with(|s, _| s.step());
        match (ck, step) {
            (k, _) if k == BACK_KEY => return Outcome::Leave,
            (k, _) if k == UNDO_KEY => return self.update(Message::Undo, colors),
            (k, _) if k == STATS_KEY => self.show_stats = !self.show_stats,
            (_, ScoutingStep::Event) => self.event_key(ck),
            (_, ScoutingStep::Player) => self.player_key(ck),
            (_, ScoutingStep::Eval) => self.eval_key(ck),
            (_, ScoutingStep::Replacement) => self.replacement_key(ck),
        }
        self.refresh(colors);
        Outcome::Stay
    }

    fn event_key(&mut self, ck: KeyCombination) {
        let option = self.keys.option_keybindings.action_for(&ck).copied();
        let event = self
            .keys
            .scouting_keybindings
            .action_for(&ck)
            .copied()
            .filter(|e| self.snapshot.events.iter().any(|c| c.value == *e));
        match (option, event) {
            (Some(option), _) => self.option(option),
            (None, Some(event)) => {
                let r = self.with(|s, rt| rt.block_on(s.choose_event(event)));
                let r = self.core(r);
                self.show(r);
            }
            (None, None) => {}
        }
    }

    fn player_key(&mut self, ck: KeyCombination) {
        let digit = keymap::digit(&ck);
        let player = digit.and_then(|d| self.snapshot.players.iter().find(|p| p.index == d));
        let r = match (digit, player.map(|p| p.id)) {
            (_, Some(id)) => self.with(|s, rt| rt.block_on(s.choose_player(id))),
            (Some(TEAM_KEY), None) => self.with(|s, rt| rt.block_on(s.choose_team())),
            (Some(_), None) => Err(ScoutingError::InvalidPlayerSelection),
            (None, None) => return,
        };
        let r = self.core(r);
        self.show(r);
    }

    fn replacement_key(&mut self, ck: KeyCombination) {
        let replacement = ck
            .as_letter()
            .and_then(replacement_index)
            .and_then(|i| self.snapshot.replacements.iter().find(|p| p.index == i))
            .map(|p| p.id);
        if let Some(id) = replacement {
            let r = self.with(|s, rt| rt.block_on(s.choose_replacement(id)));
            let r = self.core(r);
            self.show(r);
        }
    }

    /// Where the cursor on the court is: where moved, or on the cell
    /// chosen last, or in the middle.
    fn cursor(&self, chosen: Option<CourtTarget>) -> CourtCell {
        let chosen = match chosen {
            Some(CourtTarget::Cell(c) | CourtTarget::Pair(_, c)) => Some(c),
            Some(CourtTarget::Zone(_)) | None => None,
        };
        self.target_cursor.or(chosen).unwrap_or(CourtCell::CENTER)
    }

    fn net_at(&self, chosen: Option<CourtTarget>) -> CourtZoneEnum {
        let chosen = match chosen {
            Some(CourtTarget::Zone(z)) => CourtZoneEnum::numbered(z),
            Some(CourtTarget::Cell(_) | CourtTarget::Pair(_, _)) | None => None,
        };
        self.net_cursor.or(chosen).unwrap_or(CourtZoneEnum::Z3)
    }

    /// As the TUI: the arrows move on the court where the ball went, the
    /// space bar chooses there, Tab tells how many blocked, then the
    /// evaluation saves the event and a set or serve key changes it.
    fn eval_key(&mut self, ck: KeyCombination) {
        let (applies, kind, side, chosen, has_blockers) = self.with(|s, _| {
            (
                s.targets_apply(),
                s.target_kind(),
                s.target_side(),
                s.target(),
                !s.available_blockers().is_empty(),
            )
        });
        let net = kind == Some(TargetKind::Net);
        let direction = match ck {
            k if k == key!(up) && !net => Some(Direction::Up),
            k if k == key!(down) && !net => Some(Direction::Down),
            k if k == key!(left) => Some(Direction::Left),
            k if k == key!(right) => Some(Direction::Right),
            _ => None,
        }
        .filter(|_| applies);
        let eval = self
            .keys
            .eval_keybindings
            .action_for(&ck)
            .copied()
            .filter(|e| self.snapshot.evals.iter().any(|c| c.value == *e));
        let technique = self.technique_for(&ck);
        let r = match (
            ck == BLOCKERS_KEY && has_blockers,
            direction,
            applies && ck == ZONE_KEY,
        ) {
            (true, _, _) => self.with(|s, _| s.next_blockers().map(|_| Progress::Choosing)),
            (false, Some(d), _) if net => {
                let at = self.net_at(chosen);
                self.net_cursor = Some(at.moved_at_net(side.unwrap_or(CourtSide::Ours), d));
                Ok(Progress::Choosing)
            }
            (false, None, true) if net => {
                let at = self.net_at(chosen);
                self.net_cursor = Some(at);
                self.with(|s, _| s.touch_zone(at).map(|_| Progress::Choosing))
            }
            (false, Some(d), _) => {
                let at = self.cursor(chosen);
                self.target_cursor = Some(at.moved_on(side.unwrap_or(CourtSide::Theirs), d));
                Ok(Progress::Choosing)
            }
            (false, None, true) => {
                let at = self.cursor(chosen);
                self.target_cursor = Some(at);
                self.with(|s, _| s.touch_target(at).map(|_| Progress::Choosing))
            }
            (false, None, false) => match (eval, technique) {
                (Some(eval), _) => {
                    self.target_cursor = None;
                    self.net_cursor = None;
                    self.with(|s, rt| rt.block_on(s.choose_eval(eval)))
                }
                (None, Some(t)) => {
                    self.with(|s, _| s.choose_technique(t).map(|_| Progress::Choosing))
                }
                (None, None) => Ok(Progress::Choosing),
            },
        };
        let r = self.core(r);
        self.show(r);
    }

    /// The set or serve bound to the key, among the available ones.
    fn technique_for(&self, ck: &KeyCombination) -> Option<TechniqueEnum> {
        let set = self.keys.set_type_keybindings.action_for(ck).copied();
        let serve = self.keys.serve_type_keybindings.action_for(ck).copied();
        let event = self.snapshot.state.event.as_ref().map(|e| e.value);
        match event {
            Some(EventTypeEnum::A) => set.map(TechniqueEnum::Set),
            Some(EventTypeEnum::S) => serve.map(TechniqueEnum::Serve),
            _ => None,
        }
        .filter(|t| self.snapshot.techniques.iter().any(|c| c.value == *t))
    }

    fn technique_key(&self, technique: &TechniqueEnum) -> Option<KeyCombination> {
        match technique {
            TechniqueEnum::Set(set) => self.keys.set_type_keybindings.key_for(set),
            TechniqueEnum::Serve(serve) => self.keys.serve_type_keybindings.key_for(serve),
        }
    }
}

/// The index of the replacement of the key, as in the TUI: the digits,
/// then the letters.
fn replacement_index(c: char) -> Option<u8> {
    match c {
        '1'..='9' => c.to_digit(10).and_then(|d| u8::try_from(d).ok()),
        'a'..='z' => u8::try_from(c)
            .ok()
            .and_then(|b| b.checked_sub(b'a'))
            .map(|i| i.saturating_add(10)),
        _ => None,
    }
}

fn replacement_key(index: u8) -> String {
    match index {
        1..=9 => index.to_string(),
        _ => char::from(b'a'.saturating_add(index.saturating_sub(10))).to_string(),
    }
}

fn read(scouting: &Scouting, match_scope: bool) -> Snapshot {
    let state = scouting.state();
    let (players, replacements) =
        scouting.with_session(|s, _| (s.player_choices(), s.replacement_choices()));
    let scope = match match_scope {
        true => StatsScope::Match,
        false => StatsScope::Set {
            number: state.set_number,
        },
    };
    Snapshot {
        events: scouting.available_events(),
        players,
        replacements,
        evals: scouting.available_evals(),
        techniques: scouting.available_techniques(),
        blockers: scouting.available_blockers(),
        target: scouting.target_court(),
        target_ours: scouting.target_ours(),
        target_told: scouting.target_told(),
        stats: scouting.stats(scope).ok(),
        state,
    }
}

impl Trend {
    /// The trend of the series chosen, empty without statistics.
    pub fn of_stats(stats: Option<&LiveStats>, series: usize, recent: bool) -> Trend {
        stats
            .and_then(|s| s.trend.get(series))
            .map(|line| Trend::of(line, recent))
            .unwrap_or(Trend {
                sets: vec![],
                overall: None,
                band: None,
                low: 0.0,
                high: 10.0,
                points: 25.0,
            })
    }
}

// --- view ----------------------------------------------------------------

fn pct(v: Option<f64>) -> String {
    v.map_or_else(|| "–".to_string(), |v| format!("{v:.0}%"))
}

fn panel<'a>(content: impl Into<Element<'a, Message>>, c: Colors) -> Element<'a, Message> {
    container(content)
        .padding(12)
        .style(move |_| container::Style {
            background: Some(c.surface.into()),
            border: iced::Border {
                radius: 8.0.into(),
                width: 1.0,
                color: c.raised,
            },
            ..container::Style::default()
        })
        .into()
}

fn title<'a>(s: &str, c: Colors) -> Element<'a, Message> {
    text(s.to_uppercase()).size(12).color(c.muted).into()
}

/// A choice with its key: a button the mouse presses too.
fn choice<'a>(
    key: Option<String>,
    label: String,
    chosen: bool,
    message: Option<Message>,
    c: Colors,
) -> Element<'a, Message> {
    let key = container(text(key.unwrap_or_default()).size(13).color(c.accent))
        .width(Length::Fixed(44.0))
        .align_x(Alignment::Center);
    let body = row![key, text(label).size(15)]
        .spacing(8)
        .align_y(Alignment::Center);
    button(body)
        .width(Length::Fill)
        .padding(Padding::from([5, 6]))
        .on_press_maybe(message)
        .style(move |_, status| {
            let hover = matches!(status, button::Status::Hovered | button::Status::Pressed);
            button::Style {
                background: match (chosen, hover) {
                    (true, _) => Some(
                        Color {
                            a: 0.25,
                            ..c.accent
                        }
                        .into(),
                    ),
                    (false, true) => Some(c.raised.into()),
                    (false, false) => None,
                },
                text_color: c.text,
                border: iced::Border {
                    radius: 6.0.into(),
                    ..iced::Border::default()
                },
                ..button::Style::default()
            }
        })
        .into()
}

fn chip<'a>(label: &str, on: bool, message: Message, c: Colors) -> Element<'a, Message> {
    button(text(label.to_string()).size(12))
        .padding(Padding::from([3, 8]))
        .on_press(message)
        .style(move |_, _| button::Style {
            background: Some(
                match on {
                    true => Color { a: 0.3, ..c.accent },
                    false => c.raised,
                }
                .into(),
            ),
            text_color: c.text,
            border: iced::Border {
                radius: 12.0.into(),
                ..iced::Border::default()
            },
            ..button::Style::default()
        })
        .into()
}

impl ScoutingScreen {
    pub fn view<'a>(&'a self, s: &'static Strings, c: Colors) -> Element<'a, Message> {
        let left = column![self.header(s, c), self.choices(s, c)]
            .spacing(12)
            .width(Length::Fixed(320.0));
        let middle = column![self.court(c), self.last_events(s, c)]
            .spacing(12)
            .width(Length::Fill);
        let body = match self.show_stats {
            true => row![
                left,
                middle,
                container(self.stats(s, c)).width(Length::Fixed(STATS_WIDTH))
            ],
            false => row![left, middle],
        }
        .spacing(12)
        .padding(12)
        .height(Length::Fill);
        let footer = text(format!("Esc {}   ⇧U {}   F3 {}", s.back, s.undo, s.stats))
            .size(12)
            .color(c.muted);
        let page = column![body, container(footer).padding(Padding::from([0, 14]))];
        match &self.notice {
            None => page.into(),
            Some((message, error)) => {
                let message = match self.set_over {
                    true => s.set_over.to_string(),
                    false => message.clone(),
                };
                let color = match error {
                    true => theme::LOST,
                    false => c.accent,
                };
                let banner = button(text(message).size(15).color(color))
                    .padding(10)
                    .width(Length::Fill)
                    .on_press(Message::Dismiss)
                    .style(move |_, _| button::Style {
                        background: Some(c.raised.into()),
                        text_color: c.text,
                        ..button::Style::default()
                    });
                column![banner, page].into()
            }
        }
    }

    fn header<'a>(&'a self, s: &'static Strings, c: Colors) -> Element<'a, Message> {
        let st = &self.snapshot.state;
        let serving = |side: TeamSideEnum| match st.serving_team == side {
            true => format!(" ● {}", s.serving),
            false => String::new(),
        };
        let us = row![
            text(st.team.clone()).size(16),
            text(serving(TeamSideEnum::Us)).size(12).color(c.accent),
            space::horizontal(),
            text(st.score_us.to_string()).size(28),
        ]
        .align_y(Alignment::Center);
        let them = row![
            text(st.opponent.clone()).size(16).color(c.muted),
            text(serving(TeamSideEnum::Them)).size(12).color(c.accent),
            space::horizontal(),
            text(st.score_them.to_string()).size(28).color(c.muted),
        ]
        .align_y(Alignment::Center);
        let info = text(format!(
            "{} {} · {} {}–{} · {} {} · {} {}",
            s.set_n,
            st.set_number,
            s.whole_match,
            st.won,
            st.lost,
            s.rotation,
            st.rotation,
            s.substitutions,
            st.substitutions_left
        ))
        .size(12)
        .color(c.muted);
        panel(column![us, them, info].spacing(4), c)
    }

    fn choices<'a>(&'a self, s: &'static Strings, c: Colors) -> Element<'a, Message> {
        let snap = &self.snapshot;
        let st = &snap.state;
        let mut list = Column::new().spacing(2);
        let step_title = match st.step {
            ScoutingStep::Event => s.events,
            ScoutingStep::Player => s.player,
            ScoutingStep::Eval => s.eval,
            ScoutingStep::Replacement => s.replacement,
        };
        let chosen = [
            st.event.as_ref().map(|e| e.name.clone()),
            st.player
                .as_ref()
                .map(|p| format!("{} {}", p.number, p.name)),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" · ");
        list = list.push(title(step_title, c));
        if !chosen.is_empty() {
            list = list.push(text(chosen).size(14).color(c.accent));
        }
        match st.step {
            ScoutingStep::Event => {
                for e in &snap.events {
                    let key = self.keys.scouting_keybindings.key_for(&e.value);
                    list = list.push(choice(
                        key.as_ref().map(keymap::label),
                        e.name.clone(),
                        false,
                        Some(Message::Event(e.value)),
                        c,
                    ));
                }
                list = list
                    .push(space::vertical().height(8))
                    .push(title(s.options, c));
                let option_key = |o| {
                    self.keys
                        .option_keybindings
                        .key_for(&o)
                        .as_ref()
                        .map(keymap::label)
                };
                if st.inverted_p1_applies {
                    list = list.push(choice(
                        option_key(ScoutingOptionEnum::InvertedP1),
                        s.inverted_p1.to_string(),
                        st.inverted_p1,
                        Some(Message::Option(ScoutingOptionEnum::InvertedP1)),
                        c,
                    ));
                }
                if st.reception_open {
                    list = list.push(choice(
                        option_key(ScoutingOptionEnum::Reception),
                        format!("{}: {}", s.reception, st.reception),
                        false,
                        Some(Message::Option(ScoutingOptionEnum::Reception)),
                        c,
                    ));
                }
            }
            ScoutingStep::Player => {
                if st.team_choice {
                    list = list.push(choice(
                        Some(TEAM_KEY.to_string()),
                        s.team_fault.to_string(),
                        false,
                        Some(Message::Team),
                        c,
                    ));
                }
                for p in &snap.players {
                    list = list.push(choice(
                        Some(p.index.to_string()),
                        format!("{} {} · {}", p.number, p.name, p.role),
                        false,
                        Some(Message::Player(p.id.to_string())),
                        c,
                    ));
                }
            }
            ScoutingStep::Replacement => {
                for p in &snap.replacements {
                    list = list.push(choice(
                        Some(replacement_key(p.index)),
                        format!("{} {} · {}", p.number, p.name, p.role),
                        false,
                        Some(Message::Replacement(p.id.to_string())),
                        c,
                    ));
                }
            }
            ScoutingStep::Eval => {
                for e in &snap.evals {
                    let key = self.keys.eval_keybindings.key_for(&e.value);
                    let key = key.as_ref().map(keymap::label);
                    // the symbol once: in the key, unless the key is another
                    let name = match key.as_deref() == Some(e.symbol.as_str()) {
                        true => e.name.clone(),
                        false => format!("{}  {}", e.symbol, e.name),
                    };
                    list = list.push(choice(key, name, false, Some(Message::Eval(e.value)), c));
                }
                if !snap.techniques.is_empty() {
                    list = list
                        .push(space::vertical().height(8))
                        .push(title(s.techniques, c));
                    for t in &snap.techniques {
                        list = list.push(choice(
                            self.technique_key(&t.value).as_ref().map(keymap::label),
                            t.name.clone(),
                            t.chosen,
                            Some(Message::Technique(t.value)),
                            c,
                        ));
                    }
                }
                if !snap.blockers.is_empty() {
                    list = list
                        .push(space::vertical().height(8))
                        .push(title(s.blockers, c));
                    for b in &snap.blockers {
                        let message = match b.chosen {
                            true => Message::Blockers(None),
                            false => Message::Blockers(Some(b.value)),
                        };
                        list = list.push(choice(
                            Some(keymap::label(&BLOCKERS_KEY)),
                            b.name.clone(),
                            b.chosen,
                            Some(message),
                            c,
                        ));
                    }
                }
                if !snap.target.is_empty() {
                    list = list
                        .push(space::vertical().height(8))
                        .push(title(s.target, c));
                    list = list.push(
                        text(format!(
                            "← ↑ ↓ →  {}   {}",
                            keymap::label(&ZONE_KEY),
                            snap.target_told.clone().unwrap_or_default()
                        ))
                        .size(13)
                        .color(c.muted),
                    );
                }
            }
        }
        panel(scrollable(list).height(Length::Fill), c)
    }

    fn court<'a>(&'a self, c: Colors) -> Element<'a, Message> {
        let snap = &self.snapshot;
        let content = match (snap.state.step, snap.target.is_empty()) {
            (ScoutingStep::Eval, false) => {
                let chosen = self.scouting.with_session(|s, _| s.target());
                let at_net = snap.target.len() == 1;
                let cursor = match at_net {
                    true => self.net_at(chosen).number().map(|z| (z, String::new())),
                    false => {
                        let cell = self.cursor(chosen);
                        Some((cell.zone_number(), cell.subzone().to_string()))
                    }
                };
                Content::Target {
                    rows: snap.target.clone(),
                    ours: snap.target_ours,
                    cursor,
                }
            }
            _ => Content::Players(snap.state.court.clone()),
        };
        let court = canvas(Court { content, colors: c })
            .width(Length::Fill)
            .height(Length::Fill);
        panel(iced::Element::from(court).map(Message::Court), c)
    }

    fn last_events<'a>(&'a self, s: &'static Strings, c: Colors) -> Element<'a, Message> {
        let rows = self.snapshot.state.events.iter().take(8).map(|e| {
            let who = e
                .player
                .as_ref()
                .map(|p| format!("{} {}", p.number, p.name))
                .unwrap_or_default();
            let extra = [e.technique.clone(), e.target.clone(), e.blockers.clone()]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(" ");
            row![
                text(e.symbol.clone().unwrap_or_default())
                    .size(14)
                    .color(c.accent)
                    .width(Length::Fixed(18.0)),
                text(e.name.clone()).size(14).width(Length::FillPortion(2)),
                text(who)
                    .size(14)
                    .color(c.muted)
                    .width(Length::FillPortion(3)),
                text(extra)
                    .size(13)
                    .color(c.muted)
                    .width(Length::FillPortion(2)),
            ]
            .spacing(8)
            .into()
        });
        panel(
            column![
                title(s.last_events, c),
                Column::with_children(rows).spacing(3)
            ]
            .spacing(6)
            .height(Length::Fixed(220.0)),
            c,
        )
    }

    fn stats<'a>(&'a self, s: &'static Strings, c: Colors) -> Element<'a, Message> {
        let scopes = row![
            chip(s.this_set, !self.match_scope, Message::Scope(false), c),
            chip(s.whole_match, self.match_scope, Message::Scope(true), c),
        ]
        .spacing(6);
        let Some(stats) = &self.snapshot.stats else {
            return panel(column![title(s.stats, c), scopes].spacing(8), c);
        };
        let graded = |v: Option<f64>, g: Option<f64>| {
            text(pct(v))
                .size(14)
                .color(g.map_or(c.text, grade_color))
                .width(Length::Fixed(52.0))
        };
        let head = row![
            text("").width(Length::Fill),
            text(s.total)
                .size(12)
                .color(c.muted)
                .width(Length::Fixed(40.0)),
            text("#").size(12).color(c.muted).width(Length::Fixed(32.0)),
            text("=").size(12).color(c.muted).width(Length::Fixed(32.0)),
            text(s.efficiency)
                .size(12)
                .color(c.muted)
                .width(Length::Fixed(52.0)),
        ];
        let fundamentals = stats.fundamentals.iter().map(|f| {
            row![
                text(f.name.clone()).size(14).width(Length::Fill),
                text(f.total.to_string())
                    .size(14)
                    .width(Length::Fixed(40.0)),
                text(f.perfect.to_string())
                    .size(14)
                    .width(Length::Fixed(32.0)),
                text(f.errors.to_string())
                    .size(14)
                    .width(Length::Fixed(32.0)),
                graded(f.efficiency, f.grade),
            ]
            .into()
        });
        let phase = |name: &str, p: &Option<scout4all_mobile::PhaseStats>| {
            row![
                text(name.to_string()).size(14).width(Length::Fill),
                text(
                    p.as_ref()
                        .map(|p| format!("{}/{}", p.won, p.rallies))
                        .unwrap_or_default()
                )
                .size(13)
                .color(c.muted)
                .width(Length::Fixed(60.0)),
                graded(
                    p.as_ref().map(|p| p.percentage),
                    p.as_ref().and_then(|p| p.grade)
                ),
            ]
        };
        let distribution = row(stats.distribution.iter().map(|z| {
            column![
                text(z.zone.to_string()).size(12).color(c.muted),
                text(format!("{:.0}%", z.share)).size(14),
            ]
            .align_x(Alignment::Center)
            .width(Length::Fill)
            .into()
        }));
        let series = row(stats
            .trend
            .iter()
            .enumerate()
            .map(|(i, line)| chip(&line.name, i == self.series, Message::Series(i), c)))
        .spacing(4)
        .wrap();
        let modes = row![
            chip(s.so_far, !self.recent, Message::Recent(false), c),
            chip(s.last_touches, self.recent, Message::Recent(true), c),
            space::horizontal(),
            text(s.chart_with).size(12).color(c.muted),
            chip(
                "canvas",
                self.chart == ChartLib::Canvas,
                Message::Chart(ChartLib::Canvas),
                c
            ),
            chip(
                "aksel",
                self.chart == ChartLib::Aksel,
                Message::Chart(ChartLib::Aksel),
                c
            ),
        ]
        .spacing(4)
        .align_y(Alignment::Center);
        let legend = row(stats
            .trend
            .first()
            .map(|l| l.sets.iter().map(|s| s.set_number).collect::<Vec<_>>())
            .unwrap_or_default()
            .into_iter()
            .map(|n| {
                text(format!("━ {} {n}", s.set_n))
                    .size(12)
                    .color(theme::set_color(n))
                    .into()
            }))
        .spacing(10);
        let chart: Element<'a, Message> = match (self.aksel_data.trend.is_empty(), self.chart) {
            (true, _) => text(s.no_rallies).size(13).color(c.muted).into(),
            (false, ChartLib::Canvas) => charts::canvas_view(self.aksel_data.trend.clone(), c),
            (false, ChartLib::Aksel) => charts::aksel_view(&self.aksel_state, &self.aksel_data),
        };
        let content = column![
            title(s.stats, c),
            scopes,
            head,
            Column::with_children(fundamentals).spacing(2),
            space::vertical().height(6),
            phase(s.side_out, &stats.side_out),
            phase(s.break_point, &stats.break_points),
            space::vertical().height(6),
            distribution,
            space::vertical().height(6),
            title(s.trend, c),
            series,
            modes,
            legend,
            chart,
        ]
        .spacing(6);
        panel(scrollable(content).height(Length::Fill), c)
    }
}

/// Keeps the width of the statistics: wide enough for the table and the
/// chart, the court takes the rest.
pub const STATS_WIDTH: f32 = 380.0;
