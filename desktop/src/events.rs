//! The events of a set, to correct them, as in the app: the list with the
//! score after each (↑ ↓, the last ones in sight), Enter corrects the one
//! chosen. The correction beside it: its type, its evaluation, its set or
//! serve, each a line of choices (↑ ↓ among them, ← → change); Enter saves
//! if the events after it still fit and the set ends as it did, Esc
//! leaves it as it was. The changes of the lineup can't be corrected.

use {
    crate::{
        strings::Strings,
        theme::{self, Colors, SIZE_BODY, SIZE_SMALL},
        ui,
    },
    iced::{
        Alignment, Element, Length,
        widget::{Column, Id, column, container, row, scrollable, text},
    },
    scout4all_core::{
        event_edit::EditField,
        shapes::{
            enums::{EvalEnum, EventTypeEnum},
            technique::TechniqueEnum,
        },
    },
    scout4all_keys::{BACK_KEY, KeyCombination, key},
    scout4all_mobile::{Correction, CorrectionState, MobileError, Scout4All, SetEvent, SetEvents},
    std::sync::Arc,
};

/// The list, to keep the event chosen in sight.
pub const LIST: &str = "set-events";

#[derive(Debug, Clone, Copy, PartialEq)]
enum Line {
    EventType,
    Eval,
    Technique,
}

#[derive(Debug, Clone)]
pub enum Message {
    /// an event of the list: chosen and corrected
    Event(usize),
    EventType(EventTypeEnum),
    Eval(EvalEnum),
    Technique(Option<TechniqueEnum>),
    Save,
    Cancel,
}

pub enum Outcome {
    Stay,
    Back,
}

struct Correcting {
    correction: Arc<Correction>,
    state: CorrectionState,
    line: usize,
}

impl Correcting {
    fn lines(&self) -> Vec<Line> {
        [
            (EditField::EventType, Line::EventType),
            (EditField::Eval, Line::Eval),
            (EditField::Technique, Line::Technique),
        ]
        .into_iter()
        .filter(|(field, _)| self.state.fields.contains(field))
        .map(|(_, line)| line)
        .collect()
    }

    fn technique(&self) -> Option<TechniqueEnum> {
        self.state
            .techniques
            .iter()
            .find(|t| t.chosen)
            .map(|t| t.value)
    }

    /// The choice of the line the arrows are on, the next or the one
    /// before.
    fn change(&mut self, forward: bool) {
        let c = &self.correction;
        let s = &self.state;
        match self.lines().get(self.line) {
            Some(Line::EventType) => {
                let options: Vec<_> = s.event_types.iter().map(|e| e.value).collect();
                if let Some(e) = ui::cycle(&options, &s.event.event, forward) {
                    c.choose_event_type(e);
                }
            }
            Some(Line::Eval) => {
                let options: Vec<_> = s.evals.iter().map(|e| Some(e.value)).collect();
                if let Some(Some(e)) = ui::cycle(&options, &s.eval, forward) {
                    c.choose_eval(e);
                }
            }
            Some(Line::Technique) => {
                let options: Vec<_> = std::iter::once(None)
                    .chain(s.techniques.iter().map(|t| Some(t.value)))
                    .collect();
                if let Some(t) = ui::cycle(&options, &self.technique(), forward) {
                    c.choose_technique(t);
                }
            }
            None => {}
        }
        self.refresh();
    }

    /// The event as it is now, the arrows on a line still there.
    fn refresh(&mut self) {
        self.state = self.correction.state();
        self.line = self.line.min(self.lines().len().saturating_sub(1));
    }
}

pub struct EventsScreen {
    core: Arc<Scout4All>,
    pub team_id: String,
    pub match_id: String,
    set: SetEvents,
    selected: usize,
    correcting: Option<Correcting>,
    error: Option<String>,
    /// said once an event is saved
    saved: bool,
}

