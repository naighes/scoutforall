//! The settings of the app, beside its keys (F2): the language, the
//! country a new team starts with, the statistics sent; the terms of use
//! and what the app is, with how to report a problem. Each change is saved
//! as it's made: the language in `config.json` as the TUI keeps it, the
//! rest as the app saves it (`mobile`).

use {
    crate::{
        countries,
        form::{self, Field, Form},
        strings::Strings,
        theme::{Colors, SIZE_BODY, SIZE_SMALL},
        ui,
    },
    iced::{
        Element, Length,
        widget::{Column, Id, column, container, row, scrollable, space, text},
    },
    scout4all_core::shapes::enums::LanguageEnum,
    scout4all_keys::{KeyCombination, key},
    scout4all_mobile::{Scout4All, Terms},
    std::{path::Path, sync::Arc},
};

const REPOSITORY: &str = "https://github.com/naighes/scoutforall";
/// the privacy policy, the text of DATA-PRIVACY
const PRIVACY: &str = "https://naighes.github.io/scoutforall/privacy/";
/// the e-mail address that opens an issue on Linear, as the app's
const REPORTS: &str = "scout4all-0fd59357fbe3@intake.linear.app";

// the fields
const LANGUAGE: usize = 0;
const COUNTRY: usize = 1;
const STATISTICS: usize = 2;

#[derive(Debug, Clone, Copy)]
pub enum Link {
    License,
    Privacy,
    Source,
    Report,
}

#[derive(Debug, Clone)]
pub enum Message {
    Form(form::Message),
    Open(Link),
    Terms,
}

/// The languages, each in its own words, after the flag of where it's
/// spoken (Brazilian Portuguese).
fn language_name(language: LanguageEnum) -> &'static str {
    match language {
        LanguageEnum::En => "🇬🇧  English",
        LanguageEnum::It => "🇮🇹  Italiano",
        LanguageEnum::Pt => "🇧🇷  Português",
        LanguageEnum::Pl => "🇵🇱  Polski",
        LanguageEnum::Ja => "🇯🇵  日本語",
    }
}

/// What helps to reproduce a problem, at the end of a report.
fn device() -> String {
    format!(
        "scout4all {} · {} {}",
        scout4all_mobile::version(),
        std::env::consts::OS,
        std::env::consts::ARCH
    )
}

