//! The scouting of a set: the choices of the step with their keys on the
//! left, the court in the middle with the last events, the live statistics
//! on the right. The keyboard chooses as in the TUI (the event, the player
//! by its digit, the evaluation), the mouse as an alternative.

use {
    crate::{
        charts::{self, Trend},
        court::{Content, Court, Touch},
        keymap,
        strings::Strings,
        theme::{self, Colors, SIZE_BODY, SIZE_SCORE, SIZE_SMALL, grade_color},
        ui,
    },
    iced::{
        Alignment, Color, Element, Length, Padding,
        widget::{Column, canvas, column, container, row, scrollable, space, text},
    },
    scout4all_core::{
        scouting::{PlayerChoice, Progress, ScoutingError, ScoutingSession, ScoutingStep},
        shapes::{
            court_zone::{
                CELLS, CourtCell, CourtSide, CourtTarget, CourtZoneEnum, Direction, ServeStartEnum,
                TargetKind,
            },
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
        ServeStartChoice, StatsScope, TechniqueChoice,
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
    Series(usize),
    Dismiss,
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
    /// for our serve, the zones it starts from, below their court
    starts: Vec<ServeStartChoice>,
    stats: Option<LiveStats>,
}

pub struct ScoutingScreen {
    /// the team and the match, to go back to the match
    pub team_id: String,
    pub match_id: String,
    scouting: Arc<Scouting>,
    keys: ScoutingKeys,
    snapshot: Snapshot,
    /// the cursor of the arrows on the court where the ball went
    target_cursor: Option<CourtCell>,
    net_cursor: Option<CourtZoneEnum>,
    /// the cursor of the arrows on the zones our serve starts from, below
    /// the net: there instead of on the court
    start_cursor: Option<ServeStartEnum>,
    /// an error, or that the set is over: any key closes it
    notice: Option<(String, bool)>,
    set_over: bool,
    show_stats: bool,
    match_scope: bool,
    recent: bool,
    series: usize,
    /// the trend of the series chosen, as the snapshot has it
    trend: Trend,
    /// how long reading the snapshot took last, in milliseconds
    pub refresh_ms: u128,
}

impl ScoutingScreen {
    pub fn new(
        scouting: Arc<Scouting>,
        keys: ScoutingKeys,
        team_id: String,
        match_id: String,
    ) -> Self {
        let snapshot = read(&scouting, false);
        let trend = Trend::of_stats(snapshot.stats.as_ref(), 0, false);
        Self {
            team_id,
            match_id,
            scouting,
            keys,
            snapshot,
            target_cursor: None,
            net_cursor: None,
            start_cursor: None,
            notice: None,
            set_over: false,
            show_stats: true,
            match_scope: false,
            recent: false,
            series: 0,
            trend,
            refresh_ms: 0,
        }
    }

    /// The keys as changed in the settings.
    pub fn set_keys(&mut self, keys: ScoutingKeys) {
        self.keys = keys;
    }

    fn refresh(&mut self) {
        let started = std::time::Instant::now();
        self.snapshot = read(&self.scouting, self.match_scope);
        self.trend = Trend::of_stats(self.snapshot.stats.as_ref(), self.series, self.recent);
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

    pub fn update(&mut self, message: Message) -> Outcome {
        let mobile = |r: Result<Progress, MobileError>| r.map_err(|e| e.to_string());
        let outcome = match message {
            Message::Dismiss => return self.dismiss(),
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
                // as the key of the evaluation: the next event from scratch
                self.target_cursor = None;
                self.net_cursor = None;
                self.start_cursor = None;
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
            Message::Court(Touch::Start(start)) => {
                let r = mobile(
                    self.scouting
                        .touch_serve_start(start)
                        .map(|_| Progress::Choosing),
                );
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
                self.start_cursor = None;
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
            Message::Series(s) => {
                self.series = s;
                Outcome::Stay
            }
        };
        self.refresh();
        outcome
    }

    fn dismiss(&mut self) -> Outcome {
        self.notice = None;
        match self.set_over {
            true => Outcome::Leave,
            false => {
                self.refresh();
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
    pub fn key(&mut self, ck: KeyCombination) -> Outcome {
        if self.notice.is_some() {
            return self.dismiss();
        }
        let step = self.with(|s, _| s.step());
        match (ck, step) {
            (k, _) if k == BACK_KEY => return Outcome::Leave,
            (k, _) if k == UNDO_KEY => return self.update(Message::Undo),
            (k, _) if k == STATS_KEY => self.show_stats = !self.show_stats,
            (_, ScoutingStep::Event) => self.event_key(ck),
            (_, ScoutingStep::Player) => self.player_key(ck),
            (_, ScoutingStep::Eval) => self.eval_key(ck),
            (_, ScoutingStep::Replacement) => self.replacement_key(ck),
        }
        self.refresh();
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
        if let Some(r) = self.start_key(ck, direction, chosen) {
            let r = self.core(r);
            self.show(r);
            return;
        }
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
                    self.start_cursor = None;
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

    /// The arrows and the space bar on the zones our serve starts from,
    /// below the net: down from the row at the net there, up back to it,
    /// across them, the space bar tells the one of the cursor. None when
    /// the key is for the court or isn't one of these.
    fn start_key(
        &mut self,
        ck: KeyCombination,
        direction: Option<Direction>,
        chosen: Option<CourtTarget>,
    ) -> Option<Result<Progress, ScoutingError>> {
        let applies = self.with(|s, _| s.serve_start_applies());
        let at_net = self.cursor(chosen);
        let bottom = at_net.place().0 + 1 == CELLS;
        match (applies, self.start_cursor, direction, ck == ZONE_KEY) {
            (false, ..) => {
                self.start_cursor = None;
                None
            }
            (true, Some(at), Some(Direction::Up), _) => {
                self.start_cursor = None;
                self.target_cursor = CourtCell::at(CELLS - 1, at.column());
                Some(Ok(Progress::Choosing))
            }
            (true, Some(at), Some(d), _) => {
                self.start_cursor = Some(at.moved(d));
                Some(Ok(Progress::Choosing))
            }
            (true, Some(at), None, true) => {
                Some(self.with(|s, _| s.touch_serve_start(at).map(|_| Progress::Choosing)))
            }
            (true, None, Some(Direction::Down), _) if bottom => {
                self.start_cursor = Some(ServeStartEnum::below(at_net.place().1));
                Some(Ok(Progress::Choosing))
            }
            _ => None,
        }
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
        starts: scouting.serve_starts(),
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

fn small<'a>(s: impl ToString, c: Colors) -> iced::widget::Text<'a> {
    text(s.to_string()).size(SIZE_SMALL).color(c.muted)
}

impl ScoutingScreen {
    /// The match, for the bar on top.
    pub fn title(&self) -> String {
        let st = &self.snapshot.state;
        format!("{} – {}", st.team, st.opponent)
    }

    /// The keys of the screen, for the bar at the bottom.
    pub fn hints(&self, s: &Strings) -> Vec<(String, String)> {
        vec![
            ("Esc".to_string(), s.back.to_string()),
            ("⇧U".to_string(), s.undo.to_string()),
            ("F2".to_string(), s.settings.to_string()),
            ("F3".to_string(), s.stats.to_string()),
        ]
    }

    pub fn view<'a>(&'a self, s: &'static Strings, c: Colors) -> Element<'a, Message> {
        let left = column![self.header(s, c), self.choices(s, c)]
            .spacing(14)
            .width(Length::Fixed(LEFT_WIDTH));
        // the last events once there are some: before the first, the court
        let last = (!self.snapshot.state.events.is_empty())
            .then(|| container(self.last_events(s, c)).height(Length::FillPortion(1)));
        let middle = column![]
            .push(self.action(s, c))
            .push(container(self.court(c)).height(Length::FillPortion(3)))
            .push(last)
            .spacing(14)
            .width(Length::Fill);
        let body = match self.show_stats {
            true => row![
                left,
                middle,
                container(self.stats(s, c)).width(Length::Fixed(STATS_WIDTH))
            ],
            false => row![left, middle],
        }
        .spacing(14)
        .height(Length::Fill);
        match &self.notice {
            None => body.into(),
            Some((message, error)) => {
                let message = match self.set_over {
                    true => s.set_over.to_string(),
                    false => message.clone(),
                };
                let color = match error {
                    true => c.danger,
                    false => c.accent,
                };
                let banner = ui::list_row(
                    container(ui::body(message).color(color))
                        .width(Length::Fill)
                        .align_x(Alignment::Center),
                    true,
                    Some(Message::Dismiss),
                    c,
                );
                column![banner, body].spacing(10).into()
            }
        }
    }

    fn header<'a>(&'a self, s: &'static Strings, c: Colors) -> Element<'a, Message> {
        let st = &self.snapshot.state;
        let bold = theme::fonts().bold;
        let side = |name: String, score: u8, serving: bool, ours: bool| {
            let dot: Element<'a, Message> = match serving {
                true => text("●").size(SIZE_SMALL).color(c.accent).into(),
                false => space::horizontal().width(Length::Fixed(10.0)).into(),
            };
            row![
                dot,
                text(name)
                    .size(SIZE_BODY)
                    .font(bold)
                    .color(match ours {
                        true => c.text,
                        false => c.muted,
                    })
                    .width(Length::Fill),
                text(score.to_string())
                    .size(SIZE_SCORE)
                    .font(bold)
                    .color(match ours {
                        true => c.accent,
                        false => c.text,
                    }),
            ]
            .spacing(8)
            .align_y(Alignment::Center)
        };
        let info = row![
            ui::pill(
                &format!("{} {}", s.set_n, st.set_number),
                false,
                None::<Message>,
                c
            ),
            ui::pill(&format!("{}–{}", st.won, st.lost), false, None, c),
            ui::pill(&format!("⇄ {}", st.substitutions_left), false, None, c),
        ]
        .spacing(6)
        .wrap();
        // the phase at a glance: its color, its icon, its name; then the
        // rotation (P1: the setter in 1)
        let serving = st.serving_team == TeamSideEnum::Us;
        let (color, icon, name) = match serving {
            true => (c.break_point, theme::icons::BREAK, s.break_phase),
            false => (c.side_out, theme::icons::SIDE_OUT, s.side_out_phase),
        };
        let big = |t: String| {
            text(t)
                .size(theme::SIZE_TITLE + 2.0)
                .font(bold)
                .color(c.background)
        };
        let phase = container(
            row![
                ui::icon(icon, 26.0, c.background),
                big(name.to_string()).width(Length::Fill),
                ui::tip(big(st.rotation.clone()), s.rotation, c),
            ]
            .spacing(10)
            .align_y(Alignment::Center),
        )
        .padding(Padding::from([10, 14]))
        .width(Length::Fill)
        .style(move |_| container::Style {
            background: Some(color.into()),
            border: iced::Border {
                radius: 10.0.into(),
                ..iced::Border::default()
            },
            ..container::Style::default()
        });
        ui::panel(
            None,
            column![
                phase,
                side(
                    st.team.clone(),
                    st.score_us,
                    st.serving_team == TeamSideEnum::Us,
                    true
                ),
                side(
                    st.opponent.clone(),
                    st.score_them,
                    st.serving_team == TeamSideEnum::Them,
                    false
                ),
                info
            ]
            .spacing(6),
            c,
        )
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
        if !chosen.is_empty() {
            list = list.push(
                container(ui::body(chosen).color(c.accent))
                    .width(Length::Fill)
                    .align_x(Alignment::Center)
                    .padding(Padding::from([0, 0]).bottom(6)),
            );
        }
        let choice = |key: Option<String>, label: String, chosen: bool, message: Message| {
            ui::choice(key, ui::body(label), chosen, Some(message), c)
        };
        let group = |list: Column<'a, Message>, title: &str| {
            list.push(space::vertical().height(8))
                .push(ui::section(title, c))
        };
        // a choice in half the panel: the key in less room
        let half = |key: Option<String>, label: String, chosen: bool, message: Message| {
            let key: Element<'a, Message> = match key {
                Some(k) => ui::keycap(&k, c),
                None => space::horizontal().width(0).into(),
            };
            ui::list_row(
                row![
                    container(key).width(Length::Fixed(HALF_KEY)),
                    ui::body(label)
                ]
                .spacing(6)
                .align_y(Alignment::Center),
                chosen,
                Some(message),
                c,
            )
        };
        match st.step {
            ScoutingStep::Event => {
                for e in &snap.events {
                    let key = self.keys.scouting_keybindings.key_for(&e.value);
                    list = list.push(choice(
                        key.as_ref().map(keymap::label),
                        e.name.clone(),
                        false,
                        Message::Event(e.value),
                    ));
                }
                list = group(list, s.options);
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
                        Message::Option(ScoutingOptionEnum::InvertedP1),
                    ));
                }
                if st.reception_open {
                    list = list.push(choice(
                        option_key(ScoutingOptionEnum::Reception),
                        format!("{}: {}", s.reception, st.reception),
                        false,
                        Message::Option(ScoutingOptionEnum::Reception),
                    ));
                }
            }
            ScoutingStep::Player => {
                if st.team_choice {
                    list = list.push(choice(
                        Some(TEAM_KEY.to_string()),
                        s.team_fault.to_string(),
                        false,
                        Message::Team,
                    ));
                }
                for p in &snap.players {
                    list = list.push(ui::choice(
                        Some(p.index.to_string()),
                        player_label(p, c),
                        false,
                        Some(Message::Player(p.id.to_string())),
                        c,
                    ));
                }
            }
            ScoutingStep::Replacement => {
                for p in &snap.replacements {
                    list = list.push(ui::choice(
                        Some(replacement_key(p.index)),
                        player_label(p, c),
                        false,
                        Some(Message::Replacement(p.id.to_string())),
                        c,
                    ));
                }
            }
            ScoutingStep::Eval => {
                // the techniques and the blockers two by two: all seen, no
                // scrolling, even for a middle and its many attacks
                if !snap.techniques.is_empty() {
                    list = group(list, s.techniques);
                    list = list.push(two_by_two(
                        snap.techniques
                            .iter()
                            .map(|t| {
                                half(
                                    self.technique_key(&t.value).as_ref().map(keymap::label),
                                    t.name.clone(),
                                    t.chosen,
                                    Message::Technique(t.value),
                                )
                            })
                            .collect(),
                    ));
                }
                if !snap.blockers.is_empty() {
                    list = group(list, s.blockers);
                    // one key goes through them: once, before them all
                    list = list.push(
                        row![
                            ui::keycap(&keymap::label(&BLOCKERS_KEY), c),
                            iced::widget::Row::with_children(snap.blockers.iter().map(|b| {
                                let message = match b.chosen {
                                    true => Message::Blockers(None),
                                    false => Message::Blockers(Some(b.value)),
                                };
                                ui::pill(&b.name, b.chosen, Some(message), c)
                            }))
                            .spacing(6)
                            .wrap()
                            .vertical_spacing(6),
                        ]
                        .spacing(10)
                        .padding(iced::Padding::from([4, 10])),
                    );
                }
                // as in the app: the set or serve and the blockers first, then
                // the evaluation that saves the event, a row of buttons by
                // their symbol
                list = match snap.techniques.is_empty() && snap.blockers.is_empty() {
                    true => list,
                    false => group(list, s.eval),
                };
                list = list.push(self.evals(c));
                let asked = snap.state.event.as_ref().and_then(|e| s.target(e.value));
                if let (false, Some(asked)) = (snap.target.is_empty(), asked) {
                    list = group(list, asked);
                    list = list.push(
                        row![
                            ui::keycap("← ↑ ↓ →", c),
                            ui::keycap(&keymap::label(&ZONE_KEY), c),
                            ui::body(self.told(s)).color(c.accent),
                        ]
                        .spacing(6)
                        .align_y(Alignment::Center),
                    );
                }
            }
        }
        ui::lit_panel(
            Some(step_title),
            scrollable(list).height(Length::Fill),
            true,
            c,
        )
    }

    /// The evaluations in a row, by their symbol, as in the app: a click
    /// or the key saves the event; the key under the symbol when it's
    /// another, the name under the mouse.
    fn evals<'a>(&'a self, c: Colors) -> Element<'a, Message> {
        let bold = theme::fonts().bold;
        iced::widget::Row::with_children(self.snapshot.evals.iter().map(|e| {
            let key = self
                .keys
                .eval_keybindings
                .key_for(&e.value)
                .as_ref()
                .map(keymap::label)
                .filter(|k| *k != e.symbol);
            let face = column![
                text(e.symbol.clone())
                    .size(SIZE_SCORE * 0.75)
                    .font(bold)
                    .color(c.on_accent),
            ]
            .push(key.map(|k| text(k).size(SIZE_SMALL).font(bold).color(c.on_accent)))
            .align_x(Alignment::Center);
            let button = iced::widget::button(
                container(face)
                    .center_x(Length::Fill)
                    .center_y(Length::Fill),
            )
            .width(Length::Fill)
            .height(Length::Fixed(EVAL_HEIGHT))
            .padding(0)
            .on_press(Message::Eval(e.value))
            .style(move |_, status| {
                let hovered = matches!(
                    status,
                    iced::widget::button::Status::Hovered | iced::widget::button::Status::Pressed
                );
                iced::widget::button::Style {
                    background: Some(
                        match hovered {
                            true => theme::mix(c.accent, c.text, 0.25),
                            false => c.accent,
                        }
                        .into(),
                    ),
                    text_color: c.on_accent,
                    border: iced::Border {
                        radius: 10.0.into(),
                        ..iced::Border::default()
                    },
                    ..iced::widget::button::Style::default()
                }
            });
            container(ui::tip(button, &e.name, c))
                .width(Length::FillPortion(1))
                .into()
        }))
        .spacing(6)
        .padding(Padding::from([4, 0]))
        .into()
    }

    /// Ours about to serve: in position 1, before the serve is made.
    /// What's told of our serve or of where the ball went: "from 1 → 5A",
    /// "5A", "from 1"; empty when nothing is.
    fn told(&self, s: &'static Strings) -> String {
        let start = self
            .snapshot
            .starts
            .iter()
            .find(|z| z.chosen)
            .map(|z| ui::fill(s.serve_from, std::slice::from_ref(&z.label)));
        match (start, self.snapshot.target_told.clone()) {
            (Some(start), Some(target)) => format!("{start} → {target}"),
            (Some(told), None) | (None, Some(told)) => told,
            (None, None) => String::new(),
        }
    }

    fn server(&self) -> Option<&scout4all_mobile::CourtPlayer> {
        let st = &self.snapshot.state;
        match (st.serving_team, &st.last_touch, &st.event) {
            (TeamSideEnum::Us, None, None) => st.court.first(),
            _ => None,
        }
    }

    /// What is being scouted, over the court, in the color of the phase:
    /// the event and its player, big; the serve with the ball, the server
    /// by number and name; then what the court asks, if anything. Before
    /// the event, only ours about to serve: the serve and who.
    fn action<'a>(&'a self, s: &'static Strings, c: Colors) -> Option<Element<'a, Message>> {
        let st = &self.snapshot.state;
        let about_to_serve = self.server().and_then(|p| {
            let serve = self
                .snapshot
                .events
                .iter()
                .find(|e| e.value == EventTypeEnum::S)?;
            Some((serve.clone(), p.number, p.name.clone()))
        });
        let (event, player) = match (&st.event, about_to_serve) {
            (Some(e), _) => (
                e.clone(),
                st.player.as_ref().map(|p| (p.number, p.name.clone())),
            ),
            (None, Some((serve, number, name))) => (serve, Some((number, name))),
            (None, None) => return None,
        };
        let event = &event;
        let color = match st.serving_team {
            TeamSideEnum::Us => c.break_point,
            TeamSideEnum::Them => c.side_out,
        };
        let bold = theme::fonts().bold;
        let big = |t: String, color| {
            text(t)
                .size(theme::SIZE_TITLE + 4.0)
                .font(bold)
                .color(color)
        };
        let ball: Option<Element<'a, Message>> =
            (event.value == EventTypeEnum::S).then(|| ui::icon(theme::BALL, 26.0, color).into());
        let who: Element<'a, Message> = match player {
            Some((number, name)) => row![big(number.to_string(), color), big(name, c.text),]
                .spacing(8)
                .into(),
            None => big("?".to_string(), c.muted).into(),
        };
        let asked = match (st.step, self.snapshot.target.is_empty()) {
            (ScoutingStep::Eval, false) => s.target(event.value),
            _ => None,
        };
        let line = row![]
            .push(ball)
            .push(big(event.name.to_uppercase(), color))
            .push(who)
            .spacing(12)
            .align_y(Alignment::Center);
        let band = column![line]
            .push(asked.map(|a| text(a.to_string()).size(SIZE_BODY).color(c.muted)))
            .spacing(2);
        Some(
            container(band)
                .padding(Padding::from([8, 14]))
                .width(Length::Fill)
                .style(move |_| container::Style {
                    background: Some(Color { a: 0.14, ..color }.into()),
                    border: iced::Border {
                        color,
                        width: 2.0,
                        radius: 10.0.into(),
                    },
                    ..container::Style::default()
                })
                .into(),
        )
    }

    fn court<'a>(&'a self, c: Colors) -> Element<'a, Message> {
        let snap = &self.snapshot;
        let content = match (snap.state.step, snap.target.is_empty()) {
            (ScoutingStep::Eval, false) => {
                let chosen = self.scouting.with_session(|s, _| s.target());
                let at_net = snap.target.len() == 1;
                // no zone shown until the arrows move or one is chosen: the
                // choice is never made for the scout
                let cursor = match (at_net, self.net_cursor, self.target_cursor, chosen) {
                    _ if self.start_cursor.is_some() => None,
                    (_, None, None, None) => None,
                    (true, ..) => self.net_at(chosen).number().map(|z| (z, String::new())),
                    (false, ..) => {
                        let cell = self.cursor(chosen);
                        Some((cell.zone_number(), cell.subzone().to_string()))
                    }
                };
                Content::Target {
                    rows: snap.target.clone(),
                    ours: snap.target_ours,
                    cursor,
                    starts: snap.starts.clone(),
                    start_cursor: self.start_cursor,
                }
            }
            _ => Content::Players {
                server: self.server().map(|p| (p.id.clone(), c.break_point)),
                players: snap.state.court.clone(),
                pickable: snap.state.step == ScoutingStep::Player,
            },
        };
        // lit when a click on it chooses: a player, where the ball went
        let lit = match content {
            Content::Players { pickable, .. } => pickable,
            Content::Target { .. } => true,
        };
        let court = canvas(Court { content, colors: c })
            .width(Length::Fill)
            .height(Length::Fill);
        ui::lit_panel(None, iced::Element::from(court).map(Message::Court), lit, c)
    }

    fn last_events<'a>(&'a self, s: &'static Strings, c: Colors) -> Element<'a, Message> {
        let rows = self.snapshot.state.events.iter().map(|e| {
            let who = e
                .player
                .as_ref()
                .map(|p| {
                    format!(
                        "{} {}",
                        p.number,
                        scout4all_core::names::fitted(&p.name, EVENT_NAME_ROOM)
                    )
                })
                .unwrap_or_default();
            // our serve from where it started to where it went: "1→5A"
            let target = match (&e.serve_start, &e.target) {
                (Some(start), Some(target)) => Some(format!("{start}→{target}")),
                (Some(start), None) => Some(format!("{start}→")),
                (None, target) => target.clone(),
            };
            let extra = [e.technique.clone(), target, e.blockers.clone()]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(" ");
            row![
                text(e.symbol.clone().unwrap_or_default())
                    .size(SIZE_BODY)
                    .font(theme::fonts().bold)
                    .color(c.accent)
                    .width(Length::Fixed(20.0)),
                ui::body(e.name.clone()).width(Length::FillPortion(2)),
                ui::body(who).color(c.muted).width(Length::FillPortion(3)),
                small(extra, c).width(Length::FillPortion(2)),
            ]
            .spacing(8)
            .into()
        });
        ui::panel(
            Some(s.last_events),
            scrollable(Column::with_children(rows).spacing(4))
                .height(Length::Fill)
                .width(Length::Fill),
            c,
        )
    }

    fn stats<'a>(&'a self, s: &'static Strings, c: Colors) -> Element<'a, Message> {
        let scopes = container(
            row![
                ui::pill(
                    s.this_set,
                    !self.match_scope,
                    Some(Message::Scope(false)),
                    c
                ),
                ui::pill(
                    s.whole_match,
                    self.match_scope,
                    Some(Message::Scope(true)),
                    c
                ),
            ]
            .spacing(6),
        )
        .width(Length::Fill)
        .align_x(Alignment::Center);
        let Some(stats) = &self.snapshot.stats else {
            return ui::panel(Some(s.stats), scopes, c);
        };
        let graded = |v: Option<f64>, g: Option<f64>| {
            text(pct(v))
                .size(SIZE_BODY)
                .font(theme::fonts().bold)
                .color(g.map_or(c.text, |g| grade_color(g, c)))
                .width(Length::Fixed(56.0))
                .align_x(Alignment::End)
        };
        let num = |n: u32, w: f32| ui::body(n).width(Length::Fixed(w)).align_x(Alignment::End);
        let head = row![
            space::horizontal(),
            small(s.total, c)
                .width(Length::Fixed(40.0))
                .align_x(Alignment::End),
            small("#", c)
                .width(Length::Fixed(32.0))
                .align_x(Alignment::End),
            small("=", c)
                .width(Length::Fixed(32.0))
                .align_x(Alignment::End),
            small(s.efficiency, c)
                .width(Length::Fixed(56.0))
                .align_x(Alignment::End),
        ];
        let fundamentals = stats.fundamentals.iter().map(|f| {
            row![
                ui::body(f.name.clone()).width(Length::Fill),
                num(f.total, 40.0),
                num(f.perfect, 32.0),
                num(f.errors, 32.0),
                graded(f.efficiency, f.grade),
            ]
            .into()
        });
        let phase = |name: &str, p: &Option<scout4all_mobile::PhaseStats>| {
            row![
                ui::body(name).width(Length::Fill),
                small(
                    p.as_ref()
                        .map(|p| format!("{}/{}", p.won, p.rallies))
                        .unwrap_or_default(),
                    c
                ),
                graded(
                    p.as_ref().map(|p| p.percentage),
                    p.as_ref().and_then(|p| p.grade)
                ),
            ]
            .spacing(8)
            .align_y(Alignment::Center)
        };
        let distribution = row(stats.distribution.iter().map(|z| {
            column![
                small(z.zone, c),
                text(format!("{:.0}%", z.share))
                    .size(SIZE_BODY)
                    .font(theme::fonts().bold),
            ]
            .align_x(Alignment::Center)
            .width(Length::Fill)
            .into()
        }));
        let series =
            row(stats.trend.iter().enumerate().map(|(i, line)| {
                ui::pill(&line.name, i == self.series, Some(Message::Series(i)), c)
            }))
            .spacing(4)
            .wrap()
            .vertical_spacing(4);
        let modes = row![
            ui::pill(s.so_far, !self.recent, Some(Message::Recent(false)), c),
            ui::pill(s.last_touches, self.recent, Some(Message::Recent(true)), c),
        ]
        .spacing(4);
        let legend = row(stats
            .trend
            .first()
            .map(|l| l.sets.iter().map(|s| s.set_number).collect::<Vec<_>>())
            .unwrap_or_default()
            .into_iter()
            .map(|n| {
                text(format!("━ {} {n}", s.set_n))
                    .size(SIZE_SMALL)
                    .color(theme::set_color(n))
                    .into()
            }))
        .spacing(10);
        let chart: Element<'a, Message> = match self.trend.is_empty() {
            true => small(s.no_rallies, c).into(),
            false => charts::canvas_view(self.trend.clone(), c, Length::Fixed(130.0)),
        };
        let content = column![
            scopes,
            head,
            Column::with_children(fundamentals).spacing(3),
            space::vertical().height(4),
            phase(s.side_out, &stats.side_out),
            phase(s.break_point, &stats.break_points),
            space::vertical().height(4),
            distribution,
            space::vertical().height(4),
            ui::section(s.trend, c),
            series,
            modes,
            legend,
            chart,
        ]
        .spacing(6);
        ui::panel(Some(s.stats), scrollable(content).height(Length::Fill), c)
    }
}