impl EventsScreen {
    pub fn new(
        core: Arc<Scout4All>,
        team_id: String,
        match_id: String,
        set_number: u8,
    ) -> Result<EventsScreen, MobileError> {
        let set = core.set_events(team_id.clone(), match_id.clone(), set_number)?;
        Ok(EventsScreen {
            core,
            team_id,
            match_id,
            // the last events first in sight: the ones just scouted
            selected: set.events.len().saturating_sub(1),
            set,
            correcting: None,
            error: None,
            saved: false,
        })
    }

    pub fn title(&self, s: &Strings) -> String {
        format!(
            "{} – {} · {} {}",
            self.set.team, self.set.opponent, s.set_n, self.set.set_number
        )
    }

    pub fn set_number(&self) -> u8 {
        self.set.set_number
    }

    /// Where the list is, to keep the event chosen in sight.
    pub fn scroll(&self) -> (Id, f32) {
        let last = self.set.events.len().saturating_sub(1).max(1);
        (Id::new(LIST), self.selected as f32 / last as f32)
    }

    fn correct(&mut self, index: usize) {
        self.selected = index.min(self.set.events.len().saturating_sub(1));
        self.error = None;
        self.saved = false;
        let Some(event) = self.set.events.get(self.selected) else {
            return;
        };
        if !event.editable {
            self.correcting = None;
            return;
        }
        let correction = self.core.correction(
            self.team_id.clone(),
            self.match_id.clone(),
            self.set.set_number,
            event.index,
        );
        match correction {
            Ok(correction) => {
                let state = correction.state();
                self.correcting = Some(Correcting {
                    correction,
                    state,
                    line: 0,
                });
            }
            Err(e) => self.error = Some(e.to_string()),
        }
    }

    fn save(&mut self) {
        let Some(correcting) = &self.correcting else {
            return;
        };
        let saved = self
            .core
            .save_correction(correcting.correction.clone())
            .and_then(|()| {
                self.core.set_events(
                    self.team_id.clone(),
                    self.match_id.clone(),
                    self.set.set_number,
                )
            });
        match saved {
            Ok(set) => {
                self.set = set;
                self.correcting = None;
                self.error = None;
                self.saved = true;
            }
            // the event stays as chosen, to change it again
            Err(e) => self.error = Some(e.to_string()),
        }
    }

    fn cancel(&mut self) {
        self.correcting = None;
        self.error = None;
    }

    pub fn key(&mut self, ck: KeyCombination) -> Outcome {
        let count = self.set.events.len();
        match &mut self.correcting {
            Some(correcting) => {
                let lines = correcting.lines().len().max(1);
                match ck {
                    k if k == BACK_KEY => self.cancel(),
                    k if k == key!(enter) => self.save(),
                    k if k == key!(down) || k == key!(tab) => {
                        correcting.line = (correcting.line + 1) % lines;
                    }
                    k if k == key!(up)
                        || k == key!(shift - tab)
                        || k == key!(shift - backtab)
                        || k == key!(backtab) =>
                    {
                        correcting.line = (correcting.line + lines - 1) % lines;
                    }
                    k if k == key!(right) => {
                        correcting.change(true);
                        self.error = None;
                    }
                    k if k == key!(left) => {
                        correcting.change(false);
                        self.error = None;
                    }
                    _ => {}
                }
                Outcome::Stay
            }
            None => {
                match ck {
                    k if k == BACK_KEY => return Outcome::Back,
                    k if k == key!(enter) => self.correct(self.selected),
                    k if k == key!(down) && count > 0 => {
                        self.selected = (self.selected + 1) % count;
                    }
                    k if k == key!(up) && count > 0 => {
                        self.selected = (self.selected + count - 1) % count;
                    }
                    k if k == key!(home) => self.selected = 0,
                    k if k == key!(end) => self.selected = count.saturating_sub(1),
                    _ => {}
                }
                self.saved = false;
                Outcome::Stay
            }
        }
    }

