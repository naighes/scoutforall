use crate::localization::current_labels;
use crate::{
    providers::{settings_reader::SettingsReader, settings_writer::SettingsWriter},
    screens::{
        components::{
            navigation_footer::NavigationFooter, notify_banner::NotifyBanner, panel,
            team_header::TeamHeader,
        },
        screen::{
            AppAction, FieldKey, Renderable, Sba, ScreenAsync, change_value_entry,
            get_keybinding_actions, navigation_entries,
        },
        start_set_screen::StartSetScreen,
    },
    shapes::{
        actions::ScreenActionEnum,
        keybinding::{ActionsKeyBindings, navigation_keybindings},
        settings::Settings,
    },
};
use async_trait::async_trait;
use crokey::crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    widgets::{Row, Table, TableState},
};
use scout4all_core::{
    constants::MAX_ROSTER,
    match_form::{MatchForm, MatchInput, RosterChoice},
    providers::{match_writer::MatchWriter, set_writer::SetWriter},
    set_start::SetStartForm,
    shapes::{r#match::MatchEntry, team::TeamEntry},
};
use std::sync::Arc;

/// The keys of the roster: the rows, the change of a row, saving.
const ROSTER_ACTIONS: [Sba<ScreenActionEnum>; 2] = [
    Sba::Simple(ScreenActionEnum::Confirm),
    Sba::Simple(ScreenActionEnum::Back),
];

/// The players on the score sheet of the match of the form, the last step
/// of the match form: saves the match, new (then its first set starts) or
/// `editing`. Who played in a set of the match stays.
#[derive(Debug)]
pub struct MatchRosterScreen<
    MW: MatchWriter + Send + Sync,
    SSW: SetWriter + Send + Sync,
    SR: SettingsReader + Send + Sync + 'static,
    SW: SettingsWriter + Send + Sync + 'static,
> {
    settings_reader: Arc<SR>,
    settings_writer: Arc<SW>,
    settings: Settings,
    match_writer: Arc<MW>,
    set_writer: Arc<SSW>,
    team: TeamEntry,
    form: MatchForm,
    editing: Option<MatchEntry>,
    table_state: TableState,
    notify_message: NotifyBanner,
    header: TeamHeader,
    footer: NavigationFooter,
    screen_key_bindings: ActionsKeyBindings<ScreenActionEnum>,
}

impl<
    MW: MatchWriter + Send + Sync + 'static,
    SSW: SetWriter + Send + Sync + 'static,
    SR: SettingsReader + Send + Sync + 'static,
    SW: SettingsWriter + Send + Sync + 'static,
> Renderable for MatchRosterScreen<MW, SSW, SR, SW>
{
    fn footer_entries(&self) -> Vec<(String, String)> {
        navigation_entries()
            .into_iter()
            .chain([change_value_entry()])
            .chain(get_keybinding_actions(
                navigation_keybindings(),
                &ROSTER_ACTIONS,
            ))
            .collect()
    }

    fn render(&mut self, f: &mut Frame, body: Rect, footer_left: Rect, footer_right: Rect) {
        let container = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(5), Constraint::Min(1)])
            .split(body);
        let choices = self.choices();
        let chosen = choices.iter().filter(|c| c.chosen).count();
        let rows: Vec<Row> = choices.iter().map(row).collect();
        let widths = [
            Constraint::Length(4),
            Constraint::Length(4),
            Constraint::Fill(1),
            Constraint::Length(content_width(&choices)),
        ];
        let table = Table::new(rows, widths)
            .header(
                Row::new(vec!["", "#", current_labels().name, current_labels().role])
                    .style(Style::default().add_modifier(Modifier::BOLD)),
            )
            .block(panel::screen(format!(
                "{} - {}/{MAX_ROSTER}",
                current_labels().roster,
                chosen
            )))
            .row_highlight_style(
                Style::default().add_modifier(Modifier::REVERSED | Modifier::BOLD),
            );
        f.render_stateful_widget(table, container[1], &mut self.table_state);
        self.header.render(f, container[0], Some(&self.team));
        self.notify_message.render(f, footer_right);
        self.footer.render(f, footer_left, self.footer_entries());
    }
}

/// The row of a player: in or out, locked once played.
fn row(choice: &RosterChoice) -> Row<'static> {
    let mark = match (choice.chosen, choice.played) {
        (true, true) => "[*]",
        (true, false) => "[X]",
        (false, _) => "[ ]",
    };
    Row::new(vec![
        mark.to_string(),
        choice.player.number.to_string(),
        choice.player.name.clone(),
        choice
            .player
            .role
            .map_or_else(|| "-".to_string(), |r| r.to_string()),
    ])
}

/// The width of the roles and of their title, to show them whole.
fn content_width(choices: &[RosterChoice]) -> u16 {
    choices
        .iter()
        .map(|c| c.player.role.map_or(1, |r| r.to_string().chars().count()))
        .chain([current_labels().role.chars().count()])
        .max()
        .map_or(1, |w| u16::try_from(w).unwrap_or(u16::MAX))
}

