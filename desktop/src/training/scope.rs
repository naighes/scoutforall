//! The question over a session of a series, when it's changed or deleted:
//! "only this session, this and the following, the whole series", as the
//! calendars of the mail ask it. ↑ ↓ (or 1 2 3) choose, Enter answers, Esc
//! goes back.

use {
    crate::{
        strings::Strings,
        theme::{self, Colors},
        ui,
    },
    iced::{
        Alignment, Element, Length,
        widget::{Column, column, container, text},
    },
    scout4all_core::training::series::SeriesScope,
    scout4all_keys::{BACK_KEY, KeyCombination, key},
    scout4all_mobile::{MobileError, Scout4All},
};

pub const SCOPES: [SeriesScope; 3] = [
    SeriesScope::This,
    SeriesScope::ThisAndFollowing,
    SeriesScope::All,
];

/// What a key did to the question.
#[derive(Debug, PartialEq)]
pub enum Answer {
    Stay,
    /// back, nothing done
    Cancel,
    Chosen(SeriesScope),
}

/// The question, the answer chosen on it.
#[derive(Debug, Clone, Default)]
pub struct ScopeQuestion {
    pub chosen: usize,
}

fn label(s: &Strings, scope: SeriesScope) -> &'static str {
    match scope {
        SeriesScope::This => s.scope_this,
        SeriesScope::ThisAndFollowing => s.scope_following,
        SeriesScope::All => s.scope_all,
    }
}

impl ScopeQuestion {
    pub fn key(&mut self, ck: KeyCombination) -> Answer {
        let count = SCOPES.len();
        match ck {
            k if k == BACK_KEY => Answer::Cancel,
            k if k == key!(enter) => SCOPES
                .get(self.chosen)
                .map_or(Answer::Stay, |scope| Answer::Chosen(*scope)),
            k if k == key!(down) || k == key!(tab) => {
                self.chosen = (self.chosen + 1) % count;
                Answer::Stay
            }
            k if k == key!(up) || k == key!(shift - tab) || k == key!(shift - backtab) => {
                self.chosen = self.chosen.checked_sub(1).unwrap_or(count - 1);
                Answer::Stay
            }
            k if k == key!('1') => Answer::Chosen(SeriesScope::This),
            k if k == key!('2') => Answer::Chosen(SeriesScope::ThisAndFollowing),
            k if k == key!('3') => Answer::Chosen(SeriesScope::All),
            _ => Answer::Stay,
        }
    }

    pub fn hints(s: &Strings, verb: &str) -> Vec<(String, String)> {
        vec![
            ("↑ ↓ 1 2 3".to_string(), String::new()),
            ("Enter".to_string(), verb.to_string()),
            ("Esc".to_string(), s.cancel.to_string()),
        ]
    }

    /// The question: `what` says what is asked, `verb` and `icon` are of
    /// the main answer (save, delete); a click on a row answers it.
    #[allow(clippy::too_many_arguments)]
    pub fn view<'a, M: Clone + 'a>(
        &self,
        s: &'static Strings,
        c: Colors,
        what: &'static str,
        (icon, verb): (&'static [u8], &'static str),
        on_pick: impl Fn(SeriesScope) -> M,
        on_cancel: M,
    ) -> Element<'a, M> {
        let rows: Vec<Element<'a, M>> = SCOPES
            .iter()
            .enumerate()
            .map(|(i, scope)| {
                ui::choice(
                    Some((i + 1).to_string()),
                    ui::body(label(s, *scope)),
                    i == self.chosen,
                    Some(on_pick(*scope)),
                    c,
                )
            })
            .collect();
        let chosen = SCOPES
            .get(self.chosen)
            .copied()
            .unwrap_or(SeriesScope::This);
        let question = ui::panel(
            None,
            column![
                text(s.in_series_title)
                    .size(22)
                    .font(theme::fonts().bold)
                    .color(c.text),
                ui::body(what),
                Column::with_children(rows).spacing(4),
                ui::answers(
                    Some((theme::icons::CLOSE, s.cancel, on_cancel)),
                    icon,
                    verb,
                    Some(on_pick(chosen)),
                    c
                )
            ]
            .spacing(18),
            c,
        );
        container(container(question).max_width(560))
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Alignment::Center)
            .align_y(Alignment::Center)
            .into()
    }
}

/// Deletes the session; with a wider scope, the other planned sessions of
/// its series too.
pub fn delete_session(core: &Scout4All, id: &str, scope: SeriesScope) -> Result<(), MobileError> {
    match scope {
        SeriesScope::This => core.remove_session(id.to_string()),
        _ => core.remove_series(id.to_string(), scope).map(|_| ()),
    }
}

/// The question to ask before deleting the session: one when it's in a
/// series with other planned sessions.
pub fn deletion_question(core: &Scout4All, id: &str) -> Option<ScopeQuestion> {
    core.asks_series_scope(id.to_string(), None)
        .unwrap_or(false)
        .then(ScopeQuestion::default)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_keys_choose_and_answer() {
        let mut q = ScopeQuestion::default();
        assert_eq!(q.key(key!(enter)), Answer::Chosen(SeriesScope::This));
        assert_eq!(q.key(key!(down)), Answer::Stay);
        assert_eq!(q.key(key!(down)), Answer::Stay);
        assert_eq!(q.key(key!(enter)), Answer::Chosen(SeriesScope::All));
        assert_eq!(q.key(key!(down)), Answer::Stay);
        assert_eq!(q.chosen, 0);
        assert_eq!(q.key(key!(up)), Answer::Stay);
        assert_eq!(q.chosen, 2);
        assert_eq!(
            q.key(key!('2')),
            Answer::Chosen(SeriesScope::ThisAndFollowing)
        );
        assert_eq!(q.key(BACK_KEY), Answer::Cancel);
    }
}