    pub fn update(&mut self, message: Message) -> Outcome {
        match (message, &mut self.correcting) {
            (Message::Event(i), _) => self.correct(i),
            (Message::Save, Some(_)) => self.save(),
            (Message::Cancel, _) => self.cancel(),
            (Message::EventType(e), Some(c)) => {
                c.correction.choose_event_type(e);
                c.refresh();
            }
            (Message::Eval(e), Some(c)) => {
                c.correction.choose_eval(e);
                c.refresh();
            }
            (Message::Technique(t), Some(c)) => {
                c.correction.choose_technique(t);
                c.refresh();
            }
            (_, None) => {}
        }
        Outcome::Stay
    }

    pub fn hints(&self, s: &Strings) -> Vec<(String, String)> {
        match &self.correcting {
            Some(_) => vec![
                ("↑ ↓".to_string(), String::new()),
                ("← →".to_string(), s.change.to_string()),
                ("Enter".to_string(), s.save.to_string()),
                ("Esc".to_string(), s.cancel.to_string()),
            ],
            None => vec![
                ("↑ ↓".to_string(), String::new()),
                ("Enter".to_string(), s.correct.to_string()),
                ("Esc".to_string(), s.back.to_string()),
            ],
        }
    }

    pub fn view<'a>(&'a self, s: &'static Strings, c: Colors) -> Element<'a, Message> {
        let rows = self
            .set
            .events
            .iter()
            .enumerate()
            .map(|(i, e)| event_row(e, i == self.selected, i, c));
        let list: Element<'a, Message> = match self.set.events.is_empty() {
            true => ui::body(s.no_events).color(c.muted).into(),
            false => scrollable(Column::with_children(rows).spacing(2))
                .id(Id::new(LIST))
                .height(Length::Fill)
                .into(),
        };
        let side = match &self.correcting {
            Some(correcting) => self.correction(correcting, s, c),
            None => self.chosen(s, c),
        };
        row![
            container(ui::panel(
                Some(s.set_events),
                column![list].height(Length::Fill).width(Length::Fill),
                c
            ))
            .width(Length::FillPortion(3))
            .height(Length::Fill),
            container(side).width(Length::FillPortion(2)),
        ]
        .spacing(14)
        .height(Length::Fill)
        .into()
    }

    /// The event chosen, not being corrected.
    fn chosen<'a>(&'a self, s: &'static Strings, c: Colors) -> Element<'a, Message> {
        let Some(e) = self.set.events.get(self.selected) else {
            return column![].into();
        };
        let note: Element<'a, Message> = match (self.saved, &self.error, e.editable) {
            (_, Some(error), _) => ui::body(error.clone()).color(c.danger).into(),
            (true, None, _) => ui::body(s.corrected).color(c.accent).into(),
            (false, None, false) => ui::body(s.lineup_fixed).color(c.muted).into(),
            (false, None, true) => container(ui::pill(
                &format!("Enter  {}", s.correct),
                true,
                Some(Message::Event(self.selected)),
                c,
            ))
            .width(Length::Fill)
            .align_x(Alignment::End)
            .into(),
        };
        ui::panel(
            Some(&ui::fill(s.event_n, &[(e.index + 1).to_string()])),
            column![
                text(describe(e)).size(SIZE_BODY).font(theme::fonts().bold),
                player_line(e, c),
                text(format!("{}–{}", e.score_us, e.score_them))
                    .size(22)
                    .font(theme::fonts().bold)
                    .color(c.accent),
                note,
            ]
            .spacing(10)
            .width(Length::Fill),
            c,
        )
    }

    fn correction<'a>(
        &'a self,
        correcting: &'a Correcting,
        s: &'static Strings,
        c: Colors,
    ) -> Element<'a, Message> {
        let st = &correcting.state;
        let lines = correcting.lines();
        let line = |l: Line| lines.get(correcting.line) == Some(&l);
        let choices = lines.iter().map(|l| match l {
            Line::EventType => ui::choice_line(
                s.events,
                line(*l),
                st.event_types
                    .iter()
                    .map(|e| {
                        ui::pill(
                            &ui::capitalized(&e.name),
                            e.value == st.event.event,
                            Some(Message::EventType(e.value)),
                            c,
                        )
                    })
                    .collect(),
                c,
            ),
            Line::Eval => ui::choice_line(
                s.eval,
                line(*l),
                st.evals
                    .iter()
                    .map(|e| {
                        ui::pill(
                            &e.symbol,
                            Some(e.value) == st.eval,
                            Some(Message::Eval(e.value)),
                            c,
                        )
                    })
                    .collect(),
                c,
            ),
            Line::Technique => ui::choice_line(
                match st.event.event {
                    EventTypeEnum::S => s.serve_type,
                    _ => s.set_type,
                },
                line(*l),
                std::iter::once(ui::pill(
                    "–",
                    correcting.technique().is_none(),
                    Some(Message::Technique(None)),
                    c,
                ))
                .chain(st.techniques.iter().map(|t| {
                    let label = match &t.short_name {
                        Some(name) => format!("{} {name}", t.code),
                        None => t.code.clone(),
                    };
                    ui::pill(&label, t.chosen, Some(Message::Technique(Some(t.value))), c)
                }))
                .collect(),
                c,
            ),
        });
        let eval_name = st
            .evals
            .iter()
            .find(|e| Some(e.value) == st.eval)
            .map(|e| e.name.clone())
            .unwrap_or_default();
        let error: Element<'a, Message> = match &self.error {
            Some(e) => ui::body(e.clone()).color(c.danger).into(),
            None => column![].into(),
        };
        let player = st
            .event
            .player
            .as_ref()
            .map(|p| format!("{} {}", p.number, p.name))
            .unwrap_or_default();
        ui::panel(
            Some(&ui::fill(s.event_n, &[(st.index + 1).to_string()])),
            column![
                text(format!(
                    "{}–{} · {} · {}",
                    st.score_us, st.score_them, st.rotation, st.phase
                ))
                .size(SIZE_SMALL)
                .color(c.muted),
                ui::body(player),
                Column::with_children(choices).spacing(10),
                text(eval_name).size(SIZE_SMALL).color(c.muted),
                error,
                container(
                    row![
                        ui::pill(
                            &format!("Esc  {}", s.cancel),
                            false,
                            Some(Message::Cancel),
                            c
                        ),
                        ui::pill(&format!("Enter  {}", s.save), true, Some(Message::Save), c),
                    ]
                    .spacing(10)
                )
                .width(Length::Fill)
                .align_x(Alignment::End),
            ]
            .spacing(12)
            .width(Length::Fill),
            c,
        )
    }
}