#[async_trait]
impl<
    MW: MatchWriter + Send + Sync + 'static,
    SSW: SetWriter + Send + Sync + 'static,
    SR: SettingsReader + Send + Sync + 'static,
    SW: SettingsWriter + Send + Sync + 'static,
> ScreenAsync for MatchRosterScreen<MW, SSW, SR, SW>
{
    async fn handle_key(&mut self, key: KeyEvent) -> AppAction {
        let Some(key_combination) = self.screen_key_bindings.transform(key) else {
            return AppAction::None;
        };
        match (
            self.notify_message.has_value(),
            self.screen_key_bindings.get(key_combination),
            FieldKey::of(key.code),
            key.code,
        ) {
            (true, _, _, _) => {
                self.notify_message.reset();
                AppAction::None
            }
            (_, _, Some(FieldKey::Previous), _) => self.move_by(-1),
            (_, _, Some(FieldKey::Next), _) => self.move_by(1),
            (_, _, Some(FieldKey::Change { .. }), _) | (_, None, _, KeyCode::Char(' ')) => {
                self.toggle_selected()
            }
            (_, Some(ScreenActionEnum::Confirm), _, _) => self.handle_confirm().await,
            (_, Some(ScreenActionEnum::Back), _, _) => AppAction::Back(false, Some(1)),
            _ => AppAction::None,
        }
    }

    async fn refresh_data(&mut self) {}
}

impl<
    MW: MatchWriter + Send + Sync + 'static,
    SSW: SetWriter + Send + Sync + 'static,
    SR: SettingsReader + Send + Sync + 'static,
    SW: SettingsWriter + Send + Sync + 'static,
> MatchRosterScreen<MW, SSW, SR, SW>
{
    pub fn new(
        settings_reader: Arc<SR>,
        settings_writer: Arc<SW>,
        settings: Settings,
        match_writer: Arc<MW>,
        set_writer: Arc<SSW>,
        team: TeamEntry,
        form: MatchForm,
        editing: Option<MatchEntry>,
    ) -> Self {
        MatchRosterScreen {
            settings_reader,
            settings_writer,
            settings,
            match_writer,
            set_writer,
            team,
            form,
            editing,
            table_state: TableState::default().with_selected(Some(0)),
            notify_message: NotifyBanner::new(),
            header: TeamHeader::default(),
            footer: NavigationFooter::new(),
            screen_key_bindings: navigation_keybindings().slice(Sba::keys(&ROSTER_ACTIONS)),
        }
    }

    fn choices(&self) -> Vec<RosterChoice> {
        self.form.roster_choices(&self.team, self.editing.as_ref())
    }

    /// The row `by` rows away, within the list.
    fn move_by(&mut self, by: isize) -> AppAction {
        let last = self.choices().len().saturating_sub(1);
        let selected = self.table_state.selected().unwrap_or(0);
        self.table_state
            .select(Some(selected.saturating_add_signed(by).min(last)));
        AppAction::None
    }

    fn toggle_selected(&mut self) -> AppAction {
        let player = self
            .table_state
            .selected()
            .and_then(|i| self.choices().get(i).map(|c| c.player.id));
        if let Some(player) = player {
            self.form.toggle(player, &self.team, self.editing.as_ref());
        }
        AppAction::None
    }

    async fn handle_confirm(&mut self) -> AppAction {
        match (
            self.form
                .clone()
                .validate(&self.team, self.editing.as_ref()),
            self.editing.clone(),
        ) {
            (Err(e), _) => {
                self.notify_message
                    .set_error(e.message(current_labels().domain));
                AppAction::None
            }
            (Ok(input), Some(m)) => self.handle_update(m, input).await,
            (Ok(input), None) => self.handle_create(input).await,
        }
    }

    async fn handle_create(&mut self, input: MatchInput) -> AppAction {
        match self
            .match_writer
            .create(
                &self.team,
                input.opponent,
                input.date,
                input.home,
                input.max_substitutions,
                input.notes,
                input.roster,
            )
            .await
        {
            // back from the set, to the matches: the set, the roster and
            // the form
            Ok(m) => AppAction::SwitchScreen(Box::new(StartSetScreen::new(
                self.settings_reader.clone(),
                self.settings_writer.clone(),
                self.settings.clone(),
                m,
                SetStartForm::new(1, None),
                Some(3),
                self.set_writer.clone(),
            ))),
            Err(_) => {
                self.notify_message
                    .set_error(current_labels().could_not_create_match.to_string());
                AppAction::None
            }
        }
    }

    async fn handle_update(&mut self, m: MatchEntry, input: MatchInput) -> AppAction {
        match self
            .match_writer
            .update(
                &m,
                input.opponent,
                input.date,
                input.home,
                input.max_substitutions,
                input.notes,
                input.roster,
            )
            .await
        {
            // to the matches: the roster and the form
            Ok(()) => AppAction::Back(true, Some(2)),
            Err(_) => {
                self.notify_message
                    .set_error(current_labels().could_not_update_match.to_string());
                AppAction::None
            }
        }
    }
}
