use crate::localization::current_labels;
use crate::{
    screens::{
        components::{
            navigation_footer::NavigationFooter, notify_banner::NotifyBanner, panel, set_board,
        },
        screen::{
            AppAction, FieldKey, LIST_NAVIGATION, Renderable, Sba, ScreenAsync, change_value_entry,
            get_keybinding_actions, list_entries, navigation_entries,
        },
    },
    shapes::{
        actions::ScreenActionEnum,
        keybinding::{ActionsKeyBindings, navigation_keybindings},
    },
};
use async_trait::async_trait;
use crokey::crossterm::event::KeyEvent;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::Span,
    widgets::{Row, Table, TableState},
};
use scout4all_core::{
    analytics::global::{enqueue_match_for_upload, uploads_enabled},
    errors::AppError,
    event_edit::{EditField, EventEdit},
    providers::set_writer::SetWriter,
    shapes::{
        enums::{EvalEnum, EventTypeEnum, FriendlyName},
        r#match::MatchEntry,
        set::SetEntry,
        snapshot::{EventEntry, Snapshot},
        technique::TechniqueEnum,
    },
};
use std::sync::Arc;

/// The events of a set already scouted, to correct their type, evaluation
/// and technique: the set replays with the edited event in its place, and
/// the edit is saved only if the events after it still fit and the set ends
/// as it did.
#[derive(Debug)]
pub struct EditSetEventsScreen<SSW: SetWriter + Send + Sync> {
    current_match: MatchEntry,
    set: SetEntry,
    table_state: TableState,
    /// the snapshot before the selected event, computed when it's selected
    before: Snapshot,
    /// the event being edited, if any
    form: Option<EventForm>,
    notify_message: NotifyBanner,
    footer: NavigationFooter,
    set_writer: Arc<SSW>,
    screen_key_bindings: ActionsKeyBindings<ScreenActionEnum>,
}

#[derive(Debug)]
struct EventForm {
    field: EditField,
    edit: EventEdit,
}

/// The player in the list of the events, shortened when the screen is
/// narrow.
const PLAYER_MIN_WIDTH: u16 = 10;

impl<SSW: SetWriter + Send + Sync> Renderable for EditSetEventsScreen<SSW> {
    fn footer_entries(&self) -> Vec<(String, String)> {
        self.footer_actions().1
    }

    fn render(&mut self, f: &mut Frame, body: Rect, footer_left: Rect, footer_right: Rect) {
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(1)])
            .split(body);
        let (header, body) = (rows[0], rows[1]);
        // the events and the set status at least as wide as their texts on
        // a narrow screen, the court what's left
        let events_width = (body.width * 2 / 5).max(Self::events_table_width(
            &Self::column_widths(&self.event_cells()),
        ));
        let center_width = (body.width / 5).max(set_board::set_status_width(&self.before));
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(events_width),
                Constraint::Length(center_width),
                Constraint::Fill(1),
            ])
            .split(body);
        let form_height = self
            .form
            .as_ref()
            .map_or(0, |form| form.edit.fields().len() as u16 + 2);
        let left = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(3), Constraint::Length(form_height)])
            .split(columns[0]);
        set_board::render_score_header(
            f,
            header,
            &self.current_match,
            self.set.set_number,
            &self.before,
        );
        self.render_events(f, left[0]);
        self.render_form(f, left[1]);
        let shown = self.shown_event();
        set_board::render_set_status(
            f,
            columns[1],
            &self.before,
            shown.map(|e| {
                e.event_type
                    .friendly_name(current_labels().domain)
                    .to_string()
            }),
            shown
                .and_then(|e| e.player)
                .and_then(|p| self.current_match.team.find_player(p))
                .map(|p| p.name.clone()),
            shown.is_some_and(|e| e.flags.inverted_p1()),
        );
        set_board::render_court(f, columns[2], &self.current_match.team, &self.before);
        self.notify_message.render(f, footer_right);
        let (actions, footer_entries) = self.footer_actions();
        self.footer.render(f, footer_left, footer_entries);
        self.screen_key_bindings = navigation_keybindings().slice(Sba::keys(&actions));
    }
}

