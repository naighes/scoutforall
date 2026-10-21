use crate::providers::settings_reader::SettingsReader;
use crate::providers::settings_writer::SettingsWriter;
use crate::screens::screen::{Sba, get_keybinding_actions};
use crate::screens::settings_screen::{SettingsPanel, SettingsScreen};
use crate::screens::{
    components::{navigation_footer::NavigationFooter, notify_banner::NotifyBanner, set_board},
    screen::{AppAction, Renderable, ScreenAsync},
};
use crate::shapes::actions::ScreenActionEnum;
use crate::shapes::keybinding::navigation_keybindings;
use crate::shapes::keybinding::{ActionsKeyBindings, KeyBindings, combination_of};
use crate::shapes::settings::Settings;
use crate::shapes::symbol::KeyCombinationFormatExt;
use async_trait::async_trait;
use chrono::Utc;
use crokey::crossterm::event::{KeyCode, KeyEvent};
use crokey::{KeyCombination, KeyCombinationFormat};
use ratatui::text::Line;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Row, Table},
};
use scout4all_core::analytics::global::{enqueue_match_for_upload, uploads_enabled};
use scout4all_core::{
    localization::current_labels,
    providers::set_writer::SetWriter,
    shapes::{
        enums::{EvalEnum, EventTypeEnum, FriendlyName, RoleEnum},
        r#match::MatchEntry,
        set::SetEntry,
        snapshot::{EventEntry, Snapshot},
        technique::{ServeTypeEnum, SetTypeEnum, TechniqueEnum},
    },
};
use std::sync::Arc;
use uuid::Uuid;

#[derive(Debug)]
pub struct ScoutingScreen<
    SSW: SetWriter + Send + Sync,
    SW: SettingsWriter + Send + Sync + 'static,
    SR: SettingsReader + Send + Sync + 'static,
> {
    settings: Settings,
    current_match: MatchEntry,
    set: SetEntry,
    snapshot: Snapshot,
    current_event: EventTypeInput,
    player: Option<Uuid>,
    /// the set or serve chosen along with the evaluation
    technique: Option<TechniqueEnum>,
    state: ScoutingScreenState,
    notify_message: NotifyBanner,
    back_stack_count: Option<u8>,
    back: bool,
    footer: NavigationFooter,
    set_writer: Arc<SSW>,
    screen_key_bindings: ActionsKeyBindings<ScreenActionEnum>,
    event_screen_key_bindings: ActionsKeyBindings<EventTypeEnum>,
    event_key_bindings: KeyBindings<EventTypeEnum>,
    settings_writer: Arc<SW>,
    settings_reader: Arc<SR>,
}

#[derive(Debug)]
pub struct LineupChoiceEntry {
    index: u8,
    id: Uuid,
    name: String,
    number: u8,
    role: String,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash)]
enum EventTypeInput {
    Some(EventTypeEnum),
    None,
}

#[derive(Debug, PartialEq)]
enum ScoutingScreenState {
    Event,
    Player,
    Eval,
    Replacement,
}

/// The score of the set, on top.
const SCORE_HEADER_HEIGHT: u16 = 3;
/// The point sequence, below the score: a row per team and the borders.
const POINT_SEQUENCE_HEIGHT: u16 = 4;

/// The rows the tallest panel of the left column takes: the evaluations
/// of an event with, below, the sets or serves to choose from (the attack
/// of a middle blocker), each list with its borders.
fn tallest_left_column() -> u16 {
    let most_sets = RoleEnum::ALL
        .into_iter()
        .flat_map(|role| [false, true].map(|back_row| SetTypeEnum::available(role, back_row).len()))
        .max()
        .unwrap_or(0);
    EventTypeEnum::ALL
        .into_iter()
        .map(|event| {
            let techniques = match event {
                EventTypeEnum::A => most_sets,
                EventTypeEnum::S => ServeTypeEnum::ALL.len(),
                _ => 0,
            };
            let evals = event.available_evals().len() + 2;
            match techniques {
                0 => evals,
                n => evals + n + 2,
            }
        })
        .max()
        .unwrap_or(0) as u16
}

impl<
    SSW: SetWriter + Send + Sync,
    SW: SettingsWriter + Send + Sync,
    SR: SettingsReader + Send + Sync,
