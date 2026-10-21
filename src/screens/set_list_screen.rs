use crate::{
    screens::{
        components::{
            navigation_footer::NavigationFooter, notify_banner::NotifyBanner,
            team_header::TeamHeader,
        },
        edit_set_events_screen::EditSetEventsScreen,
        screen::{AppAction, Renderable, Sba, ScreenAsync, get_keybinding_actions},
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
    widgets::{Block, Borders, Row, Table},
};
use scout4all_core::{
    localization::current_labels,
    providers::{match_reader::MatchReader, set_writer::SetWriter},
    shapes::r#match::MatchEntry,
};
use std::sync::Arc;

/// The sets of a match, to choose the one whose events are edited.
#[derive(Debug)]
pub struct SetListScreen<MR: MatchReader + Send + Sync, SSW: SetWriter + Send + Sync> {
    current_match: MatchEntry,
    selected: usize,
    notify_message: NotifyBanner,
    header: TeamHeader,
    footer: NavigationFooter,
    match_reader: Arc<MR>,
    set_writer: Arc<SSW>,
    screen_key_bindings: ActionsKeyBindings<ScreenActionEnum>,
}

/// The keys of the set list.
const SET_LIST_ACTIONS: [Sba<ScreenActionEnum>; 5] = [
    Sba::Simple(ScreenActionEnum::Up),
    Sba::Simple(ScreenActionEnum::Down),
    Sba::Redacted(ScreenActionEnum::Select, |_| {
        current_labels().edit_events.to_string()
    }),
    Sba::Simple(ScreenActionEnum::Back),
    Sba::Simple(ScreenActionEnum::Quit),
];

impl<MR: MatchReader + Send + Sync + 'static, SSW: SetWriter + Send + Sync + 'static> Renderable
    for SetListScreen<MR, SSW>
{
    fn footer_entries(&self) -> Vec<(String, String)> {
        get_keybinding_actions(navigation_keybindings(), &SET_LIST_ACTIONS)
    }

    fn render(&mut self, f: &mut Frame, body: Rect, footer_left: Rect, footer_right: Rect) {
        let container = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(5), Constraint::Min(1)])
            .split(body);
        let rows: Vec<Row> = self
            .set_rows()
            .into_iter()
            .enumerate()
            .map(|(i, cells)| {
                Row::new(cells).style(match i == self.selected {
                    true => Style::default().add_modifier(Modifier::REVERSED | Modifier::BOLD),
                    false => Style::default(),
                })
            })
            .collect();
        let table = Table::new(
            rows,
            [
                Constraint::Length(10),
                Constraint::Length(12),
                Constraint::Length(20),
            ],
        )
        .block(Block::default().borders(Borders::ALL).title(format!(
            "{} - {}",
            current_labels().choose_the_set,
            self.current_match.opponent
        )));
        f.render_widget(table, container[1]);
        self.header
            .render(f, container[0], Some(&self.current_match.team));
        self.notify_message.render(f, footer_right);
        self.footer.render(f, footer_left, self.footer_entries());
        self.screen_key_bindings = navigation_keybindings().slice(Sba::keys(&SET_LIST_ACTIONS));
    }
}

#[async_trait]
impl<MR: MatchReader + Send + Sync + 'static, SSW: SetWriter + Send + Sync + 'static> ScreenAsync
    for SetListScreen<MR, SSW>
{
    async fn handle_key(&mut self, key: KeyEvent) -> AppAction {
        let Some(key_combination) = self.screen_key_bindings.transform(key) else {
            return AppAction::None;
        };
        match (
            self.notify_message.has_value(),
            self.screen_key_bindings.get(key_combination),
        ) {
            (true, _) => {
                self.notify_message.reset();
                AppAction::None
            }
            (_, Some(ScreenActionEnum::Up)) => {
                self.selected = self.selected.saturating_sub(1);
                AppAction::None
            }
            (_, Some(ScreenActionEnum::Down)) => {
                self.selected = (self.selected + 1).min(self.sets_count().saturating_sub(1));
                AppAction::None
            }
            (_, Some(ScreenActionEnum::Select)) => self.edit_selected_set(),
            (_, Some(ScreenActionEnum::Back)) => AppAction::Back(true, Some(1)),
            (_, Some(ScreenActionEnum::Quit)) => AppAction::Quit(Ok(())),
            _ => AppAction::None,
        }
    }

    async fn refresh_data(&mut self) {
        // the scores change with the events edited
        match self
            .match_reader
            .read_single(&self.current_match.team, &self.current_match.id)
            .await
        {
            Ok(m) => self.current_match = m,
            Err(_) => self
                .notify_message
                .set_error(current_labels().could_not_load_matches.to_string()),
        }
    }
}

impl<MR: MatchReader + Send + Sync + 'static, SSW: SetWriter + Send + Sync + 'static>
    SetListScreen<MR, SSW>
{
    pub fn new(current_match: MatchEntry, match_reader: Arc<MR>, set_writer: Arc<SSW>) -> Self {
        SetListScreen {
            current_match,
            selected: 0,
            notify_message: NotifyBanner::new(),
            header: TeamHeader::default(),
            footer: NavigationFooter::new(),
            match_reader,
            set_writer,
            screen_key_bindings: ActionsKeyBindings::empty(),
        }
    }

    fn sets_count(&self) -> usize {
        self.current_match.sets.len()
    }

    /// The set numbers in order.
    fn set_numbers(&self) -> Vec<u8> {
        let mut numbers: Vec<u8> = self
            .current_match
            .sets
            .iter()
            .map(|s| s.set_number)
            .collect();
        numbers.sort();
        numbers
    }

    /// The set, its score (the home team first) and whether it's over.
    fn set_rows(&self) -> Vec<Vec<String>> {
        self.set_numbers()
            .into_iter()
            .map(|n| {
                let snapshot = self.current_match.get_set_snapshot(n).ok().flatten();
                let score = snapshot
                    .as_ref()
                    .map(|s| match self.current_match.home {
                        true => format!("{} – {}", s.score_us, s.score_them),
                        false => format!("{} – {}", s.score_them, s.score_us),
                    })
                    .unwrap_or_default();
                let status = match snapshot.and_then(|s| s.get_set_winner(n)) {
                    Some(_) => "",
                    None => current_labels().in_progress,
                };
                vec![format!(" set {n}"), score, status.to_string()]
            })
            .collect()
    }

    fn edit_selected_set(&mut self) -> AppAction {
        let set = self
            .set_numbers()
            .get(self.selected)
            .and_then(|n| self.current_match.sets.iter().find(|s| s.set_number == *n))
            .cloned();
        match set.map(|set| {
            EditSetEventsScreen::new(self.current_match.clone(), set, self.set_writer.clone())
        }) {
            Some(Ok(screen)) => AppAction::SwitchScreen(Box::new(screen)),
            Some(Err(_)) | None => {
                self.notify_message
                    .set_error(current_labels().could_not_compute_snapshot.to_string());
                AppAction::None
            }
        }
    }
}