#[async_trait]
impl<SSW: SetWriter + Send + Sync> ScreenAsync for EditSetEventsScreen<SSW> {
    async fn handle_key(&mut self, key: KeyEvent) -> AppAction {
        let Some(key_combination) = self.screen_key_bindings.transform(key) else {
            return AppAction::None;
        };
        let action = self.screen_key_bindings.get(key_combination).copied();
        match (
            self.notify_message.has_value(),
            self.form.is_some(),
            FieldKey::of(key.code),
            action,
        ) {
            // the form stays as it was after an error
            (true, _, _, _) => {
                self.notify_message.reset();
                AppAction::None
            }
            (_, true, Some(FieldKey::Change { back }), _) => {
                self.change_value(back);
                AppAction::None
            }
            (_, true, Some(FieldKey::Previous), _) => self.move_field(false),
            (_, true, Some(FieldKey::Next), _) => self.move_field(true),
            (_, true, _, Some(ScreenActionEnum::Confirm)) => self.save().await,
            (_, true, _, Some(ScreenActionEnum::Back)) => {
                self.form = None;
                AppAction::None
            }
            (_, false, _, Some(ScreenActionEnum::Up | ScreenActionEnum::Previous)) => {
                self.select(false)
            }
            (_, false, _, Some(ScreenActionEnum::Down | ScreenActionEnum::Next)) => {
                self.select(true)
            }
            (_, false, _, Some(ScreenActionEnum::Select)) => self.edit_selected(),
            (_, false, _, Some(ScreenActionEnum::Back)) => AppAction::Back(true, Some(1)),
            (_, false, _, Some(ScreenActionEnum::Quit)) => AppAction::Quit(Ok(())),
            _ => AppAction::None,
        }
    }

    async fn refresh_data(&mut self) {}
}

impl<SSW: SetWriter + Send + Sync> EditSetEventsScreen<SSW> {
    pub fn new(
        current_match: MatchEntry,
        set: SetEntry,
        set_writer: Arc<SSW>,
    ) -> Result<Self, AppError> {
        let before = set.snapshot_before(0, current_match.max_substitutions)?;
        Ok(EditSetEventsScreen {
            current_match,
            set,
            table_state: TableState::default().with_selected(Some(0)),
            before,
            form: None,
            notify_message: NotifyBanner::new(),
            footer: NavigationFooter::new(),
            set_writer,
            screen_key_bindings: ActionsKeyBindings::empty(),
        })
    }

    fn selected(&self) -> usize {
        self.table_state.selected().unwrap_or(0)
    }

    /// The event the status shows: the one being edited, or the selected one.
    fn shown_event(&self) -> Option<&EventEntry> {
        self.form
            .as_ref()
            .map(|form| form.edit.event())
            .or_else(|| self.set.events.get(self.selected()))
    }

    /// Moves to the next or previous event, and replays the set up to it.
    fn select(&mut self, next: bool) -> AppAction {
        let last = self.set.events.len().saturating_sub(1);
        let selected = match next {
            true => (self.selected() + 1).min(last),
            false => self.selected().saturating_sub(1),
        };
        self.table_state.select(Some(selected));
        self.replay_before_selected();
        AppAction::None
    }

    fn replay_before_selected(&mut self) {
        match self
            .set
            .snapshot_before(self.selected(), self.current_match.max_substitutions)
        {
            Ok(snapshot) => self.before = snapshot,
            Err(_) => self.notify_message.set_error(
                current_labels()
                    .domain
                    .could_not_compute_snapshot
                    .to_string(),
            ),
        }
    }

    fn edit_selected(&mut self) -> AppAction {
        match EventEdit::new(
            &self.set,
            self.selected(),
            self.current_match.max_substitutions,
        ) {
            Ok(edit) => {
                self.form = Some(EventForm {
                    field: EditField::EventType,
                    edit,
                });
            }
            Err(e) => self
                .notify_message
                .set_error(e.message(current_labels().domain).to_string()),
        }
        AppAction::None
    }

