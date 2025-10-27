use std::env::var;

use crate::{
    localization::current_labels,
    screens::{
        components::{
            navigation_footer::NavigationFooter, notify_banner::NotifyBanner, text_box::TextBox,
        },
        screen::{get_keybinding_actions, AppAction, Renderable, Sba, ScreenAsync},
    },
    shapes::{enums::ScreenActionEnum, keybinding::KeyBindings, settings::Settings},
};
use async_trait::async_trait;
use crokey::{
    crossterm::event::{KeyCode, KeyEvent},
    Combiner,
};
use once_cell::sync::Lazy;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    widgets::{Block, Borders},
    Frame,
};
use regex::Regex;
use serde_json::json;

static EMAIL_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)^[a-z0-9._%+-]+@[a-z0-9.-]*$").unwrap());

fn email_validator(current: &str, c: char) -> bool {
    if !c.is_ascii_alphanumeric() && !"@._%+-".contains(c) {
        return false;
    }
    let mut s = current.to_string();
    s.push(c);
    true
}

#[derive(Debug)]
pub struct ReportAnIssueScreen {
    title: TextBox,
    description: TextBox,
    name: TextBox,
    email: TextBox,
    field: usize,
    notify_message: NotifyBanner,
    back: bool,
    footer: NavigationFooter,
    footer_entries: Vec<(String, String)>,
    combiner: Combiner,
    screen_key_bindings: KeyBindings,
}

impl Renderable for ReportAnIssueScreen {
    fn render(&mut self, f: &mut Frame, body: Rect, footer_left: Rect, footer_right: Rect) {
        let area = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([
                Constraint::Length(3), // 0: title
                Constraint::Length(6), // 1: description
                Constraint::Length(3), // 2: name
                Constraint::Length(3), // 3: email
                Constraint::Min(1),
            ])
            .split(body);
        self.notify_message.render(f, footer_right);
        self.render_header(f, body);
        self.title.render(f, area[0]);
        self.description.render(f, area[1]);
        self.name.render(f, area[2]);
        self.email.render(f, area[3]);
        self.footer
            .render(f, footer_left, self.footer_entries.clone());
    }
}

#[async_trait]
impl ScreenAsync for ReportAnIssueScreen {
    async fn handle_key(&mut self, key: KeyEvent) -> AppAction {
        if let Some(key_combination) = self.combiner.transform(key) {
            match (
                self.screen_key_bindings.get(key_combination),
                key.code,
                &self.notify_message.has_value(),
            ) {
                (_, _, true) => self.handle_error_reset(),
                (None, KeyCode::Backspace, _) => self.handle_backspace(),
                (None, KeyCode::Char(c), _) => self.handle_char(c),
                (Some(&ScreenActionEnum::Previous), _, _) => self.handle_backtab(),
                (Some(&ScreenActionEnum::Next), _, _) => self.handle_tab(),
                (Some(&ScreenActionEnum::Back), _, _) => AppAction::Back(true, Some(1)),
                (Some(&ScreenActionEnum::Confirm), _, _) => {
                    if self.field == 1 {
                        self.description.handle_char('\n');
                        AppAction::None
                    } else {
                        self.handle_enter().await
                    }
                }
                (Some(&ScreenActionEnum::Quit), _, _) => AppAction::Quit(Ok(())),
                _ => AppAction::None,
            }
        } else {
            AppAction::None
        }
    }

    async fn refresh_data(&mut self) {}
}

impl ReportAnIssueScreen {
    pub fn new(settings: Settings) -> Self {
        let title = TextBox::new(current_labels().title.to_owned(), true, None);
        let description = TextBox::new(current_labels().description.to_owned(), false, None)
            .enable_multiline(true);
        let name = TextBox::new(current_labels().name.to_owned(), false, None);
        let email = TextBox::with_validator(
            current_labels().email.to_owned(),
            false,
            None,
            email_validator,
        );
        let screen_actions = vec![
            &ScreenActionEnum::Next,
            &ScreenActionEnum::Previous,
            &ScreenActionEnum::Confirm,
            &ScreenActionEnum::Back,
            &ScreenActionEnum::Quit,
        ];
        let kb = &settings.keybindings;
        let footer_entries = get_keybinding_actions(kb, Sba::ScreenActions(&screen_actions));
        let screen_key_bindings = kb.slice(screen_actions);
        ReportAnIssueScreen {
            title,
            description,
            name,
            email,
            field: 0,
            notify_message: NotifyBanner::new(),
            back: false,
            footer: NavigationFooter::new(),
            footer_entries,
            combiner: Combiner::default(),
            screen_key_bindings,
        }
    }

