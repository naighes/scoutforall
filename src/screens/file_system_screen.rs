use crate::{
    errors::AppError,
    localization::current_labels,
    screens::{
        components::notify_banner::NotifyBanner,
        screen::{AppAction, Screen},
    },
};
use crossterm::event::KeyCode;
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::Span,
    widgets::{Block, Borders, List, ListItem, ListState, Padding, Paragraph, Wrap},
    Frame,
};
use std::{
    fs,
    path::{Path, PathBuf},
};

pub trait FileSystemAction {
    fn on_selected(&mut self, path: &Path) -> Result<AppAction, AppError>;
    fn is_selectable(&self, path: &Path) -> bool;
    fn is_visible(&self, path: &Path) -> bool;
}

pub struct FileSystemScreen<A>
where
    A: FileSystemAction,
{
    current_folder: PathBuf,
    notify_message: NotifyBanner,
    list_state: ListState,
    entries: Vec<PathBuf>,
    title: String,
    action: A,
    back: bool,
}

impl<A> FileSystemScreen<A>
where
    A: FileSystemAction,
{
    pub fn new(initial_folder: PathBuf, title_label: &str, action: A) -> Self {
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

    fn render_footer(&mut self, f: &mut Frame, area: Rect) {
        let block = Block::default()
            .borders(Borders::NONE)
            .padding(Padding::new(1, 0, 0, 0));
        let mut menu: Vec<String> = vec![];
        if !self.entries.is_empty() {
            menu.push(format!("↑↓ = {}", current_labels().navigate));
        }
        if !self.is_root() {
            menu.push(format!("Backspace = {}", current_labels().up_one_level));
        }
        if self.list_state.selected().is_some() {
            menu.push(format!("Enter = {}", current_labels().select));
        }
        menu.push(format!("Esc = {}", current_labels().back));
        menu.push(format!("Q = {}", current_labels().quit));
        let paragraph = Paragraph::new(menu.join(" | "))
            .block(block)
            .wrap(Wrap { trim: true });
        f.render_widget(paragraph, area);
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

impl<A> Screen for FileSystemScreen<A>
where
    A: FileSystemAction,
{
    fn handle_key(&mut self, key: crossterm::event::KeyEvent) -> super::screen::AppAction {
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
                let child = self
                    .list_state
                    .selected()
                    .and_then(|s| self.entries.get(s))
                    .cloned();
                match child {
                    Some(child) => match self.action.is_selectable(&child) {
                        true => match self.action.on_selected(&child) {
                            Ok(_) => {
                                self.notify_message
                                    .set_info(current_labels().operation_successful.to_string());
                                self.back = true;
                                AppAction::None
                            }
                            Err(e) => {
                                self.notify_message.set_error(format!("{}", e));
                                AppAction::None
                            }
                        },
                        _ => self.enter_directory(&child),
                    },
                    None => AppAction::None,
                }
            }
            (KeyCode::Esc, _) => AppAction::Back(true, Some(1)),
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

    fn render(&mut self, f: &mut Frame, body: Rect, footer_left: Rect, footer_right: Rect) {
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
            self.render_empty_directory(f, body);
        } else {
            self.render_directory_content(f, body, items, &self.title.clone());
        }
        self.notify_message.render(f, footer_right);
        self.render_footer(f, footer_left);
    }

    fn on_resume(&mut self, _: bool) {}
}
