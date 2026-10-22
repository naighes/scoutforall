use crate::shapes::keybinding::navigation_keybindings;

use crate::localization::current_labels;
use crate::{
    screens::{
        components::{
            navigation_footer::NavigationFooter, notify_banner::NotifyBanner, panel,
            text_box::TextBox,
        },
        screen::{
            AppAction, FieldKey, Renderable, Sba, ScreenAsync, get_keybinding_actions,
            navigation_entries,
        },
    },
    shapes::{actions::ScreenActionEnum, keybinding::ActionsKeyBindings},
};
use async_trait::async_trait;
use crokey::{
    Combiner,
    crossterm::event::{KeyCode, KeyEvent},
};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
};
use scout4all_core::util::config_var;
use serde_json::json;

/// An e-mail address: a name of letters, digits and `._%+-`, then `@` and
/// a domain of letters, digits, dots and hyphens, with a dot in it.
fn is_valid_email(email: &str) -> bool {
    let name_char = |c: char| c.is_ascii_alphanumeric() || "._%+-".contains(c);
    let domain_char = |c: char| c.is_ascii_alphanumeric() || ".-".contains(c);
    match email.split_once('@') {
        Some((name, domain)) => {
            !name.is_empty()
                && name.chars().all(name_char)
                && domain.chars().all(domain_char)
                && domain
                    .split_once('.')
                    .is_some_and(|(host, top)| !host.is_empty() && !top.is_empty())
        }
        None => false,
    }
}

/// The title and the text of the issue: the description, then the name and
/// the e-mail address when given. The error is the label to show: the title
/// or the description missing, or the e-mail address not valid.
fn issue(
    title: &str,
    description: &str,
    name: &str,
    email: &str,
) -> Result<(String, String), &'static str> {
    let labels = current_labels();
    let [title, description, name, email] = [title, description, name, email].map(str::trim);
    match (
        title.is_empty(),
        description.is_empty(),
        email.is_empty() || is_valid_email(email),
    ) {
        (true, _, _) => Err(labels.title_cannot_be_empty),
        (_, true, _) => Err(labels.description_cannot_be_empty),
        (_, _, false) => Err(labels.invalid_email_address),
        (false, false, true) => {
            let contacts: Vec<String> = [("name", name), ("e-mail", email)]
                .iter()
                .filter(|(_, value)| !value.is_empty())
                .map(|(key, value)| format!("{key}: {value}"))
                .collect();
            let text = match contacts.is_empty() {
                true => description.to_string(),
                false => format!("{description}\n\n{}", contacts.join("\n")),
            };
            Ok((title.to_string(), text))
        }
    }
}

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
    screen_key_bindings: ActionsKeyBindings<ScreenActionEnum>,
}

impl Renderable for ReportAnIssueScreen {
    fn footer_entries(&self) -> Vec<(String, String)> {
        self.footer_entries.clone()
    }

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
                (_, code, _) if FieldKey::of(code) == Some(FieldKey::Previous) => {
                    self.handle_backtab()
                }
                (_, code, _) if FieldKey::of(code) == Some(FieldKey::Next) => self.handle_tab(),
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
    pub fn new() -> Self {
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
        let screen_actions = &[
            Sba::Simple(ScreenActionEnum::Confirm),
            Sba::Simple(ScreenActionEnum::Back),
            Sba::Simple(ScreenActionEnum::Quit),
        ];
        let kb = navigation_keybindings();
        let footer_entries = navigation_entries()
            .into_iter()
            .chain(get_keybinding_actions(kb, screen_actions))
            .collect();
        let screen_key_bindings = kb.slice(Sba::keys(screen_actions));
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
        let text = |field: &TextBox| field.get_selected_value().unwrap_or_default();
        let issue = issue(
            &text(&self.title),
            &text(&self.description),
            &text(&self.name),
            &text(&self.email),
        );
        match (self.field, issue) {
            (0, _) => AppAction::None,
            (_, Err(label)) => {
                self.notify_message.set_error(label.to_string());
                AppAction::None
            }
            (_, Ok((title, text))) => {
                match self.send_issue(&title, &text).await {
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
        let block = panel::screen(current_labels().report_an_issue);
        f.render_widget(block, area);
    }

    async fn send_issue(&self, title: &str, description: &str) -> Result<(), String> {
        const LINEAR_API_URL: &str = "https://api.linear.app/graphql";
        let token = config_var("LINEAR_TOKEN", option_env!("LINEAR_TOKEN")).unwrap_or_default();
        let team_id =
            config_var("LINEAR_TEAM_ID", option_env!("LINEAR_TEAM_ID")).unwrap_or_default();
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_email_addresses() {
        for email in [
            "anna@club.it",
            "Anna.Rossi+volley@mail.club-a.com",
            "a_b%c@x.io",
        ] {
            assert!(is_valid_email(email), "{email}");
        }
        for email in [
            "",
            "anna",
            "@club.it",
            "anna@",
            "anna@club",
            "anna@.it",
            "anna@club.",
            "an na@club.it",
            "anna@club@it",
            "annà@club.it",
        ] {
            assert!(!is_valid_email(email), "{email}");
        }
    }

    /// The e-mail address is optional: an issue without it is sent; one
    /// not valid isn't.
    #[test]
    fn the_email_address_is_optional_but_valid() {
        let labels = current_labels();
        assert_eq!(
            issue("crash", "on start", "", ""),
            Ok(("crash".to_string(), "on start".to_string()))
        );
        assert_eq!(
            issue("crash", "on start", "", "anna"),
            Err(labels.invalid_email_address)
        );
        assert_eq!(
            issue(" crash ", "on start", "Anna", "anna@club.it"),
            Ok((
                "crash".to_string(),
                "on start\n\nname: Anna\ne-mail: anna@club.it".to_string()
            ))
        );
        assert_eq!(
            issue("crash", "on start", "", "anna@club.it").map(|(_, text)| text),
            Ok("on start\n\ne-mail: anna@club.it".to_string())
        );
    }

    /// The title and the description are required, blanks aren't enough.
    #[test]
    fn the_title_and_the_description_are_required() {
        let labels = current_labels();
        assert_eq!(
            issue("", "on start", "", ""),
            Err(labels.title_cannot_be_empty)
        );
        assert_eq!(
            issue("  ", "on start", "", ""),
            Err(labels.title_cannot_be_empty)
        );
        assert_eq!(
            issue("crash", " ", "", ""),
            Err(labels.description_cannot_be_empty)
        );
    }
}
