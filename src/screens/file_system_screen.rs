use crate::localization::current_labels;
use crate::shapes::keybinding::navigation_keybindings;
use crate::{
    providers::{settings_reader::SettingsReader, settings_writer::SettingsWriter},
    screens::{
        components::{navigation_footer::NavigationFooter, notify_banner::NotifyBanner, panel},
        screen::{AppAction, LIST_NAVIGATION, Renderable, Sba, ScreenAsync, cycle, list_entries},
    },
    shapes::{actions::ScreenActionEnum, keybinding::ActionsKeyBindings},
};
use async_trait::async_trait;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::Span,
    widgets::{Borders, List, ListItem, ListState, Paragraph},
};
use scout4all_core::errors::AppError;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};

#[async_trait]
pub trait FileSystemAction {
    async fn on_selected(&mut self, path: &Path) -> Result<AppAction, AppError>;
    fn is_selectable(&self, path: &Path) -> bool;
    fn is_visible(&self, path: &Path) -> bool;
    fn success_message_suffix(&self) -> Option<String> {
        None
    }
}

pub struct FileSystemScreen<
    A: Send + Sync,
    SR: SettingsReader + Send + Sync,
    SW: SettingsWriter + Send + Sync,
> where
    A: FileSystemAction,
{
    current_folder: PathBuf,
    notify_message: NotifyBanner,
    list_state: ListState,
    entries: Vec<PathBuf>,
    title: String,
    action: A,
    back: bool,
    settings_reader: Arc<SR>,
    settings_writer: Arc<SW>,
    screen_key_bindings: ActionsKeyBindings<ScreenActionEnum>,
    footer: NavigationFooter,
}

impl<A: Send + Sync, SR: SettingsReader + Send + Sync, SW: SettingsWriter + Send + Sync>
    FileSystemScreen<A, SR, SW>
where
    A: FileSystemAction,
{
    pub fn new(
        initial_folder: PathBuf,
        title_label: &str,
        action: A,
        settings_reader: Arc<SR>,
        settings_writer: Arc<SW>,
    ) -> Self {
        let entries = Self::compute_entries(&initial_folder, &action);
        let mut list_state = ListState::default();
        list_state.select(if entries.is_empty() { None } else { Some(0) });
        Self {
            current_folder: initial_folder,
            notify_message: NotifyBanner::new(),
            list_state,
            entries,
            title: title_label.to_string(),
            action,
            back: false,
            footer: NavigationFooter::new(),
            screen_key_bindings: ActionsKeyBindings::empty(),
            settings_reader,
            settings_writer,
        }
    }

    fn is_root(&self) -> bool {
        self.current_folder.parent().is_none()
    }

    fn move_selection(&mut self, forward: bool) {
        if let Some(selected) = self.list_state.selected() {
            self.list_state
                .select(cycle(selected, self.entries.len(), forward));
        }
    }

    fn next(&mut self) {
        self.move_selection(true);
    }

    fn previous(&mut self) {
        self.move_selection(false);
    }

    fn enter_directory(&mut self, path: &Path) -> AppAction {
        self.current_folder = path.to_path_buf();
        self.entries = Self::compute_entries(&self.current_folder, &self.action);
        self.list_state.select(if self.entries.is_empty() {
            None
        } else {
            Some(0)
        });
        AppAction::None
    }

    fn compute_entries(folder: &PathBuf, action: &A) -> Vec<PathBuf> {
        match fs::read_dir(folder) {
            Ok(d) => d
                .filter_map(|entry| entry.ok().map(|e| e.path()))
                .filter(|x| action.is_visible(x))
                .collect(),
            Err(_) => vec![],
        }
    }

    async fn save_selected_directory(&mut self, child: &Path) {
        let dir_to_save = if child.is_dir() {
            Some(child.to_path_buf())
        } else {
            child.parent().map(|p| p.to_path_buf())
        };
        if let Some(dir) = dir_to_save
            && let Ok(mut settings) = self.settings_reader.read().await
        {
            settings.last_used_dir = Some(dir.clone());
            let _ = self.settings_writer.save(settings).await;
        }
    }

    fn get_footer_actions(&self) -> Vec<Sba<ScreenActionEnum>> {
        let actions = &mut vec![];

        if !self.entries.is_empty() {
            actions.extend(LIST_NAVIGATION);
        }
        if let Some(true) = self
            .list_state
            .selected()
            .and_then(|s| self.entries.get(s))
            .map(|s| s.is_dir())
        {
            actions.push(Sba::Simple(ScreenActionEnum::EnterDirectory));
        }
        if !self.is_root() {
            actions.push(Sba::Simple(ScreenActionEnum::OneLevelUp));
        }
        if self.list_state.selected().is_some() {
            actions.push(Sba::Simple(ScreenActionEnum::Select));
        }
        actions.push(Sba::Simple(ScreenActionEnum::Back));
        actions.push(Sba::Simple(ScreenActionEnum::Quit));
        actions.clone()
    }

    fn render_directory_content(
        &mut self,
        f: &mut Frame,
        area: Rect,
        items: Vec<ListItem>,
        title_label: &str,
    ) {
        let list = List::new(items)
            .block(panel::screen(title_label))
            .highlight_style(
                Style::default()
                    .add_modifier(Modifier::BOLD)
                    .add_modifier(Modifier::REVERSED),
            )
            .highlight_symbol(">> ");
        f.render_stateful_widget(list, area, &mut self.list_state);
    }
}

