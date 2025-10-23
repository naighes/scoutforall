use crate::{
    errors::AppError,
    localization::current_labels,
    providers::{settings_reader::SettingsReader, settings_writer::SettingsWriter},
    screens::{
        components::{navigation_footer::NavigationFooter, notify_banner::NotifyBanner},
        screen::{AppAction, Renderable, ScreenAsync},
    },
};
use async_trait::async_trait;
use crossterm::event::KeyCode;
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::Span,
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Frame,
};
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
    footer: NavigationFooter,
    settings_reader: Arc<SR>,
    settings_writer: Arc<SW>,
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
            settings_reader,
            settings_writer,
        }
    }

    fn is_root(&self) -> bool {
        self.current_folder.parent().is_none()
    }

    fn move_selection<F>(&mut self, f: F)
    where
        F: Fn(usize, usize) -> usize,
    {
        if let Some(selected) = self.list_state.selected() {
            if self.entries.is_empty() {
                self.list_state.select(None);
            } else {
                let new_index = f(selected, self.entries.len());
                self.list_state.select(Some(new_index));
            }
        }
    }

    fn next(&mut self) {
        self.move_selection(
            |selected, len| {
                if selected < len - 1 {
                    selected + 1
                } else {
                    0
                }
            },
        );
    }

    fn previous(&mut self) {
        self.move_selection(
            |selected, len| {
                if selected > 0 {
                    selected - 1
                } else {
                    len - 1
                }
            },
        );
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
        if let Some(dir) = dir_to_save {
            if let Ok(mut settings) = self.settings_reader.read().await {
                settings.last_used_dir = Some(dir.clone());
                let _ = self.settings_writer.save(settings).await;
            }
        }
    }

    fn render_empty_directory(&self, f: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Percentage(40),
                Constraint::Length(3),
                Constraint::Percentage(40),
            ])
            .split(area);
        let paragraph = Paragraph::new(current_labels().empty_directory)
            .block(Block::default().borders(Borders::NONE))
            .alignment(Alignment::Center);
        f.render_widget(paragraph, chunks[1]);
    }

    fn get_footer_entries(&self) -> Vec<(String, String)> {
        let mut entries: Vec<(String, String)> = vec![];
        if !self.entries.is_empty() {
            entries.push(("↑↓".to_string(), current_labels().navigate.to_string()));
        }
        if let Some(true) = self
            .list_state
            .selected()
            .and_then(|s| self.entries.get(s))
            .map(|s| s.is_dir())
        {
            entries.push((
                current_labels().space.to_string(),
                current_labels().enter_directory.to_string(),
            ));
        }
        if !self.is_root() {
            entries.push((
                "Backspace".to_string(),
                current_labels().up_one_level.to_string(),
            ));
        }
        if self.list_state.selected().is_some() {
            entries.push((
                current_labels().enter.to_string(),
                current_labels().select.to_string(),
            ));
        }
        entries.push(("Esc".to_string(), current_labels().back.to_string()));
        entries.push(("Q".to_string(), current_labels().quit.to_string()));
        entries
    }

    fn render_directory_content(
        &mut self,
        f: &mut Frame,
        area: Rect,
        items: Vec<ListItem>,
        title_label: &str,
    ) {
        let list = List::new(items)
            .block(Block::default().borders(Borders::ALL).title(title_label))
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
    fn render(&mut self, f: &mut Frame, body: Rect, footer_left: Rect, footer_right: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(1)])
            .split(body);
        let header = Paragraph::new(self.current_folder.to_string_lossy().to_string())
            .block(
                Block::default()
                    .borders(Borders::BOTTOM)
                    .title(current_labels().current_directory.to_string()),
            )
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
            self.render_empty_directory(f, chunks[1]);
        } else {
            self.render_directory_content(f, chunks[1], items, &self.title.clone());
        }
        self.notify_message.render(f, footer_right);
        self.footer
            .render(f, footer_left, self.get_footer_entries().clone());
    }
}

#[async_trait]
impl<A: Send + Sync, SR: SettingsReader + Send + Sync, SW: SettingsWriter + Send + Sync> ScreenAsync
    for FileSystemScreen<A, SR, SW>
where
    A: FileSystemAction,
{
    async fn handle_key(&mut self, key: crossterm::event::KeyEvent) -> super::screen::AppAction {
        match (key.code, &self.notify_message.has_value()) {
            (_, true) => {
                self.notify_message.reset();
                if self.back {
                    AppAction::Back(true, Some(1))
                } else {
                    AppAction::None
                }
            }
            (KeyCode::Down, _) => {
                self.next();
                AppAction::None
            }
            (KeyCode::Up, _) => {
                self.previous();
                AppAction::None
            }
            (KeyCode::Enter, _) => {
                let current = self.current_folder.clone();
                match self.action.is_selectable(&current) {
                    true => match self.action.on_selected(&current).await {
                        Ok(_) => {
                            self.save_selected_directory(&current).await;
                            let message = if let Some(suffix) = self.action.success_message_suffix()
                            {
                                format!("{}: {}", current_labels().operation_successful, suffix)
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
            }
            (KeyCode::Esc, _) => AppAction::Back(true, Some(1)),
            (KeyCode::Char(' '), _) => {
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
            (KeyCode::Backspace, _) => {
                let parent = self.current_folder.parent().map(|p| p.to_path_buf());
                if let Some(parent) = parent {
                    self.enter_directory(&parent);
                }
                AppAction::None
            }
            _ => AppAction::None,
        }
    }

    async fn refresh_data(&mut self) {}
}
