use crate::{
    constants::TEAM_DESCRIPTOR_FILE_NAME,
    errors::{AppError, IOError, MatchError},
    localization::Language,
    logging::logger::log_error,
    providers::{
        match_reader::MatchReader,
        set_reader::SetReader,
        store::match_index::{Indexed, MatchIndex, TeamIndex, fingerprint},
    },
    shapes::{
        enums::LineupRulesEnum,
        listed::{Listed, Unreadable},
        r#match::{MatchEntry, MatchStatus, MatchSummary},
        set::SetEntry,
        team::TeamEntry,
    },
    storage::{Store, StoreEntry, layout},
};
use async_trait::async_trait;
use futures::future::join_all;
use serde_json::{Value, from_slice, from_value};
use std::{cmp::Ordering, sync::Arc};

pub struct StoreMatchReader {
    store: Arc<dyn Store>,
    set_reader: Arc<dyn SetReader + Send + Sync>,
    language: Language,
    index: MatchIndex,
}

#[async_trait]
impl MatchReader for StoreMatchReader {
    async fn read_all(&self, team: &TeamEntry) -> Result<Vec<Listed<MatchEntry>>, AppError> {
        Ok(self
            .read_all_with_status(team)
            .await?
            .into_iter()
            .map(|listed| listed.map(|(entry, _)| entry))
            .collect())
    }

    async fn read_all_with_status(
        &self,
        team: &TeamEntry,
    ) -> Result<Vec<Listed<(MatchEntry, MatchStatus)>>, AppError> {
        let folders = self.match_folders(team).await?;
        let mut entries: Vec<Listed<(MatchEntry, MatchStatus)>> =
            join_all(folders.iter().map(|match_id| self.listed(match_id, team))).await;
        // the most recent first, then the unreadable ones by name
        entries.sort_by(|a, b| match (a, b) {
            (Ok((a, _)), Ok((b, _))) => b.date.cmp(&a.date),
            (Ok(_), Err(_)) => Ordering::Less,
            (Err(_), Ok(_)) => Ordering::Greater,
            (Err(a), Err(b)) => a.id.cmp(&b.id),
        });
        Ok(entries)
    }

    async fn read_summaries(
        &self,
        team: &TeamEntry,
    ) -> Result<Vec<Listed<MatchSummary>>, AppError> {
        let folder = layout::team_folder(&team.id);
        let team_stamp = self
            .store
            .stamps(&folder)
            .await?
            .into_iter()
            .find(|(name, _)| name == TEAM_DESCRIPTOR_FILE_NAME)
            .and_then(|(_, stamp)| stamp);
        let indexed = self.index.team(&*self.store, &team.id).await;
        // before the files are read: see `match_index`
        let folders = self.store.folder_stamps(&folder).await?;
        let read: Vec<(Listed<MatchSummary>, Option<Indexed>)> =
            join_all(folders.iter().map(|(match_id, files)| {
                let indexed = &indexed;
                async move {
                    let stamp = fingerprint(team_stamp, files);
                    match (stamp, indexed.get(match_id)) {
                        (Some(stamp), Some(hit)) if hit.stamp == stamp => {
                            (Ok(hit.summary.clone()), Some(hit.clone()))
                        }
                        _ => match self.listed(match_id, team).await {
                            Ok((m, status)) => {
                                let summary = MatchSummary::of(&m, &status);
                                let entry = stamp.map(|stamp| Indexed {
                                    stamp,
                                    summary: summary.clone(),
                                });
                                (Ok(summary), entry)
                            }
                            Err(unreadable) => (Err(unreadable), None),
                        },
                    }
                }
            }))
            .await;
        let now: TeamIndex = read
            .iter()
            .filter_map(|(_, entry)| entry.clone())
            .map(|entry| (entry.summary.id.clone(), entry))
            .collect();
        self.index.update(&*self.store, &team.id, now).await;
        let mut summaries: Vec<Listed<MatchSummary>> =
            read.into_iter().map(|(listed, _)| listed).collect();
        summaries.sort_by(|a, b| match (a, b) {
            (Ok(a), Ok(b)) => b.date.cmp(&a.date),
            (Ok(_), Err(_)) => Ordering::Less,
            (Err(_), Ok(_)) => Ordering::Greater,
            (Err(a), Err(b)) => a.id.cmp(&b.id),
        });
        Ok(summaries)
    }