    fn move_field(&mut self, next: bool) -> AppAction {
        if let Some(form) = self.form.as_mut() {
            form.field = cycle(&form.edit.fields(), form.field, !next, true);
        }
        AppAction::None
    }

    /// Changes the value of the field: a new event keeps the evaluation and
    /// the technique when it allows them.
    fn change_value(&mut self, back: bool) {
        let Some(form) = self.form.as_mut() else {
            return;
        };
        let edit = &mut form.edit;
        let event = edit.event().clone();
        match form.field {
            EditField::EventType => {
                edit.choose_event_type(cycle(&edit.event_types(), event.event_type, back, true))
            }
            EditField::Eval => {
                if let Some(eval) = event.eval {
                    edit.choose_eval(cycle(&edit.evals(), eval, back, true));
                }
            }
            EditField::Technique => {
                edit.choose_technique(cycle(&edit.techniques(), event.technique, back, true))
            }
        }
    }

    async fn save(&mut self) -> AppAction {
        let Some(form) = self.form.as_ref() else {
            return AppAction::None;
        };
        let saved = match form.edit.apply() {
            Ok(set) => self
                .set_writer
                .update(&self.current_match, &set)
                .await
                .map(|_| set)
                .map_err(|_| current_labels().could_not_save_set),
            Err(e) => Err(e.message(current_labels().domain)),
        };
        match saved {
            Ok(set) => {
                if let Some(existing) = self
                    .current_match
                    .sets
                    .iter_mut()
                    .find(|s| s.set_number == set.set_number)
                {
                    *existing = set.clone();
                }
                self.set = set;
                self.form = None;
                self.replay_before_selected();
                self.enqueue_match_for_analytics().await;
            }
            Err(message) => self.notify_message.set_error(message.to_string()),
        }
        AppAction::None
    }

    /// A finished match is uploaded again with the events edited.
    async fn enqueue_match_for_analytics(&self) {
        let finished = self
            .current_match
            .get_status()
            .is_ok_and(|status| status.match_finished);
        if finished && uploads_enabled() {
            // silently ignore errors
            let _ =
                enqueue_match_for_upload(self.current_match.team.id, self.current_match.id.clone())
                    .await;
        }
    }

    fn eval_name(event: &EventEntry, eval: Option<EvalEnum>) -> String {
        eval.map(|e| e.friendly_name(event.event_type, current_labels().domain))
            .unwrap_or_else(|| "-".to_string())
    }

    fn technique_name(technique: Option<TechniqueEnum>) -> String {
        technique
            .map(|t| t.to_string())
            .unwrap_or_else(|| "-".to_string())
    }

    /// The texts of the events, a row each: the number, the event, the
    /// player, the evaluation and the set or serve.
    fn event_cells(&self) -> Vec<[String; 5]> {
        self.set
            .events
            .iter()
            .enumerate()
            .map(|(i, e)| {
                [
                    format!("{:>3}", i + 1),
                    e.event_type
                        .friendly_name(current_labels().domain)
                        .to_string(),
                    e.player
                        .and_then(|p| self.current_match.team.find_player(p))
                        .map(|p| p.name.clone())
                        .unwrap_or_else(|| "-".to_string()),
                    Self::eval_name(e, e.eval),
                    Self::technique_name(e.technique),
                ]
            })
            .collect()
    }

    /// The width of each column: the widest of its texts.
    fn column_widths(cells: &[[String; 5]]) -> [u16; 5] {
        std::array::from_fn(|column| {
            cells
                .iter()
                .map(|row| Span::raw(row[column].as_str()).width() as u16)
                .max()
                .unwrap_or(0)
        })
    }

    /// The width the table takes to show the texts, the player shortened
    /// to `PLAYER_MIN_WIDTH`: with the space between the columns and the
    /// borders.
    fn events_table_width(widths: &[u16; 5]) -> u16 {
        let [number, event, player, eval, technique] = *widths;
        number + event + player.min(PLAYER_MIN_WIDTH) + eval + technique + 4 + 2
    }