/// The event and its set or serve, e.g. "Attacco · A".
fn describe(e: &SetEvent) -> String {
    let name = ui::capitalized(&e.event.name);
    match &e.event.technique {
        Some(t) => format!("{name} · {t}"),
        None => name,
    }
}

fn player_line<'a>(e: &SetEvent, c: Colors) -> Element<'a, Message> {
    match &e.event.player {
        Some(p) => text(format!("{} {}", p.number, p.name))
            .size(SIZE_SMALL)
            .color(c.muted)
            .into(),
        None => column![].into(),
    }
}

/// An event: its place, the symbol of the evaluation, the event and the
/// player, the score after it; the changes of the lineup dimmed.
fn event_row<'a>(e: &SetEvent, chosen: bool, i: usize, c: Colors) -> Element<'a, Message> {
    let color = match e.editable {
        true => c.text,
        false => c.muted,
    };
    let bold = theme::fonts().bold;
    let body = row![
        text((e.index + 1).to_string())
            .size(SIZE_SMALL)
            .color(c.muted)
            .width(Length::Fixed(36.0))
            .align_x(Alignment::End),
        text(e.event.symbol.clone().unwrap_or_default())
            .size(SIZE_BODY)
            .font(bold)
            .color(color)
            .width(Length::Fixed(20.0)),
        column![
            text(describe(e)).size(SIZE_BODY).color(color),
            player_line(e, c),
        ]
        .width(Length::Fill),
        text(format!("{}–{}", e.score_us, e.score_them))
            .size(SIZE_BODY)
            .font(bold)
            .color(color),
    ]
    .spacing(10)
    .align_y(Alignment::Center);
    ui::list_row(body, chosen, Some(Message::Event(i)), c)
}