    fn handle_error_reset(&mut self) -> AppAction {
        self.notify_message.reset();
        if self.back {
            AppAction::Back(true, Some(1))
        } else {
            AppAction::None
        }
    }

    fn handle_tab(&mut self) -> AppAction {
        self.field = (self.field + 1) % 4;
        self.update_writing_modes();
        AppAction::None
    }

    fn handle_backtab(&mut self) -> AppAction {
        self.field = (self.field + 3) % 4;
        self.update_writing_modes();
        AppAction::None
    }

    fn update_writing_modes(&mut self) {
        self.title.writing_mode = self.field == 0;
        self.description.writing_mode = self.field == 1;
        self.name.writing_mode = self.field == 2;
        self.email.writing_mode = self.field == 3;
    }

    fn handle_backspace(&mut self) -> AppAction {
        self.title.handle_backspace();
        self.description.handle_backspace();
        self.name.handle_backspace();
        self.email.handle_backspace();
        AppAction::None
    }

    async fn handle_enter(&mut self) -> AppAction {
        match (
            self.field,
            self.title.get_selected_value(),
            self.description.get_selected_value(),
            self.name.get_selected_value(),
            self.email.get_selected_value(),
        ) {
            (0, _, _, _, _) => AppAction::None,
            (_, None, _, _, _) => {
                self.notify_message
                    .set_error(current_labels().name_cannot_be_empty.to_string());
                AppAction::None
            }
            (_, _, None, _, _) => {
                self.notify_message
                    .set_error(current_labels().description_cannot_be_empty.to_string());
                AppAction::None
            }
            (_, Some(title), Some(description), _, _) => {
                let mut full_description = description.to_string();
                if let Some(name) = self.name.get_selected_value() {
                    if !name.is_empty() {
                        full_description.push_str(&format!("\n\nname: {}", name));
                    }
                }
                if let Some(email) = self.email.get_selected_value() {
                    if !email.is_empty() {
                        full_description.push_str(&format!("\ne-mail: {}", email));
                    } else if !EMAIL_RE.is_match(&email) {
                        self.notify_message
                            .set_error(current_labels().invalid_email_address.to_string());
                        return AppAction::None;
                    }
                }
                let result = self
                    .send_issue(title.as_str(), full_description.as_str())
                    .await;
                match result {
                    Ok(_) => {
                        self.notify_message
                            .set_info(current_labels().issue_reported_successfully.to_string());
                        self.back = true;
                    }
                    Err(_) => {
                        self.notify_message
                            .set_error(current_labels().failed_to_report_issue.to_string());
                    }
                }
                AppAction::None
            }
        }
    }

    fn handle_char(&mut self, c: char) -> AppAction {
        self.title.handle_char(c);
        self.description.handle_char(c);
        self.name.handle_char(c);
        self.email.handle_char(c);
        AppAction::None
    }

    fn render_header(&self, f: &mut Frame, area: Rect) {
        let block = Block::default()
            .borders(Borders::ALL)
            .title(current_labels().report_an_issue);
        f.render_widget(block, area);
    }

    async fn send_issue(&self, title: &str, description: &str) -> Result<(), String> {
        const LINEAR_API_URL: &str = "https://api.linear.app/graphql";
        let token = var("LINEAR_TOKEN").unwrap_or_default();
        let team_id = var("LINEAR_TEAM_ID").unwrap_or_default();
        let client = reqwest::Client::new();
        let query = r#"
            mutation CreateIssue($input: IssueCreateInput!) {
                issueCreate(input: $input) {
                    success
                    issue {
                        id
                        identifier
                        title
                        description
                    }
                }
            }
        "#;
        let body = json!({
            "query": query,
            "variables": {
                "input": {
                    "title": title,
                    "description": description,
                    "teamId": team_id
                }
            }
        });
        let response = client
            .post(LINEAR_API_URL)
            .header("Authorization", token)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Network error: {}", e))?;
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(format!("http error {}: {}", status, text));
        }
        let json: serde_json::Value =
            serde_json::from_str(&text).map_err(|e| format!("invalid json response: {}", e))?;
        if json["data"]["issueCreate"]["success"] == true {
            Ok(())
        } else {
            Err(format!("failed to create issue: {}", json))
        }
    }
}