/// A player to choose: the number in bold, the name, the role muted.
/// A player to choose: the number and the name (the role of the slot of
/// the lineup the core gives is no word for the scout).
/// The letters of a name in the last events, else abbreviated.
const EVENT_NAME_ROOM: usize = 14;

/// How wide the column of the score and the choices is.
const LEFT_WIDTH: f32 = 380.0;

/// How wide the key of a choice in half the panel is.
const HALF_KEY: f32 = 44.0;

/// The buttons of the evaluations: big, as in the app.
const EVAL_HEIGHT: f32 = 56.0;

/// The choices two by two, each half as wide.
fn two_by_two<'a>(choices: Vec<Element<'a, Message>>) -> Element<'a, Message> {
    let mut choices = choices.into_iter().peekable();
    let mut lines = Column::new().spacing(2);
    while choices.peek().is_some() {
        let line: Vec<Element<'a, Message>> = choices
            .by_ref()
            .take(2)
            .map(|choice| container(choice).width(Length::FillPortion(1)).into())
            .collect();
        let alone = line.len() < 2;
        let line = iced::widget::Row::with_children(line).spacing(2);
        lines = lines.push(match alone {
            true => line.push(space::horizontal().width(Length::FillPortion(1))),
            false => line,
        });
    }
    lines.into()
}