    async fn read_single(&self, team: &TeamEntry, match_id: &str) -> Result<MatchEntry, AppError> {
        self.map_entry(match_id, team).await.map(|(entry, _)| entry)
    }

    async fn exists(&self, team: &TeamEntry, match_id: &str) -> Result<bool, AppError> {
        self.store
            .exists(&layout::match_folder(&team.id, match_id))
            .await
    }
}

impl StoreMatchReader {
    pub fn new(
        store: Arc<dyn Store>,
        set_reader: Arc<dyn SetReader + Send + Sync>,
        language: Language,
    ) -> Self {
        Self {
            store,
            set_reader,
            language,
            index: MatchIndex::default(),
        }
    }

    /// The folders of the matches of the team.
    async fn match_folders(&self, team: &TeamEntry) -> Result<Vec<String>, AppError> {
        Ok(self
            .store
            .list(&layout::team_folder(&team.id))
            .await?
            .into_iter()
            .filter_map(|entry| match entry {
                StoreEntry::Folder(name) => Some(name),
                StoreEntry::Blob(_) => None,
            })
            .collect())
    }

    /// The match of the folder, or why it can't be read (logged).
    async fn listed(&self, match_id: &str, team: &TeamEntry) -> Listed<(MatchEntry, MatchStatus)> {
        self.map_entry(match_id, team).await.map_err(|e| {
            log_error(&format!(
                "match {:?} of team {} is unreadable: {}",
                match_id, team.id, e
            ));
            Unreadable {
                id: match_id.to_string(),
                reason: match e {
                    AppError::Match(MatchError::SetNotValid(reason)) => Some(reason),
                    _ => None,
                },
            }
        })
    }

    /// The match, with the status its sets replay to.
    async fn map_entry(
        &self,
        match_id: &str,
        team: &TeamEntry,
    ) -> Result<(MatchEntry, MatchStatus), AppError> {
        let content = self
            .store
            .read(&layout::match_descriptor(&team.id, match_id))
            .await?
            .ok_or_else(|| {
                AppError::IO(IOError::Msg(
                    self.language
                        .labels()
                        .match_descriptor_file_not_found
                        .to_string(),
                ))
            })?;
        let (mut entry, rules) = read_descriptor(&content)?;
        entry.id = match_id.to_string();
        entry.team = team.with_roster(&entry.roster);
        entry.sets = self.set_reader.read_all(&entry).await?;
        fill_lineup_rules(&mut entry.sets, rules);
        // a match whose sets don't replay can't be shown nor scouted: the
        // first event that doesn't tells why
        let status = entry.get_status().map_err(|e| match entry.replay_issue() {
            Some(issue) => AppError::Match(MatchError::SetNotValid(
                issue.describe(self.language.labels()),
            )),
            None => e,
        })?;
        Ok((entry, status))
    }
}

/// The match of a descriptor, as saved in its folder or in an archive,
/// without its sets; and the lineup rules of its sets saved without them:
/// only matches created since v0.3.0 save the substitutions, and v0.3.0
/// didn't save the rules of its sets, but scouted them with V2.
pub(crate) fn read_descriptor(
    content: &[u8],
) -> Result<(MatchEntry, Option<LineupRulesEnum>), AppError> {
    let descriptor: Value = from_slice(content).map_err(|e| AppError::IO(IOError::from(e)))?;
    let rules = descriptor
        .get("max_substitutions")
        .map(|_| LineupRulesEnum::V2);
    let entry: MatchEntry = from_value(descriptor).map_err(|e| AppError::IO(IOError::from(e)))?;
    Ok((entry, rules))
}

/// The sets saved without their rules get `rules`, from `read_descriptor`.
pub(crate) fn fill_lineup_rules(sets: &mut [SetEntry], rules: Option<LineupRulesEnum>) {
    for set in sets.iter_mut() {
        set.lineup_rules = set.lineup_rules.or(rules);
    }
}