impl<A: Send + Sync, SR: SettingsReader + Send + Sync, SW: SettingsWriter + Send + Sync> Renderable
    for FileSystemScreen<A, SR, SW>
where
    A: FileSystemAction,
{
    fn footer_entries(&self) -> Vec<(String, String)> {
        list_entries(navigation_keybindings(), &self.get_footer_actions())
    }

    fn render(&mut self, f: &mut Frame, body: Rect, footer_left: Rect, footer_right: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(1)])
            .split(body);
        let header = Paragraph::new(self.current_folder.to_string_lossy().to_string())
            .block(panel::section(current_labels().current_directory).borders(Borders::BOTTOM))
            .style(Style::default().fg(Color::Cyan));
        f.render_widget(header, chunks[0]);
        let items: Vec<ListItem> = self
            .entries
            .iter()
            .map(|t| {
                let name = t.file_name().unwrap_or_default().to_string_lossy();
                let (content, style) = if t.is_dir() {
                    (name.to_string(), Style::default().fg(Color::Yellow))
                } else {
                    (name.to_string(), Style::default().fg(Color::White))
                };
                ListItem::new(Span::styled(content, style))
            })
            .collect();

        if items.is_empty() {
            panel::render_empty(f, chunks[1], current_labels().empty_directory);
        } else {
            self.render_directory_content(f, chunks[1], items, &self.title.clone());
        }

        let actions = &self.get_footer_actions();

        self.notify_message.render(f, footer_right);
        let kb = navigation_keybindings();
        self.footer.render(f, footer_left, self.footer_entries());

        self.screen_key_bindings = kb.slice(Sba::keys(actions));
    }
}

#[async_trait]
impl<A: Send + Sync, SR: SettingsReader + Send + Sync, SW: SettingsWriter + Send + Sync> ScreenAsync
    for FileSystemScreen<A, SR, SW>
where
    A: FileSystemAction,
{
    async fn handle_key(
        &mut self,
        key: crokey::crossterm::event::KeyEvent,
    ) -> super::screen::AppAction {
        if let Some(key_combination) = self.screen_key_bindings.transform(key) {
            match (
                self.screen_key_bindings.get(key_combination),
                key.code,
                &self.notify_message.has_value(),
            ) {
                (_, _, true) => {
                    self.notify_message.reset();
                    if self.back {
                        AppAction::Back(true, Some(1))
                    } else {
                        AppAction::None
                    }
                }
                (Some(ScreenActionEnum::Next | ScreenActionEnum::Down), _, _) => {
                    self.next();
                    AppAction::None
                }
                (Some(ScreenActionEnum::Previous | ScreenActionEnum::Up), _, _) => {
                    self.previous();
                    AppAction::None
                }
                (Some(ScreenActionEnum::Select), _, _) => {
                    let current = self.current_folder.clone();
                    if let Some(selected_path) =
                        &self.list_state.selected().and_then(|s| self.entries.get(s))
                    {
                        match self.action.is_selectable(selected_path) {
                            true => match self.action.on_selected(selected_path).await {
                                Ok(_) => {
                                    self.save_selected_directory(&current).await;
                                    let message = if let Some(suffix) =
                                        self.action.success_message_suffix()
                                    {
                                        format!(
                                            "{}: {}",
                                            current_labels().operation_successful,
                                            suffix
                                        )
                                    } else {
                                        current_labels().operation_successful.to_string()
                                    };
                                    self.notify_message.set_info(message);
                                    self.back = true;
                                    AppAction::None
                                }
                                Err(e) => {
                                    self.notify_message.set_error(format!("{}", e));
                                    AppAction::None
                                }
                            },
                            _ => {
                                self.notify_message
                                    .set_error(current_labels().invalid_selection.to_string());
                                AppAction::None
                            }
                        }
                    } else {
                        self.notify_message
                            .set_error(current_labels().invalid_selection.to_string());
                        AppAction::None
                    }
                }
                (Some(ScreenActionEnum::Back), _, _) => AppAction::Back(true, Some(1)),
                (Some(ScreenActionEnum::Quit), _, _) => AppAction::Quit(Ok(())),
                (Some(ScreenActionEnum::EnterDirectory), _, _) => {
                    let child = self
                        .list_state
                        .selected()
                        .and_then(|s| self.entries.get(s))
                        .cloned();
                    match child {
                        Some(child) => match child.is_dir() {
                            true => {
                                self.enter_directory(&child);
                                AppAction::None
                            }
                            _ => AppAction::None,
                        },
                        None => AppAction::None,
                    }
                }
                (Some(ScreenActionEnum::OneLevelUp), _, _) => {
                    let parent = self.current_folder.parent().map(|p| p.to_path_buf());
                    if let Some(parent) = parent {
                        self.enter_directory(&parent);
                    }
                    AppAction::None
                }
                _ => AppAction::None,
            }
        } else {
            AppAction::None
        }
    }

    async fn refresh_data(&mut self) {}
}
