use crate::{
    errors::{AppError, MatchError},
    providers::{match_writer::MatchWriter, store::write_json},
    shapes::{r#match::MatchEntry, set::SetEntry, team::TeamEntry},
    storage::{Store, layout},
};
use async_trait::async_trait;
use chrono::{DateTime, FixedOffset};
use std::sync::Arc;

pub struct StoreMatchWriter(Arc<dyn Store>);

impl StoreMatchWriter {
    pub fn new(store: Arc<dyn Store>) -> Self {
        Self(store)
    }
}

#[async_trait]
impl MatchWriter for StoreMatchWriter {
    async fn create(
        &self,
        team: &TeamEntry,
        opponent: String,
        date: DateTime<FixedOffset>,
        home: bool,
        max_substitutions: u8,
    ) -> Result<MatchEntry, AppError> {
        let match_id = MatchEntry::id_of(&opponent, &date);
        let m = MatchEntry {
            opponent,
            date,
            id: match_id.clone(),
            team: team.clone(),
            home,
            max_substitutions,
            sets: vec![],
        };
        let key = layout::match_descriptor(&team.id, &match_id);
        if self.0.exists(&key).await? {
            return Err(AppError::Match(MatchError::MatchAlreadyExists(match_id)));
        }
        write_json(&*self.0, &key, &m).await?;
        Ok(m)
    }

    async fn update(
        &self,
        m: &MatchEntry,
        opponent: String,
        date: DateTime<FixedOffset>,
        home: bool,
        max_substitutions: u8,
    ) -> Result<(), AppError> {
        // the reader replays the sets saved without the lineup rules with
        // those of v0.3.0 once the descriptor saves the substitutions: they
        // get the rules they replay with, before the descriptor is written
        for set in m.sets.iter().filter(|s| s.lineup_rules.is_none()) {
            let rules = set.find_lineup_rules(m.max_substitutions)?;
            let with_rules = SetEntry {
                lineup_rules: Some(rules),
                ..set.clone()
            };
            let key = layout::set_descriptor(&m.team.id, &m.id, set.set_number);
            write_json(&*self.0, &key, &with_rules).await?;
        }
        let updated = MatchEntry {
            opponent,
            date,
            home,
            max_substitutions,
            id: m.id.clone(),
            team: m.team.clone(),
            sets: m.sets.clone(),
        };
        write_json(
            &*self.0,
            &layout::match_descriptor(&m.team.id, &m.id),
            &updated,
        )
        .await
    }
}