> Renderable for ScoutingScreen<SSW, SW, SR>
{
    fn footer_entries(&self) -> Vec<(String, String)> {
        get_keybinding_actions(navigation_keybindings(), &self.get_screen_actions())
    }

    fn render(&mut self, f: &mut Frame, body: Rect, footer_left: Rect, footer_right: Rect) {
        // the point sequence only when the tallest panel of the left column
        // still fits below it
        let point_sequence_height = match body.height
            >= SCORE_HEADER_HEIGHT + POINT_SEQUENCE_HEIGHT + tallest_left_column()
        {
            true => POINT_SEQUENCE_HEIGHT,
            false => 0,
        };
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(SCORE_HEADER_HEIGHT),
                Constraint::Length(point_sequence_height),
                Constraint::Min(1),
            ])
            .split(body);
        let (header, point_sequence, body) = (rows[0], rows[1], rows[2]);
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(40),
                Constraint::Percentage(20),
                Constraint::Percentage(40),
            ])
            .split(body);
        // the sets or serves are listed below the evaluations
        let techniques = match self.state {
            ScoutingScreenState::Eval => self.available_techniques(),
            _ => vec![],
        };
        // the list to choose from takes half of the column, or all it needs
        // when it's longer, the latest events the rest
        let list_height = (columns[0].height / 2)
            .max(self.left_list_len() as u16 + 2)
            .min(columns[0].height);
        let left = Layout::default()
            .direction(Direction::Vertical)
            .constraints(match techniques.len() {
                0 => [
                    Constraint::Length(list_height),
                    Constraint::Length(0),
                    Constraint::Min(0),
                ],
                n => [
                    Constraint::Length(self.available_evals().len() as u16 + 2),
                    Constraint::Length(n as u16 + 2),
                    Constraint::Min(0),
                ],
            })
            .split(columns[0]);
        let (left_top, left_middle, left_bottom, center, right) =
            (left[0], left[1], left[2], columns[1], columns[2]);
        match self.state {
            ScoutingScreenState::Event => {
                self.render_available_events(f, left_top);
            }
            ScoutingScreenState::Eval => {
                self.render_eval_table(f, left_top);
                self.render_technique_table(f, left_middle, &techniques);
            }
            ScoutingScreenState::Player => {
                self.render_lineup_choices(f, left_top);
            }
            ScoutingScreenState::Replacement => {
                self.render_replacement_choices(f, left_top);
            }
        }
        let screen_actions = &self.get_screen_actions();
        let kb = navigation_keybindings();
        let screen_key_bindings = kb.slice(Sba::keys(screen_actions));

        self.render_header(f, header);
        if point_sequence_height > 0 {
            set_board::render_point_sequence(
                f,
                point_sequence,
                &self.current_match,
                &self.snapshot,
            );
        }
        self.footer.render(f, footer_left, self.footer_entries());
        self.render_recent_events(f, left_bottom);
        self.render_set_status(f, center);
        self.render_court(f, right);
        self.notify_message.render(f, footer_right);
        self.screen_key_bindings = screen_key_bindings;
    }
}

#[async_trait]
impl<
    SSW: SetWriter + Send + Sync,
    SW: SettingsWriter + Send + Sync,
    SR: SettingsReader + Send + Sync,
> ScreenAsync for ScoutingScreen<SSW, SW, SR>
{
    async fn handle_key(&mut self, key: KeyEvent) -> AppAction {
        use ScoutingScreenState::*;
        if let Some(key_combination) = self.screen_key_bindings.transform(key) {
            match (
                &self.notify_message.has_value(),
                &self.screen_key_bindings.get(key_combination),
                key.code,
                &self.state,
            ) {
                (true, _, _, _) => {
                    self.notify_message.reset();
                    if self.back {
                        return AppAction::Back(true, self.back_stack_count);
                    } else {
                        return AppAction::None;
                    }
                }
                (false, Some(ScreenActionEnum::Back), _, _) => {
                    return AppAction::Back(true, self.back_stack_count);
                }
                (false, Some(ScreenActionEnum::Settings), _, _) => {
                    AppAction::SwitchScreen(Box::new(SettingsScreen::new(
                        self.settings.clone(),
                        self.settings_writer.clone(),
                        self.settings_reader.clone(),
                        SettingsPanel::Shortcuts,
                    )))
                }
                (false, action, _, Event) => self.handle_event_screen(key, action.cloned()).await,
                (false, action, _, Player) => {
                    return self.handle_player_screen(key, action.cloned()).await;
                }
                (false, action, _, Eval) => {
                    return self.handle_eval_screen(key, action.cloned()).await;
                }
                (false, action, _, Replacement) => {
                    return self.handle_replacement_screen(key, action.cloned()).await;
                }
            }
        } else {
            return AppAction::None;
        }
    }

    async fn refresh_data(&mut self) {
        if let Ok(settings) = &self.settings_reader.read().await {
            self.settings = settings.to_owned();
            self.event_key_bindings = settings.scouting_keybindings.clone();
            // all the events: the available ones change along the set, and the
            // others get "not available" when pressed
            self.event_screen_key_bindings =
                ActionsKeyBindings::from(self.event_key_bindings.reverse_map().iter().collect());
        }
    }
}

impl<
    SSW: SetWriter + Send + Sync,
    SW: SettingsWriter + Send + Sync,
    SR: SettingsReader + Send + Sync,