/// The text as a URL takes it: the letters and digits as they are, the
/// rest as bytes.
fn encoded(text: &str) -> String {
    text.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                char::from(b).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// An e-mail ready to send, that opens an issue on Linear.
pub fn report_url(s: &Strings) -> String {
    let subject = format!(
        "scout4all {}: {}",
        scout4all_mobile::version(),
        s.report_title
    );
    let body = ui::fill(s.report_body, &[device()]);
    format!(
        "mailto:{REPORTS}?subject={}&body={}",
        encoded(&subject),
        encoded(&body)
    )
}

/// Writes the language in `config.json`, as the TUI keeps it.
fn save_language(dir: &Path, language: LanguageEnum) -> Result<(), String> {
    // the code, as serde writes it and the TUI reads it
    crate::settings::merge(dir, serde_json::json!({ "language": language }))
}

pub struct General {
    core: Arc<Scout4All>,
    pub language: LanguageEnum,
    pub form: Form,
    /// the terms of use, being read
    pub terms: Option<Terms>,
    pub error: Option<String>,
}

impl General {
    pub fn new(core: Arc<Scout4All>, language: LanguageEnum, s: &Strings) -> General {
        let mut general = General {
            language,
            form: Form::new(s.general, vec![]),
            terms: None,
            error: None,
            core,
        };
        general.build(s);
        general
    }

    /// The form, in the words of the language, with the country chosen.
    fn build(&mut self, s: &Strings) {
        let country = self
            .core
            .default_country()
            .map(|c| c.name)
            .unwrap_or_default();
        let focused = self.form.focused;
        let mut form = Form::new(
            s.general,
            vec![
                Field::choice(
                    s.language,
                    LanguageEnum::ALL
                        .iter()
                        .map(|l| language_name(*l).to_string())
                        .collect(),
                    LanguageEnum::ALL
                        .iter()
                        .position(|l| *l == self.language)
                        .unwrap_or(0),
                ),
                Field::search(s.default_country, country).hint(s.search_country),
                Field::check(s.send_statistics, self.core.analytics_enabled(), false)
                    .hint(s.send_statistics_detail),
            ],
        );
        form.plain = true;
        form.focused = focused;
        self.form = form;
    }

    pub fn focus_id(&self) -> Option<Id> {
        match self.terms {
            Some(_) => None,
            None => Some(self.form.focus_id()),
        }
    }

    /// A field changed: saved at once.
    fn changed(&mut self, dir: &Path, s: &Strings, field: usize) {
        let saved = match field {
            LANGUAGE => {
                let language = LanguageEnum::ALL
                    .get(self.form.index(LANGUAGE))
                    .copied()
                    .unwrap_or(LanguageEnum::En);
                save_language(dir, language).map(|()| {
                    self.language = language;
                    self.core.set_language(language.to_string());
                })
            }
            COUNTRY => {
                let written = self.form.text(COUNTRY);
                self.form
                    .suggest(COUNTRY, countries::suggestions(&self.core, &written));
                // saved once it's a country: while written, it's not yet
                match countries::chosen(&self.core, &written) {
                    Ok(Some(country)) => self
                        .core
                        .set_default_country(country)
                        .map_err(|e| e.to_string()),
                    Ok(None) | Err(()) => Ok(()),
                }
            }
            STATISTICS => {
                let on = matches!(
                    self.form.fields.get(STATISTICS).map(|f| &f.value),
                    Some(form::Value::Check { on: true, .. })
                );
                self.core
                    .set_analytics_enabled(on)
                    .map(|()| {
                        if on {
                            send_statistics(self.core.clone());
                        }
                    })
                    .map_err(|e| e.to_string())
            }
            _ => Ok(()),
        };
        self.error = saved.err().map(|e| s.not_saved.replace("{}", &e));
    }

    fn open(&mut self, s: &Strings, link: Link) {
        let url = match link {
            Link::License => format!("{REPOSITORY}/blob/master/LICENSE"),
            Link::Privacy => PRIVACY.to_string(),
            Link::Source => REPOSITORY.to_string(),
            Link::Report => report_url(s),
        };
        self.error = open::that_detached(url)
            .err()
            .map(|_| s.no_app_to_open.to_string());
    }

    /// After a key or a click: false to leave the settings.
    fn outcome(&mut self, dir: &Path, s: &Strings, outcome: form::Outcome) -> bool {
        match outcome {
            form::Outcome::Changed(field) => {
                self.changed(dir, s, field);
                true
            }
            form::Outcome::Cancel => false,
            form::Outcome::Stay | form::Outcome::Save => true,
        }
    }

    /// The language changed: the form in its words, the country too.
    pub fn relabel(&mut self, s: &Strings) {
        self.build(s);
    }

    pub fn key(&mut self, dir: &Path, s: &Strings, ck: KeyCombination) -> bool {
        match (&self.terms, ck) {
            (Some(_), k) if k == scout4all_keys::BACK_KEY => {
                self.terms = None;
                true
            }
            (Some(_), _) => true,
            (None, k) if k == key!(t) => {
                self.terms = Some(self.core.terms());
                true
            }
            (None, k) if k == key!(r) => {
                self.open(s, Link::Report);
                true
            }
            (None, ck) => {
                let outcome = self.form.key(ck);
                self.outcome(dir, s, outcome)
            }
        }
    }

    pub fn update(&mut self, dir: &Path, s: &Strings, message: Message) -> bool {
        match message {
            Message::Form(m) => {
                let outcome = self.form.update(m);
                self.outcome(dir, s, outcome)
            }
            Message::Open(link) => {
                self.open(s, link);
                true
            }
            Message::Terms => {
                self.terms = Some(self.core.terms());
                true
            }
        }
    }

    pub fn hints(&self, s: &Strings) -> Vec<(String, String)> {
        match self.terms {
            Some(_) => vec![("Esc".to_string(), s.back.to_string())],
            None => vec![
                ("⇧⇥ ⇥".to_string(), s.page.to_string()),
                ("↑ ↓".to_string(), String::new()),
                (format!("← → {}", s.space), s.change.to_string()),
                ("t".to_string(), s.terms_of_use.to_string()),
                ("r".to_string(), s.report_problem.to_string()),
                ("Esc".to_string(), s.back.to_string()),
            ],
        }
    }

    pub fn view<'a>(&'a self, s: &'static Strings, c: Colors) -> Element<'a, Message> {
        if let Some(terms) = &self.terms {
            return terms_view(terms, None, c);
        }
        let error: Element<'a, Message> = match &self.error {
            Some(e) => ui::body(e.clone()).color(c.danger).into(),
            None => space::vertical().height(0).into(),
        };
        column![
            error,
            self.form.view(s, c, Message::Form, Some(about(s, c)))
        ]
        .spacing(10)
        .into()
    }
}

/// What the app is: its version, license, data and code; the terms and how
/// to report a problem.
fn about<'a>(s: &'static Strings, c: Colors) -> Element<'a, Message> {
    let line = |label: &str, value: Element<'a, Message>| {
        column![
            text(label.to_string()).size(SIZE_SMALL).color(c.muted),
            value
        ]
        .spacing(4)
    };
    ui::panel(
        Some(s.about),
        column![
            line(s.version, ui::body(scout4all_mobile::version()).into()),
            line(
                s.license,
                ui::pill(s.license_name, false, Some(Message::Open(Link::License)), c)
            ),
            line(
                s.data_privacy,
                ui::pill(PRIVACY, false, Some(Message::Open(Link::Privacy)), c)
            ),
            line(
                s.source_code,
                ui::pill(
                    REPOSITORY.trim_start_matches("https://"),
                    false,
                    Some(Message::Open(Link::Source)),
                    c
                )
            ),
            line(
                s.terms_of_use,
                ui::pill(
                    &format!("t  {}", s.terms_of_use),
                    false,
                    Some(Message::Terms),
                    c
                )
            ),
            line(
                s.report_problem_detail,
                ui::pill(
                    &format!("r  {}", s.report_problem),
                    true,
                    Some(Message::Open(Link::Report)),
                    c
                )
            ),
        ]
        .spacing(14)
        .width(Length::Fill),
        c,
    )
}