fn player_label<'a>(p: &PlayerChoice, c: Colors) -> Element<'a, Message> {
    row![
        text(p.number.to_string())
            .size(SIZE_BODY)
            .font(theme::fonts().bold)
            .color(c.accent)
            .width(Length::Fixed(28.0)),
        ui::name(p.name.clone()),
    ]
    .spacing(6)
    .align_y(Alignment::Center)
    .into()
}

/// Keeps the width of the statistics: wide enough for the table and the
/// chart, the court takes the rest.
pub const STATS_WIDTH: f32 = 400.0;

#[cfg(test)]
mod tests {
    use {
        super::*,
        crate::{
            stats::tests::{match_against, team_with_two_matches},
            strings::strings,
        },
        scout4all_core::{scouting::ChoiceOrder, shapes::enums::LanguageEnum},
        scout4all_mobile::Scout4All,
    };

    /// Where our serve starts from, from the keyboard: down from the row
    /// at the net onto the zones below it, across them, the space bar
    /// tells one, up back to the court; saved with the serve, then
    /// remembered for the server.
    #[test]
    fn the_arrows_reach_where_the_serve_starts_from() {
        let s = strings(LanguageEnum::It);
        let dir = tempfile::tempdir().unwrap();
        let core = Scout4All::new(dir.path().to_string_lossy().into_owned(), "it".into()).unwrap();
        let team = team_with_two_matches(&core);
        // three aces: we still serve
        let arezzo = match_against(&core, &team, "Arezzo");
        let scouting = core
            .scouting_in_order(&team, &arezzo, ChoiceOrder::EventFirst)
            .unwrap();
        let mut screen = ScoutingScreen::new(scouting, ScoutingKeys::default(), team, arezzo);
        let chosen = |screen: &ScoutingScreen| {
            screen
                .snapshot
                .starts
                .iter()
                .find(|z| z.chosen)
                .map(|z| z.value)
        };
        screen.key(key!(s));
        let labels: Vec<&str> = screen
            .snapshot
            .starts
            .iter()
            .map(|z| z.label.as_str())
            .collect();
        assert_eq!(labels, vec!["5", "7", "6", "9", "1"]);
        // remembered from the aces before: none told
        assert_eq!(chosen(&screen), None);
        // from the middle of the court down to the net, then below it
        for _ in 0..3 {
            screen.key(key!(down));
        }
        assert_eq!(screen.start_cursor, None);
        screen.key(key!(down));
        assert_eq!(screen.start_cursor, Some(ServeStartEnum::Z6));
        screen.key(key!(right));
        screen.key(ZONE_KEY);
        assert_eq!(chosen(&screen), Some(ServeStartEnum::Z9));
        assert_eq!(screen.told(s), "da 9");
        // up: back on the court, above it; down again: on it
        screen.key(key!(up));
        assert_eq!(screen.start_cursor, None);
        assert_eq!(
            screen.target_cursor.map(|c| c.to_string()),
            Some("4B".into())
        );
        screen.key(key!(down));
        assert_eq!(screen.start_cursor, Some(ServeStartEnum::Z9));
        for _ in 0..5 {
            screen.key(key!(left));
        }
        assert_eq!(screen.start_cursor, Some(ServeStartEnum::Z5));
        // the space bar tells 5 instead of 9; again on it, takes it back
        screen.key(ZONE_KEY);
        assert_eq!(chosen(&screen), Some(ServeStartEnum::Z5));
        screen.key(ZONE_KEY);
        assert_eq!(chosen(&screen), None);
        screen.key(ZONE_KEY);
        assert_eq!(chosen(&screen), Some(ServeStartEnum::Z5));
        screen.update(Message::Eval(EvalEnum::Positive));
        screen.refresh();
        assert_eq!(screen.start_cursor, None);
        assert_eq!(
            screen
                .snapshot
                .state
                .events
                .first()
                .and_then(|e| e.serve_start.clone()),
            Some("5".into())
        );
        // their error: the same server again, from 5
        screen.key(key!(shift - e));
        screen.key(key!(s));
        assert_eq!(chosen(&screen), Some(ServeStartEnum::Z5));
        // a click on the zones below the court
        screen.update(Message::Court(Touch::Start(ServeStartEnum::Z1)));
        screen.refresh();
        assert_eq!(chosen(&screen), Some(ServeStartEnum::Z1));
    }
}
