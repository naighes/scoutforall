use std::sync::Arc;

use crate::{
    providers::settings_writer::SettingsWriter,
    screens::{
        components::{navigation_footer::NavigationFooter, panel},
        screen::{AppAction, Renderable, Sba, ScreenAsync, cycle, get_keybinding_actions},
    },
    shapes::{
        actions::ScreenActionEnum,
        keybinding::{ActionsKeyBindings, navigation_keybindings},
        settings::{Settings, TERMS_VERSION, set_settings},
        symbol::KeyCombinationFormatExt,
    },
};
use async_trait::async_trait;
use crokey::{
    KeyCombinationFormat,
    crossterm::event::{KeyCode, KeyEvent},
};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    widgets::{Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap},
};
use scout4all_core::{
    localization::current_labels, logging::logger::log_error, shapes::enums::FriendlyName,
    shapes::enums::LanguageEnum,
};

/// The terms of use, shown at startup until the user has seen them: the
/// program is free, its license, the data it sends. The user chooses the
/// language and whether to send the statistics, then goes on.
#[derive(Debug)]
pub struct TermsScreen<SW: SettingsWriter + Send + Sync> {
    settings: Settings,
    settings_writer: Arc<SW>,
    /// the first line of the terms shown, kept within their length
    scroll: u16,
    footer: NavigationFooter,
    screen_key_bindings: ActionsKeyBindings<ScreenActionEnum>,
    format: KeyCombinationFormat,
}

impl<SW> Renderable for TermsScreen<SW>
where
    SW: SettingsWriter + Send + Sync + 'static,
{
    fn footer_entries(&self) -> Vec<(String, String)> {
        let labels = current_labels();
        let fmt = KeyCombinationFormatExt::new(&self.format);
        let mut entries = get_keybinding_actions(
            navigation_keybindings(),
            &[
                Sba::Simple(ScreenActionEnum::ScrollUp),
                Sba::Simple(ScreenActionEnum::ScrollDown),
            ],
        );
        entries.push((
            format!(
                "{}/{}",
                fmt.to_string(KeyCode::Left),
                fmt.to_string(KeyCode::Right)
            ),
            labels.language.to_string(),
        ));
        entries.push((
            fmt.to_string(KeyCode::Char(' ')),
            labels.send_statistics.to_string(),
        ));
        entries.extend(get_keybinding_actions(
            navigation_keybindings(),
            &[
                Sba::Redacted(ScreenActionEnum::Confirm, |_| {
                    current_labels().proceed.to_string()
                }),
                Sba::Simple(ScreenActionEnum::Quit),
            ],
        ));
        entries
    }

    fn render(&mut self, f: &mut Frame, body: Rect, footer_left: Rect, _footer_right: Rect) {
        // the language and the statistics, a line each
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(3), Constraint::Length(4)])
            .split(body);
        self.render_terms(f, chunks[0]);
        self.render_choices(f, chunks[1]);
        self.footer.render(f, footer_left, self.footer_entries());
    }
}

#[async_trait]
impl<SW> ScreenAsync for TermsScreen<SW>
where
    SW: SettingsWriter + Send + Sync + 'static,
{
    async fn handle_key(&mut self, key: KeyEvent) -> AppAction {
        let Some(key_combination) = self.screen_key_bindings.transform(key) else {
            return AppAction::None;
        };
        match (key.code, self.screen_key_bindings.get(key_combination)) {
            (KeyCode::Left | KeyCode::Right, _) => self.change_language(key.code == KeyCode::Right),
            (KeyCode::Char(' '), _) => self.toggle_analytics(),
            (_, Some(ScreenActionEnum::ScrollUp)) => self.scroll = self.scroll.saturating_sub(1),
            // kept within the terms when rendered
            (_, Some(ScreenActionEnum::ScrollDown)) => self.scroll = self.scroll.saturating_add(1),
            (_, Some(ScreenActionEnum::Confirm)) => return self.proceed().await,
            (_, Some(ScreenActionEnum::Quit)) => return AppAction::Quit(Ok(())),
            _ => {}
        }
        AppAction::None
    }

    async fn refresh_data(&mut self) {}
}

impl<SW> TermsScreen<SW>
where
    SW: SettingsWriter + Send + Sync + 'static,
{
    pub fn new(settings: Settings, settings_writer: Arc<SW>) -> Self {
        let screen_key_bindings = navigation_keybindings().slice(vec![
            &ScreenActionEnum::ScrollUp,
            &ScreenActionEnum::ScrollDown,
            &ScreenActionEnum::Confirm,
            &ScreenActionEnum::Quit,
        ]);
        TermsScreen {
            settings,
            settings_writer,
            scroll: 0,
            footer: NavigationFooter::new(),
            screen_key_bindings,
            format: KeyCombinationFormat::default(),
        }
    }

    /// the next or previous language, shown right away: saved on going on
    fn change_language(&mut self, forward: bool) {
        let languages = LanguageEnum::ALL;
        let current = languages
            .iter()
            .position(|l| *l == self.settings.language)
            .unwrap_or_default();
        if let Some(language) =
            cycle(current, languages.len(), forward).and_then(|i| languages.get(i))
        {
            self.settings.language = *language;
            // no uploads yet: the terms aren't seen until the user goes on
            set_settings(self.settings.clone());
        }
    }

    fn toggle_analytics(&mut self) {
        self.settings.analytics_enabled = !self.settings.analytics_enabled;
    }

    /// Saves the choices with the terms seen, then shows the teams: if the
    /// settings can't be saved the choices apply all the same, and the terms
    /// are shown again at the next startup.
    async fn proceed(&mut self) -> AppAction {
        let settings = Settings {
            terms_version: TERMS_VERSION,
            ..self.settings.clone()
        };
        match self.settings_writer.save(settings.clone()).await {
            Ok(saved) => set_settings(saved),
            Err(e) => {
                log_error(&format!("could not save the terms of use as seen: {e}"));
                set_settings(settings);
            }
        }
        AppAction::Back(true, Some(1))
    }

    /// The terms, scrolled within their length, with a scrollbar on the
    /// right when they don't fit.
    fn render_terms(&mut self, f: &mut Frame, area: Rect) {
        let labels = current_labels();
        let block = panel::screen(labels.terms_of_use);
        let inner = block.inner(area);
        f.render_widget(block, area);
        let text = Rect {
            width: inner.width.saturating_sub(1),
            ..inner
        };
        let paragraph = Paragraph::new(labels.terms_text).wrap(Wrap { trim: true });
        let lines = u16::try_from(paragraph.line_count(text.width)).unwrap_or(u16::MAX);
        let max_scroll = lines.saturating_sub(text.height);
        self.scroll = self.scroll.min(max_scroll);
        f.render_widget(paragraph.scroll((self.scroll, 0)), text);
        if max_scroll > 0 {
            let mut state =
                ScrollbarState::new(usize::from(max_scroll) + 1).position(usize::from(self.scroll));
            f.render_stateful_widget(
                Scrollbar::new(ScrollbarOrientation::VerticalRight),
                inner,
                &mut state,
            );
        }
    }

    fn render_choices(&self, f: &mut Frame, area: Rect) {
        let labels = current_labels();
        let checkbox = match self.settings.analytics_enabled {
            true => "[X]",
            false => "[ ]",
        };
        let choices = Paragraph::new(vec![
            format!(
                "{}: ◀ {} ▶",
                labels.language,
                self.settings.language.friendly_name(labels)
            )
            .into(),
            format!("{checkbox} {}", labels.enable_send_analytics).into(),
        ])
        .block(panel::section(labels.settings));
        f.render_widget(choices, area);
    }
}