> ScoutingScreen<SSW, SW, SR>
{
    pub fn new(
        settings_reader: Arc<SR>,
        settings_writer: Arc<SW>,
        settings: Settings,
        current_match: MatchEntry,
        set: SetEntry,
        snapshot: Snapshot,
        back_stack_count: Option<u8>,
        set_writer: Arc<SSW>,
    ) -> Self {
        let event_key_bindings = settings.scouting_keybindings.clone();
        ScoutingScreen {
            settings_reader,
            settings_writer,
            settings,
            current_match,
            set,
            snapshot,
            current_event: EventTypeInput::None,
            player: None,
            technique: None,
            state: ScoutingScreenState::Event,
            notify_message: NotifyBanner::new(),
            back_stack_count,
            back: false,
            footer: NavigationFooter::new(),
            set_writer,
            screen_key_bindings: ActionsKeyBindings::empty(),
            event_key_bindings: event_key_bindings.to_owned(),
            event_screen_key_bindings: ActionsKeyBindings::from(
                event_key_bindings.reverse_map().iter().collect(),
            ),
        }
    }

    async fn enqueue_match_for_analytics(&self) {
        // the global flag, not the settings this screen was created with: they
        // may have changed since
        if uploads_enabled() {
            let _ =
                enqueue_match_for_upload(self.current_match.team.id, self.current_match.id.clone())
                    .await;
            // silently ignore errors
        }
    }

    // filters and validates a potential lineup choice for a given event.
    fn filter_lineup_choices(
        &self,
        index: u8,
        (role, player_id): (String, Option<Uuid>),
    ) -> Option<LineupChoiceEntry> {
        player_id
            // ensure the player can play the event
            .filter(|id| match self.current_event {
                EventTypeInput::Some(event_type) => self.snapshot.can_play(event_type, id),
                EventTypeInput::None => self.snapshot.current_lineup.find_position(id).is_some(),
            })
            .and_then(|id| self.current_match.team.find_player(id))
            .map(|player| LineupChoiceEntry {
                index,
                id: player.id,
                name: player.name.clone(),
                number: player.number,
                role,
            })
    }

    // filters and validates the players that can be replaced in the current lineup.
    fn filter_replaceable_choices(
        &self,
        index: u8,
        (role, player_id): (String, Option<Uuid>),
    ) -> Option<LineupChoiceEntry> {
        player_id.and_then(|id| {
            self.snapshot
                .current_lineup
                .find_position(&id)
                .and_then(|_| self.current_match.team.find_player(id))
                .map(|p| LineupChoiceEntry {
                    index,
                    id,
                    name: p.name.clone(),
                    number: p.number,
                    role,
                })
        })
    }

    /// Returns the list of lineup choices depending on the current event type.
    ///
    /// - If the current event is a substitution, this function
    ///   returns the players currently on court that can be replaced.
    /// - For all other event types, it returns the players in the current lineup
    ///   that are eligible to be associated with the event.
    fn get_lineup_choices(&self) -> Vec<LineupChoiceEntry> {
        let (choices, filter_fn): (
            Vec<(u8, (String, Option<Uuid>))>,
            Box<dyn Fn(u8, (String, Option<Uuid>)) -> Option<LineupChoiceEntry>>,
        ) = match self.current_event {
            EventTypeInput::Some(EventTypeEnum::R) => (
                self.snapshot
                    .current_lineup
                    .get_replaceable_lineup_choices(),
                Box::new(|i, data| self.filter_replaceable_choices(i, data)),
            ),
            _ => (
                self.snapshot.current_lineup.get_lineup_choices(),
                Box::new(|i, data| self.filter_lineup_choices(i, data)),
            ),
        };
        choices
            .into_iter()
            .filter_map(|(i, (role, player_id))| filter_fn(i, (role, player_id)))
            .collect()
    }

    /// The events of the match up to now: the ones of the sets before, then
    /// the ones of this set.
    fn match_events(&self) -> impl DoubleEndedIterator<Item = &EventEntry> {
        self.current_match
            .sets
            .iter()
            .filter(|s| s.set_number < self.set.set_number)
            .flat_map(|s| s.events.iter())
            .chain(self.set.events.iter())
    }

    /// The sets or serves the scout chooses from along with the evaluation:
    /// the sets the attacker can receive, or the serves.
    fn available_techniques(&self) -> Vec<TechniqueEnum> {
        match (self.current_event, self.player) {
            (EventTypeInput::Some(EventTypeEnum::A), Some(attacker)) => self
                .snapshot
                .available_set_types(&attacker)
                .into_iter()
                .map(TechniqueEnum::Set)
                .collect(),
            (EventTypeInput::Some(EventTypeEnum::S), Some(_)) => ServeTypeEnum::ALL
                .into_iter()
                .map(TechniqueEnum::Serve)
                .collect(),
            _ => vec![],
        }
    }

    /// The set or serve chosen before the scout picks one: the set told by
    /// the attacker and the touch before, the last serve of the server in
    /// the match.
    fn preselected_technique(&self) -> Option<TechniqueEnum> {
        match (self.current_event, self.player) {
            (EventTypeInput::Some(EventTypeEnum::A), Some(attacker)) => self
                .snapshot
                .preselected_set_type(&attacker)
                .map(TechniqueEnum::Set),
            (EventTypeInput::Some(EventTypeEnum::S), Some(server)) => Some(TechniqueEnum::Serve(
                ServeTypeEnum::preselected(&server, self.match_events()),
            )),
            _ => None,
        }
    }

    /// The set or serve bound to the key, among the available ones.
    fn technique_for(&self, ck: &KeyCombination) -> Option<TechniqueEnum> {
        match self.current_event {
            EventTypeInput::Some(EventTypeEnum::A) => self
                .settings
                .set_type_keybindings
                .action_for(ck)
                .copied()
                .map(TechniqueEnum::Set),
            EventTypeInput::Some(EventTypeEnum::S) => self
                .settings
                .serve_type_keybindings
                .action_for(ck)
                .copied()
                .map(TechniqueEnum::Serve),
            _ => None,
        }
        .filter(|technique| self.available_techniques().contains(technique))
    }

    fn technique_key(&self, technique: &TechniqueEnum) -> Option<KeyCombination> {
        match technique {
            TechniqueEnum::Set(set) => self.settings.set_type_keybindings.key_for(set),
            TechniqueEnum::Serve(serve) => self.settings.serve_type_keybindings.key_for(serve),
        }
    }

    /// The entries of the list the scout chooses from in the left column:
    /// the events, the evaluations, the players or their replacements.
    fn left_list_len(&self) -> usize {
        match (&self.state, self.player) {
            (ScoutingScreenState::Event, _) => self.snapshot.get_available_options().len(),
            (ScoutingScreenState::Eval, _) => self.available_evals().len(),
            (ScoutingScreenState::Player, _) => self.get_lineup_choices().len(),
            (ScoutingScreenState::Replacement, Some(replaced)) => self
                .snapshot
                .current_lineup
                .get_available_replacements(&self.current_match.team, replaced)
                .len(),
            (ScoutingScreenState::Replacement, None) => 0,
        }
    }

    fn available_evals(&self) -> Vec<EvalEnum> {
        match self.current_event {
            EventTypeInput::Some(event_type) => event_type.available_evals(),
            EventTypeInput::None => vec![],
        }
    }

    async fn undo_last_event(&mut self) -> AppAction {
        use EventTypeEnum::*;
        use ScoutingScreenState::*;
        // it's the event selection screen => remove the entry from the csv file
        let Ok(Some(removed_event)) = self
            .set_writer
            .remove_last_event(&self.current_match, self.set.set_number)
            .await
        else {
            // TODO: handle error?
            return AppAction::None;
        };
        self.set.events.pop();
        // set the previous (removed) event player
        self.player = removed_event.player;
        match (removed_event.event_type, removed_event.player) {
            (A | B | D | P | S, Some(_)) => {
                // (A)ttack, (B) block, (D)ig, (P)ass and (S)erve require evaluation
                self.current_event = EventTypeInput::Some(removed_event.event_type);
                // set the prompt eval state
                self.state = Eval;
            }
            (R, Some(_)) => {
                self.current_event = EventTypeInput::Some(removed_event.event_type);
                self.state = Replacement;
            }
            (_, Some(_)) => {
                // the removed event involved a player => set the prompt player state
                self.current_event = EventTypeInput::Some(removed_event.event_type);
                self.state = Player;
            }
            _ => {
                self.current_event = EventTypeInput::None;
                // set the prompt event selection state
                self.state = Event;
            }
        };
        // need to recompute snapshot from scratch
        match self
            .set
            .compute_snapshot(self.current_match.max_substitutions)
        {
            // since the last event has been removed, snapshot needs to
            // be re-computed from scratch
            Ok(snapshot) => {
                self.snapshot = snapshot;
                // the set or serve of the removed event, if it was saved
                self.technique = removed_event
                    .technique
                    .or_else(|| self.preselected_technique());
                AppAction::None
            }
            Err(_) => {
                self.notify_message
                    .set_error(current_labels().could_not_compute_snapshot.to_string());
                AppAction::None
            }
        }
    }

    async fn add_event(&mut self, event: &EventEntry) -> AppAction {
        // the event is saved only when it applies: otherwise the set wouldn't
        // replay anymore. The snapshot is updated only when it's saved.
        let mut snapshot = self.snapshot.clone();
        let saved = match snapshot.add_event(event) {
            Ok(()) => {
                self.set_writer
                    .append_event(&self.current_match, self.set.set_number, event)
                    .await
            }
            Err(e) => Err(e),
        };
        match saved {
            Ok(_) => {
                self.snapshot = snapshot;
                // append event to the set
                self.set.events.push(event.clone());
                // reset state
                self.current_event = EventTypeInput::None;
                self.player = None;
                self.technique = None;
                self.state = ScoutingScreenState::Event;
                match self.snapshot.get_set_winner(self.set.set_number) {
                    None => AppAction::None,
                    Some(_) => {
                        // update or add the set to the match
                        if let Some(existing_set) = self
                            .current_match
                            .sets
                            .iter_mut()
                            .find(|s| s.set_number == self.set.set_number)
                        {
                            // replace existing set with the completed one
                            *existing_set = self.set.clone();
                        } else {
                            // add new set if not there
                            self.current_match.sets.push(self.set.clone());
                        }
                        self.notify_message
                            .set_info(current_labels().set_over.to_string());
                        // check if match is finished and analytics are enabled
                        if let Ok(status) = self.current_match.get_status()
                            && status.match_finished
                        {
                            self.enqueue_match_for_analytics().await;
                        }
                        self.back = true;
                        AppAction::None
                    }
                }
            }
            Err(_) => {
                self.notify_message
                    .set_error(current_labels().could_not_add_event.to_string());
                AppAction::None
            }
        }
    }

    /* event handling */

    // sequence is event type => player => eval
    async fn handle_event_screen(
        &mut self,
        key: KeyEvent,
        action: Option<ScreenActionEnum>,
    ) -> AppAction {
        use EventTypeEnum::*;
        if let Some(ScreenActionEnum::Undo) = action {
            // undo
            return self.undo_last_event().await;
        }
        if let Some(key_combination) = self.event_screen_key_bindings.transform(key) {
            if let Some(evt) = self.event_screen_key_bindings.get(key_combination) {
                match (self.snapshot.get_available_options().contains(evt), evt) {
                    // no substitutions left
                    (true, R)
                        if self
                            .snapshot
                            .current_lineup
                            .was_max_number_of_substitutions_reached() =>
                    {
                        self.notify_message
                            .set_error(current_labels().max_substitutions_reached.to_string());
                        AppAction::None
                    }
                    // player is inferred when serving
                    (true, S) => {
                        self.player = self.snapshot.current_lineup.get_serving_player();
                        self.state = ScoutingScreenState::Eval;
                        self.current_event = EventTypeInput::Some(S);
                        self.technique = self.preselected_technique();
                        AppAction::None
                    }
                    // these events require player selection
                    (true, e) if e.requires_player() => {
                        self.current_event = EventTypeInput::Some(*e);
                        self.state = ScoutingScreenState::Player;
                        AppAction::None
                    }
                    // these events do not require player nor evaluation selection
                    (true, evt) if matches!(evt, OE | OS | OF | CL) => {
                        let entry = EventEntry {
                            timestamp: Utc::now(),
                            event_type: *evt,
                            eval: None,
                            player: None,
                            target_player: None,
                            technique: None,
                        };
                        self.add_event(&entry).await
                    }
                    // the selected option is not available => error
                    _ => {
                        self.current_event = EventTypeInput::None;
                        let template = current_labels().event_is_not_available;
                        self.notify_message
                            .set_error(template.replace("{}", &evt.to_string()));
                        AppAction::None
                    }
                }
            } else {
                AppAction::None
            }
        } else {
            AppAction::None
        }
    }

    async fn handle_replacement_screen(
        &mut self,
        key: KeyEvent,
        action: Option<ScreenActionEnum>,
    ) -> AppAction {
        use KeyCode::*;
        match (action, key.code, &self.current_event, self.player) {
            (Some(ScreenActionEnum::Undo), _, _, _) => {
                // undo
                self.player = None;
                self.state = ScoutingScreenState::Player;
                AppAction::None
            }
            (_, Char(_), EventTypeInput::Some(_), None) => {
                // replaced player is required
                self.notify_message
                    .set_error(current_labels().no_player_selected.to_string());
                AppAction::None
            }
            (_, Char(c), EventTypeInput::Some(event_type), Some(replaced_id)) => {
                // find available replacements for the selected (replaced) player
                let available_replacements = self
                    .snapshot
                    .current_lineup
                    .get_available_replacements(&self.current_match.team, replaced_id);
                if let Some(d) = c.to_digit(10).and_then(|d| u8::try_from(d).ok())
                    && (1..=available_replacements.len() as u8).contains(&d)
                    && let Some((_, p)) = available_replacements.iter().find(|(i, _)| *i == d)
                {
                    let entry = EventEntry {
                        timestamp: Utc::now(),
                        event_type: *event_type,
                        eval: None,
                        player: Some(replaced_id),
                        target_player: Some(p.id),
                        technique: None,
                    };
                    return self.add_event(&entry).await;
                }
                AppAction::None
            }
            _ => AppAction::None,
        }
    }

    async fn handle_player_screen(
        &mut self,
        key: KeyEvent,
        action: Option<ScreenActionEnum>,
    ) -> AppAction {
        let available_lineup_players = self.get_lineup_choices();
        use KeyCode::*;
        // undo
        if let Some(ScreenActionEnum::Undo) = action {
            self.current_event = EventTypeInput::None;
            self.state = ScoutingScreenState::Event;
            return AppAction::None;
        }
        let player = match key.code {
            Char(c) => c
                .to_digit(10)
                .take_if(|d| (1..=7).contains(d))
                .map(|d| d as u8)
                .and_then(|d| available_lineup_players.iter().find(|p| p.index == d)),
            _ => None,
        };
        match (player, &self.current_event) {
            (Some(player), EventTypeInput::Some(event_type))
                if event_type.requires_evaluation() =>
            {
                self.player = Some(player.id);
                self.state = ScoutingScreenState::Eval;
                self.technique = self.preselected_technique();
                AppAction::None
            }
            (Some(player), EventTypeInput::Some(EventTypeEnum::R)) => {
                self.player = Some(player.id);
                self.state = ScoutingScreenState::Replacement;
                AppAction::None
            }
            (Some(player), EventTypeInput::Some(event_type)) => {
                self.add_event(&EventEntry {
                    timestamp: Utc::now(),
                    event_type: *event_type,
                    eval: None,
                    player: Some(player.id),
                    target_player: None,
                    technique: None,
                })
                .await
            }
            _ => {
                self.notify_message
                    .set_error(current_labels().invalid_player_selection.to_string());
                AppAction::None
            }
        }
    }

    async fn handle_eval_screen(
        &mut self,
        key: KeyEvent,
        action: Option<ScreenActionEnum>,
    ) -> AppAction {
        // undo
        if let Some(ScreenActionEnum::Undo) = action {
            // it's an eval screen, so go to
            // * the ScoutingScreenState::Player screen if it's not serving
            // * otherwise, so it's serving, go to the ScoutingScreenState::Event
            self.player = None;
            self.technique = None;
            if self.current_event == EventTypeInput::Some(S) {
                self.current_event = EventTypeInput::None;
                self.state = ScoutingScreenState::Event;
            } else {
                self.state = ScoutingScreenState::Player;
            }
            return AppAction::None;
        }
        use EventTypeEnum::*;
        let ck = combination_of(key);
        // the evaluation bound to the key, if the event allows it
        let eval = match &self.current_event {
            EventTypeInput::Some(event_type) => self
                .settings
                .eval_keybindings
                .action_for(&ck)
                .copied()
                .filter(|eval| event_type.available_evals().contains(eval)),
            EventTypeInput::None => None,
        };
        match (eval, self.technique_for(&ck), self.current_event) {
            // ensure event type allows evaluation
            (Some(eval), _, EventTypeInput::Some(event_type @ (A | B | P | D | S))) => {
                let entry = EventEntry {
                    timestamp: Utc::now(),
                    event_type,
                    eval: Some(eval),
                    player: self.player,
                    target_player: None,
                    technique: self.technique,
                };
                self.add_event(&entry).await
            }
            (Some(_), _, EventTypeInput::Some(event_type)) => {
                let template = current_labels().evaluation_not_allowed_for_event;
                self.notify_message
                    .set_error(template.replace("{}", &event_type.to_string()));
                AppAction::None
            }
            // the set or serve changes, the evaluation saves the event
            (None, Some(technique), _) => {
                self.technique = Some(technique);
                AppAction::None
            }
            _ => AppAction::None,
        }
    }

    /* rendering */
    fn render_header(&self, f: &mut Frame, area: Rect) {
        set_board::render_score_header(
            f,
            area,
            &self.current_match,
            self.set.set_number,
            &self.snapshot,
        );
    }

    fn render_available_events(&mut self, f: &mut Frame, area: Rect) {
        let scf = KeyCombinationFormat::default();
        let se = self.event_key_bindings.clone();
        let rows: Vec<Row> = self
            .snapshot
            .get_available_options()
            .iter()
            .map(|ev| {
                Row::new(vec![
                    Line::from(ev.friendly_name(current_labels())),
                    Line::from(
                        se.key_for(ev)
                            .map(|kc| scf.to_string(kc))
                            .unwrap_or_default(),
                    )
                    .right_aligned(),
                ])
            })
            .collect();
        let table = Table::new(rows, [Constraint::Percentage(60), Constraint::Min(2)]).block(
            Block::default()
                .borders(Borders::ALL)
                .title(current_labels().choose_the_event)
                .style(Style::default().add_modifier(Modifier::REVERSED)),
        );
        f.render_widget(table, area);
    }

    fn render_technique_table(&self, f: &mut Frame, area: Rect, techniques: &[TechniqueEnum]) {
        let labels = current_labels();
        let rows: Vec<Row> = techniques
            .iter()
            .map(|technique| {
                let key = self
                    .technique_key(technique)
                    .map(|k| {
                        KeyCombinationFormatExt::new(&KeyCombinationFormat::default()).to_string(k)
                    })
                    .unwrap_or_else(|| labels.unassigned.to_string());
                let (check, style) = match self.technique == Some(*technique) {
                    true => ("[X]", Style::default().add_modifier(Modifier::BOLD)),
                    false => ("[ ]", Style::default()),
                };
                Row::new(vec![format!(
                    "{} {} => {}",
                    check,
                    key,
                    technique.description()
                )])
                .style(style)
            })
            .collect();
        let title = match self.current_event {
            EventTypeInput::Some(EventTypeEnum::S) => labels.serve_type,
            _ => labels.set_type,
        };
        let table = Table::new(rows, [Constraint::Percentage(100)]).block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .style(Style::default().add_modifier(Modifier::REVERSED)),
        );
        f.render_widget(table, area);
    }

    fn render_eval_table(&mut self, f: &mut Frame, area: Rect) {
        let available_evals = self.available_evals();
        let rows: Vec<Row> = available_evals
            .iter()
            .map(|ev| {
                let key = self
                    .settings
                    .eval_keybindings
                    .key_for(ev)
                    .map(|k| {
                        KeyCombinationFormatExt::new(&KeyCombinationFormat::default()).to_string(k)
                    })
                    .unwrap_or_else(|| current_labels().unassigned.to_string());
                Row::new(vec![format!(
                    "{} => {}",
                    key,
                    if let EventTypeInput::Some(last_event) = self.current_event {
                        if let Some(desc) = ev.friendly_description(last_event, current_labels()) {
                            format!(
                                "{} ({})",
                                ev.friendly_name(last_event, current_labels()),
                                desc
                            )
                        } else {
                            ev.friendly_name(last_event, current_labels())
                        }
                    } else {
                        current_labels().unknown.to_string()
                    }
                )])
            })
            .collect();
        let table = Table::new(rows, [Constraint::Percentage(100)]).block(
            Block::default()
                .borders(Borders::ALL)
                .title(current_labels().choose_the_evaluation)
                .style(Style::default().add_modifier(Modifier::REVERSED)),
        );
        f.render_widget(table, area);
    }

    fn render_court(&self, f: &mut Frame, area: Rect) {
        set_board::render_court(f, area, &self.current_match.team, &self.snapshot);
    }

    fn recent_event_row(&'_ self, i: usize, e: &EventEntry) -> Row<'_> {
        Row::new(vec![
            format!(" {:<12}", e.event_type.friendly_name(current_labels())),
            format!(
                " {:<20}",
                e.player
                    .and_then(|p1| self.current_match.team.find_player(p1))
                    .map(|p| p.name.as_str())
                    .unwrap_or("-")
            ),
            format!(
                " {:<10}",
                [
                    e.eval
                        .map(|e1| e1.friendly_name(e.event_type, current_labels())),
                    e.technique.map(|t| t.to_string()),
                ]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(" ")
            ),
        ])
        .style(if i == 0 {
            Style::default()
                .bg(Color::Blue)
                .fg(Color::White)
                .add_modifier(Modifier::BOLD)
        } else if i.is_multiple_of(2) {
            Style::default().bg(Color::DarkGray)
        } else {
            Style::default()
        })
    }

    fn render_recent_events(&self, f: &mut Frame, area: Rect) {
        let mut events: Vec<_> = self.set.events.iter().collect();
        events.sort_by_key(|e| e.timestamp);
        let rows = events
            .into_iter()
            .rev()
            .take(16)
            .enumerate()
            .map(|(i, e)| self.recent_event_row(i, e));
        let table = Table::new(
            rows,
            // the event wide enough for "freeball received"
            [
                Constraint::Percentage(44),
                Constraint::Percentage(24),
                Constraint::Percentage(32),
            ],
        )
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(current_labels().latest_events),
        );
        f.render_widget(table, area);
    }

    fn render_lineup_choices(&self, f: &mut Frame, area: Rect) {
        let lineup_choices = self.get_lineup_choices();
        let rows: Vec<Row> = lineup_choices
            .iter()
            .map(Self::format_player_choice_row)
            .collect();
        self.render_player_choices_table(f, area, rows, current_labels().player_selection);
    }

    fn render_replacement_choices(&mut self, f: &mut Frame, area: Rect) {
        match self.player {
            None => {
                self.notify_message
                    .set_error(current_labels().no_player_selected.to_string());
            }
            Some(replaced_id) => {
                let rows: Vec<Row> = self
                    .snapshot
                    .current_lineup
                    .get_available_replacements(&self.current_match.team, replaced_id)
                    .iter()
                    .map(|(i, player)| LineupChoiceEntry {
                        index: *i,
                        id: player.id,
                        name: player.name.clone(),
                        number: player.number,
                        role: player
                            .role
                            .map_or_else(|| "-".to_string(), |r| r.to_string()),
                    })
                    .map(|e| Self::format_player_choice_row(&e))
                    .collect();
                self.render_player_choices_table(
                    f,
                    area,
                    rows,
                    current_labels().select_replacement,
                );
            }
        }
    }

    fn format_player_choice_row<'a>(entry: &LineupChoiceEntry) -> Row<'a> {
        Row::new(vec![
            format!(" {:<12}", entry.index),
            format!(" #{:<12}", entry.number),
            format!(" {:<12}", entry.name),
            format!(" {:<20}", entry.role),
        ])
    }

    fn render_player_choices_table(&self, f: &mut Frame, area: Rect, rows: Vec<Row>, title: &str) {
        let table = Table::new(
            rows,
            [
                Constraint::Percentage(8),
                Constraint::Percentage(8),
                Constraint::Percentage(56),
                Constraint::Percentage(28),
            ],
        )
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .style(Style::default().add_modifier(Modifier::REVERSED)),
        );
        f.render_widget(table, area);
    }

    fn get_screen_actions(&self) -> Vec<Sba<ScreenActionEnum>> {
        match (self.set.events.len(), &self.state) {
            (0, ScoutingScreenState::Event) => vec![
                Sba::Simple(ScreenActionEnum::Settings),
                Sba::Simple(ScreenActionEnum::Back),
            ],
            _ => vec![
                Sba::Simple(ScreenActionEnum::Undo),
                Sba::Simple(ScreenActionEnum::Settings),
                Sba::Simple(ScreenActionEnum::Back),
            ],
        }
    }

    fn render_set_status(&self, f: &mut Frame, area: Rect) {
        let event = match &self.current_event {
            EventTypeInput::Some(ev) => Some(ev.friendly_name(current_labels()).to_string()),
            _ => None,
        };
        let player = self
            .player
            .and_then(|p| self.current_match.team.find_player(p))
            .map(|p| p.name.clone());
        set_board::render_set_status(f, area, &self.snapshot, event, player);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::fs::{
        settings_reader::FileSystemSettingsReader, settings_writer::FileSystemSettingsWriter,
    };
    use scout4all_core::{
        constants::DEFAULT_MAX_SUBSTITUTIONS,
        providers::{
            fs::{
                match_reader::FileSystemMatchReader, match_writer::FileSystemMatchWriter,
                set_reader::FileSystemSetReader, set_writer::FileSystemSetWriter,
                team_writer::FileSystemTeamWriter,
            },
            match_reader::MatchReader,
            match_writer::MatchWriter,
            team_writer::{TeamInput, TeamWriter},
        },
        shapes::{enums::TeamSideEnum, player::PlayerEntry},
    };
    use tempfile::TempDir;

    /// An event that doesn't apply isn't saved: up to v0.3.0 it was, and the
    /// set didn't open anymore.
    #[tokio::test]
    async fn event_that_does_not_apply_is_not_saved() {
        let dir = TempDir::new().unwrap();
        let base = dir.path();
        let players: Vec<PlayerEntry> = (0..7)
            .map(|i| PlayerEntry {
                name: format!("player {}", i + 1),
                number: i + 1,
                ..Default::default()
            })
            .collect();
        let ids: Vec<Uuid> = players.iter().map(|p| p.id).collect();
        let team = FileSystemTeamWriter::new(base)
            .save(TeamInput::New {
                id: None,
                name: "Volley Club".into(),
                year: 2025,
                classification: None,
                gender: None,
                players,
            })
            .await
            .unwrap();
        let m = FileSystemMatchWriter::new(base)
            .create(
                &team,
                "Cortona Volley".into(),
                Utc::now().fixed_offset(),
                true,
                DEFAULT_MAX_SUBSTITUTIONS,
            )
            .await
            .unwrap();
        let set_writer = Arc::new(FileSystemSetWriter::new(base));
        let positions: [Uuid; 6] = ids[..6].try_into().unwrap();
        let set = set_writer
            .create(
                &m,
                1,
                TeamSideEnum::Them,
                positions,
                ids[6],
                None,
                positions[0],
                vec![],
            )
            .await
            .unwrap();
        let snapshot = set.compute_snapshot(m.max_substitutions).unwrap();
        let mut screen = ScoutingScreen::new(
            Arc::new(FileSystemSettingsReader::new(base)),
            Arc::new(FileSystemSettingsWriter::new(base)),
            Settings::default(),
            m.clone(),
            set,
            snapshot,
            None,
            set_writer,
        );
        let event = |event_type, player, target_player| EventEntry {
            timestamp: Utc::now(),
            event_type,
            player,
            eval: None,
            target_player,
            technique: None,
        };
        // a player who isn't on court is substituted
        screen
            .add_event(&event(
                EventTypeEnum::R,
                Some(Uuid::new_v4()),
                Some(Uuid::new_v4()),
            ))
            .await;
        assert!(screen.notify_message.has_value());
        assert!(screen.set.events.is_empty());
        screen
            .add_event(&event(EventTypeEnum::OE, None, None))
            .await;
        assert_eq!(screen.snapshot.score_us, 1);
        // the set opens with the event that applied only
        let reader = FileSystemMatchReader::new(base, Arc::new(FileSystemSetReader::new(base)));
        let m = reader
            .read_single(&team, &m.id)
            .await
            .expect("expected the set to open");
        assert_eq!(m.sets[0].events.len(), 1);
        let snapshot = m.get_set_snapshot(1).unwrap().unwrap();
        assert_eq!(snapshot.score_us, 1);
        assert_eq!(snapshot.score_them, 0);
    }
}