#[cfg(test)]
#[allow(clippy::unreachable)]
mod tests {
    use {
        super::*,
        crate::{game, strings::strings},
        scout4all_core::shapes::enums::LanguageEnum,
        scout4all_mobile::{DvwChoice, DvwSide},
    };

    /// The match of the tests of the TUI, three sets, imported.
    fn imported(dir: &tempfile::TempDir) -> (Arc<Scout4All>, String, String) {
        let core = Scout4All::new(dir.path().to_string_lossy().into_owned(), "it".into()).unwrap();
        let bytes = include_bytes!("../../src/tests/fixtures/three_sets.dvw").to_vec();
        let choice = DvwChoice {
            side: DvwSide::Home,
            day: "2015-01-25".into(),
            max_substitutions: 6,
            team_id: None,
            classification: core.classifications().first().map(|c| c.value),
            gender: core.genders().first().map(|g| g.value),
            country: core.countries("Ital".into()).first().map(|c| c.value),
        };
        let imported = core.import_dvw(bytes, choice, false).unwrap();
        (core, imported.team_id, imported.match_id)
    }

    #[test]
    fn the_serve_of_an_event_corrected_from_the_keys() {
        let s = strings(LanguageEnum::It);
        let dir = tempfile::tempdir().unwrap();
        let (core, team_id, match_id) = imported(&dir);
        // from the match: the digit opens the events of the set
        let mut m =
            game::MatchScreen::new(core.clone(), team_id.clone(), match_id.clone()).unwrap();
        assert!(matches!(m.key(s, key!(1)), game::Outcome::Events(1)));
        assert!(matches!(m.key(s, key!(9)), game::Outcome::Stay));

        let mut events = EventsScreen::new(core.clone(), team_id, match_id, 1).unwrap();
        let last = events.set.events.len() - 1;
        assert_eq!(events.selected, last);
        assert_eq!(events.scroll().1, 1.0);
        events.key(key!(down));
        assert_eq!(events.selected, 0);
        // a change of the lineup: not corrected
        if let Some(i) = events.set.events.iter().position(|e| !e.editable) {
            events.update(Message::Event(i));
            assert!(events.correcting.is_none());
        }
        let Some(serve) = events
            .set
            .events
            .iter()
            .position(|e| e.editable && e.event.event == EventTypeEnum::S)
        else {
            unreachable!("the set has serves")
        };
        let before = events.set.events[serve].event.technique.clone();
        events.update(Message::Event(serve));
        let Some(correcting) = &events.correcting else {
            unreachable!("a serve is corrected")
        };
        let Some(line) = correcting
            .lines()
            .iter()
            .position(|l| *l == Line::Technique)
        else {
            unreachable!("a serve has its type")
        };
        let was = correcting.technique();
        for _ in 0..line {
            events.key(key!(down));
        }
        events.key(key!(right));
        let now = events.correcting.as_ref().and_then(Correcting::technique);
        assert_ne!(now, was);
        // Esc leaves it as it was
        events.key(BACK_KEY);
        assert!(events.correcting.is_none());
        assert_eq!(
            events.set.events[serve].event.technique.is_some(),
            was.is_some()
        );
        // again, saved
        events.key(key!(enter));
        assert!(events.correcting.is_some());
        for _ in 0..line {
            events.key(key!(down));
        }
        events.key(key!(right));
        events.key(key!(enter));
        assert!(events.correcting.is_none(), "{:?}", events.error);
        assert!(events.saved);
        let reread =
            EventsScreen::new(core, events.team_id.clone(), events.match_id.clone(), 1).unwrap();
        assert_eq!(reread.set.events, events.set.events);
        assert_ne!(reread.set.events[serve].event.technique, before);
        assert!(matches!(events.key(BACK_KEY), Outcome::Back));
    }
}