    fn render_events(&mut self, f: &mut Frame, area: Rect) {
        let cells = self.event_cells();
        let [number, event, _, eval, technique] = Self::column_widths(&cells);
        let rows: Vec<Row> = cells.into_iter().map(Row::new).collect();
        // the player takes the width left: the column is wide enough for
        // at least `PLAYER_MIN_WIDTH` (see `events_table_width`)
        let table = Table::new(
            rows,
            [
                Constraint::Length(number),
                Constraint::Length(event),
                Constraint::Fill(1),
                Constraint::Length(eval),
                Constraint::Length(technique),
            ],
        )
        .row_highlight_style(Style::default().add_modifier(Modifier::REVERSED | Modifier::BOLD))
        .block(panel::section(current_labels().events));
        f.render_stateful_widget(table, area, &mut self.table_state);
    }

    fn render_form(&self, f: &mut Frame, area: Rect) {
        let Some(form) = &self.form else {
            return;
        };
        let labels = current_labels();
        let event = form.edit.event();
        let rows: Vec<Row> = form
            .edit
            .fields()
            .into_iter()
            .map(|field| {
                let (label, value) = match field {
                    EditField::EventType => (
                        labels.event,
                        event.event_type.friendly_name(labels.domain).to_string(),
                    ),
                    EditField::Eval => (labels.evaluation, Self::eval_name(event, event.eval)),
                    EditField::Technique => (
                        match event.event_type {
                            EventTypeEnum::S => labels.serve_type,
                            _ => labels.set_type,
                        },
                        Self::technique_name(event.technique),
                    ),
                };
                Row::new(vec![format!(" {label}"), format!("◀ {value} ▶")]).style(
                    match field == form.field {
                        true => Style::default().add_modifier(Modifier::REVERSED),
                        false => Style::default(),
                    },
                )
            })
            .collect();
        let table = Table::new(rows, [Constraint::Percentage(40), Constraint::Min(10)]).block(
            panel::section(format!("{} {}", labels.edit, self.selected() + 1)),
        );
        f.render_widget(table, area);
    }

    /// The actions bound to keys, and the footer listing them.
    fn footer_actions(&self) -> (Vec<Sba<ScreenActionEnum>>, Vec<(String, String)>) {
        let kb = navigation_keybindings();
        match self.form {
            Some(_) => {
                let actions = vec![
                    Sba::Simple(ScreenActionEnum::Confirm),
                    Sba::Redacted(ScreenActionEnum::Back, |_| {
                        current_labels().cancel.to_string()
                    }),
                ];
                let entries = navigation_entries()
                    .into_iter()
                    .chain([change_value_entry()])
                    .chain(get_keybinding_actions(kb, &actions))
                    .collect();
                (actions, entries)
            }
            None => {
                let mut actions = LIST_NAVIGATION.to_vec();
                actions.extend([
                    Sba::Redacted(ScreenActionEnum::Select, |_| {
                        current_labels().edit.to_string()
                    }),
                    Sba::Simple(ScreenActionEnum::Back),
                    Sba::Simple(ScreenActionEnum::Quit),
                ]);
                let entries = list_entries(kb, &actions);
                (actions, entries)
            }
        }
    }
}

/// The option after (or before) `current`, wrapping around if `wrap`; the
/// first one if `current` isn't among them.
fn cycle<T: PartialEq + Copy>(options: &[T], current: T, back: bool, wrap: bool) -> T {
    let len = options.len();
    let next = options
        .iter()
        .position(|o| *o == current)
        .map(|i| match (back, wrap, i) {
            (true, true, 0) => len - 1,
            (true, false, 0) => 0,
            (true, _, i) => i - 1,
            (false, true, i) => (i + 1) % len,
            (false, false, i) => (i + 1).min(len - 1),
        });
    match next {
        Some(i) => options[i],
        None => options.first().copied().unwrap_or(current),
    }
}
