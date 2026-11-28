//! The choice of a gym in a form: the pinned first, then the ones the team
//! used (TODO/021 §13).

use {
    crate::{
        form::{self, Field, Form, Outcome, Value},
        strings::Strings,
        theme::Colors,
    },
    crokey::{KeyCombination, key},
    iced::{Element, Length, widget::container},
    scout4all_mobile::Scout4All,
};

/// The key of a new gym, on the field of the choice.
pub const NEW_KEY: KeyCombination = key!(n);

const EMPTY: &str = "–";

/// The name a gym is shown by: "deleted gym" for a removed one (sessions
/// and matches keep it, TODO/021 §13).
pub fn venue_label(s: &Strings, name: &str, deleted: bool) -> String {
    match deleted {
        true => s.deleted_venue.to_string(),
        false => name.to_string(),
    }
}

/// The gyms a form of the team offers, and the one `current` even when it
/// was removed: id and name, in the order of the choice (pinned first, then
/// the ones the team used).
pub fn venue_options(
    s: &Strings,
    core: &Scout4All,
    team_id: &str,
    current: Option<&str>,
) -> Vec<(String, String)> {
    let mut options: Vec<(String, String)> = core
        .venue_choices(team_id.to_string(), String::new())
        .unwrap_or_default()
        .into_iter()
        .map(|v| (v.id, v.name))
        .collect();
    if let Some(kept) = current
        .filter(|id| !options.iter().any(|(o, _)| o == id))
        .and_then(|id| core.venue(id.to_string()).ok())
    {
        let name = venue_label(s, &kept.name, kept.deleted);
        options.push((kept.id, name));
    }
    options
}

/// The choice of a gym: the empty one, then `options`; `current` is the
/// one chosen.
pub fn venue_value(options: &[(String, String)], current: Option<&str>) -> Value {
    Value::Choice {
        options: std::iter::once(EMPTY.to_string())
            .chain(options.iter().map(|(_, name)| name.clone()))
            .collect(),
        index: current
            .and_then(|id| options.iter().position(|(o, _)| o == id))
            .map_or(0, |at| at + 1),
    }
}

/// The id of the gym at the position `index` of a choice made by `venue_value`.
pub fn venue_at(options: &[(String, String)], index: usize) -> Option<String> {
    index
        .checked_sub(1)
        .and_then(|at| options.get(at))
        .map(|(id, _)| id.clone())
}

/// The field of the choice of a gym, a new one made from it.
pub fn venue_field(s: &Strings, options: &[(String, String)], current: Option<&str>) -> Field {
    Field {
        value: venue_value(options, current),
        add: Some(s.new_venue),
        ..Field::choice(s.session_location, vec![], 0)
    }
}

/// The hint of `NEW_KEY`.
pub fn new_hint(s: &Strings) -> (String, String) {
    ("n".to_string(), s.new_venue.to_string())
}

/// A gym made from the form that chooses one, with the name only: a small
/// form over it.
pub struct NewVenue {
    form: Form,
}

/// What became of the small form of a new gym.
pub enum Step {
    Stay,
    Close,
    /// made: its id and name
    Made(String, String),
}

impl NewVenue {
    pub fn new(s: &Strings) -> NewVenue {
        NewVenue {
            form: Form::new(s.new_venue, vec![Field::text(s.venue_name, String::new())]),
        }
    }

    pub fn focus_id(&self) -> iced::widget::Id {
        self.form.focus_id()
    }

    pub fn update(&mut self, core: &Scout4All, message: form::Message) -> Step {
        let outcome = self.form.update(message);
        self.outcome(core, outcome)
    }

    pub fn key(&mut self, core: &Scout4All, ck: KeyCombination) -> Step {
        let outcome = self.form.key(ck);
        self.outcome(core, outcome)
    }

    fn outcome(&mut self, core: &Scout4All, outcome: Outcome) -> Step {
        match outcome {
            Outcome::Cancel => Step::Close,
            Outcome::Save => match core.create_venue_named(self.form.text(0)) {
                Ok(venue) => Step::Made(venue.id, venue.name),
                Err(e) => {
                    self.form.reject(0, e.to_string());
                    Step::Stay
                }
            },
            Outcome::Stay | Outcome::Changed(_) => Step::Stay,
        }
    }

    pub fn view<'a, M: Clone + 'a>(
        &'a self,
        s: &'static Strings,
        c: Colors,
        on: impl Fn(form::Message) -> M + Copy + 'a,
    ) -> Element<'a, M> {
        container(container(self.form.view(s, c, on, None)).max_width(520.0))
            .center_x(Length::Fill)
            .into()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use {
        super::*, crate::strings::strings, scout4all_core::shapes::enums::LanguageEnum,
        scout4all_mobile::venues::VenueDraft,
    };

    /// A removed gym is offered no more, but the one a session still has
    /// stays in its form, told as removed.
    #[test]
    fn a_removed_gym_is_told_as_removed() {
        let s = strings(LanguageEnum::It);
        let dir = tempfile::tempdir().unwrap();
        let core =
            Scout4All::for_tests(dir.path().to_string_lossy().into_owned(), "it".into()).unwrap();
        let team = core.tutorial_team().unwrap();
        let gym = core
            .save_venue(
                None,
                VenueDraft {
                    name: "Terontola".into(),
                    ..VenueDraft::default()
                },
            )
            .unwrap()
            .id;
        assert_eq!(
            venue_options(s, &core, &team, None),
            vec![(gym.clone(), "Terontola".to_string())]
        );
        core.remove_venue(gym.clone()).unwrap();
        assert_eq!(venue_options(s, &core, &team, None), vec![]);
        assert_eq!(
            venue_options(s, &core, &team, Some(&gym)),
            vec![(gym, s.deleted_venue.to_string())]
        );
    }
}