/// The terms of use, a paragraph each; `below` after them (the choice
/// about the statistics and the button, at the first start).
pub fn terms_view<'a, M: 'a>(
    terms: &Terms,
    below: Option<Element<'a, M>>,
    c: Colors,
) -> Element<'a, M> {
    let paragraphs = terms
        .paragraphs
        .iter()
        .map(|p| text(p.clone()).size(SIZE_BODY).into());
    let body = column![
        Column::with_children(paragraphs).spacing(12),
        below.unwrap_or_else(|| space::vertical().height(0).into()),
    ]
    .spacing(20)
    .width(Length::Fill);
    let panel = ui::panel(
        Some(&ui::capitalized(&terms.title)),
        scrollable(body).height(Length::Fill),
        c,
    );
    row![
        space::horizontal().width(Length::FillPortion(1)),
        container(panel)
            .width(Length::FillPortion(4))
            .height(Length::Fill),
        space::horizontal().width(Length::FillPortion(1)),
    ]
    .height(Length::Fill)
    .into()
}

/// Sends the matches queued, on a thread of its own: it blocks while the
/// network answers. Nothing to do when the statistics are off.
pub fn send_statistics(core: Arc<Scout4All>) {
    std::thread::spawn(move || {
        let _ = core.upload_pending();
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    // the settings are what reads the keys

    #[test]
    fn each_change_is_saved_as_it_is_made() {
        let s = crate::strings::strings(LanguageEnum::It);
        let dir = tempfile::tempdir().unwrap();
        let core = Scout4All::new(dir.path().to_string_lossy().into_owned(), "it".into()).unwrap();
        core.accept_terms().unwrap();
        let mut settings = crate::settings::SettingsScreen::new(
            core.clone(),
            dir.path().to_path_buf(),
            scout4all_keys::ScoutingKeys::default().normalized(),
            LanguageEnum::It,
            s,
        );
        // the language: in config.json, as the TUI reads it
        settings.key(key!(right), s);
        assert_eq!(settings.language(), LanguageEnum::Pt);
        let config = || -> serde_json::Value {
            serde_json::from_str(&std::fs::read_to_string(dir.path().join("config.json")).unwrap())
                .unwrap()
        };
        assert_eq!(config()["language"], "pt");
        // the country of new teams, searched (by its code: "Itália" in
        // Portuguese)
        settings.key(key!(down), s);
        settings.update(
            crate::settings::Message::General(Message::Form(form::Message::Input(
                COUNTRY,
                "IT".into(),
            ))),
            s,
        );
        assert_eq!(
            core.default_country().map(|c| c.value.to_string()),
            Some("IT".to_string())
        );
        // the countries offered close with Esc, the settings stay
        assert!(settings.general.form.lit().is_some());
        assert!(matches!(
            settings.key(scout4all_keys::BACK_KEY, s),
            crate::settings::Outcome::Stay
        ));
        // the statistics: off, then on again
        assert!(core.analytics_enabled());
        settings.key(key!(down), s);
        settings.key(key!(space), s);
        assert!(!core.analytics_enabled());
        settings.key(key!(space), s);
        assert!(core.analytics_enabled());
        // the terms, read and left with Esc; Esc again leaves the settings
        settings.key(key!(t), s);
        assert!(settings.general.terms.is_some());
        assert!(matches!(
            settings.key(scout4all_keys::BACK_KEY, s),
            crate::settings::Outcome::Stay
        ));
        assert!(settings.general.terms.is_none());
        // Tab: the keys, and back
        settings.key(key!(tab), s);
        assert!(settings.focus_id().is_none());
        settings.key(key!(tab), s);
        assert!(settings.focus_id().is_some());
        assert!(matches!(
            settings.key(scout4all_keys::BACK_KEY, s),
            crate::settings::Outcome::Close
        ));
    }

    #[test]
    fn the_report_is_an_e_mail_ready_to_send() {
        let s = crate::strings::strings(LanguageEnum::It);
        let url = report_url(s);
        assert!(url.starts_with(&format!("mailto:{REPORTS}?subject=scout4all%20")));
        assert!(url.contains("Problema%20nell%27app%20desktop"));
        assert!(!url.contains(' '));
        assert!(!url.contains('\n'));
        assert_eq!(encoded("a b/è"), "a%20b%2F%C3%A8");
    }
}
