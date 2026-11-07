//! The index of the matches: what the lists show of each match (its
//! `MatchSummary`), so the home and the matches of a team don't read and
//! replay every set. It's a cache, the files stay the truth: each entry
//! carries a stamp of the files it comes from (the size and the time each
//! one was modified, of the match folder and of `team.json`), and when they
//! don't match any more the match is read again. Kept in
//! `layout::MATCH_INDEX`, read once, then up to date in memory; a missing
//! or unreadable index, or one of another version, starts empty.
//!
//! Never stale: a file changed without its stamp telling isn't indexed
//! (see `Store::stamps`), a match that can't be read neither (its reason is
//! in the language of the app); the stamp is taken before the files are
//! read, so a change while reading only makes the entry miss next time.

use crate::{
    logging::logger::log_error,
    shapes::r#match::MatchSummary,
    storage::{Stamp, Store, layout},
};
use rustc_hash::FxHasher;
use serde::{Deserialize, Serialize};
use serde_json::{from_slice, to_vec};
use std::{
    collections::BTreeMap,
    hash::{Hash, Hasher},
    sync::{Mutex, MutexGuard, PoisonError},
};
use uuid::Uuid;

/// Changed with what the index keeps; with the version of the app too,
/// whose rules decide the status of a match.
const FORMAT: u32 = 1;

fn version() -> String {
    format!("{FORMAT}/{}", env!("CARGO_PKG_VERSION"))
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
struct IndexFile {
    version: String,
    /// by team id, then by match id (its folder)
    teams: BTreeMap<String, TeamIndex>,
}

/// A match as indexed: the stamp of its files, and its summary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Indexed {
    pub stamp: u64,
    pub summary: MatchSummary,
}

/// The matches of a team, by id.
pub(crate) type TeamIndex = BTreeMap<String, Indexed>;

#[derive(Debug, Default)]
pub(crate) struct MatchIndex {
    /// `None` until read
    loaded: Mutex<Option<IndexFile>>,
    /// one write at a time: they go through the same temporary file
    writing: futures::lock::Mutex<()>,
}

impl MatchIndex {
    /// The matches of the team as indexed.
    pub(crate) async fn team(&self, store: &dyn Store, team: &Uuid) -> TeamIndex {
        if self.lock().is_none() {
            let read = read_index(store).await;
            self.lock().get_or_insert(read);
        }
        self.lock()
            .as_ref()
            .and_then(|index| index.teams.get(&team.to_string()).cloned())
            .unwrap_or_default()
    }

    /// The matches of the team as they are now, saved if they changed. A
    /// write that fails only costs the next listing a read of the matches.
    pub(crate) async fn update(&self, store: &dyn Store, team: &Uuid, matches: TeamIndex) {
        let _writing = self.writing.lock().await;
        let json = {
            let mut loaded = self.lock();
            let index = loaded.get_or_insert_with(empty);
            let key = team.to_string();
            match index.teams.get(&key) == Some(&matches) {
                true => None,
                false => {
                    index.teams.insert(key, matches);
                    to_vec(&*index).ok()
                }
            }
        };
        if let Some(json) = json
            && let Err(e) = store.write(layout::MATCH_INDEX, &json).await
        {
            log_error(&format!("the index of the matches wasn't saved: {e}"));
        }
    }

    fn lock(&self) -> MutexGuard<'_, Option<IndexFile>> {
        self.loaded.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

fn empty() -> IndexFile {
    IndexFile {
        version: version(),
        teams: BTreeMap::new(),
    }
}

async fn read_index(store: &dyn Store) -> IndexFile {
    store
        .read(layout::MATCH_INDEX)
        .await
        .ok()
        .flatten()
        .and_then(|json| from_slice::<IndexFile>(&json).ok())
        .filter(|index| index.version == version())
        .unwrap_or_else(empty)
}

/// The stamp of a match: of `team.json` and of the files of its folder;
/// `None` when one of them isn't settled.
pub(crate) fn fingerprint(team: Option<Stamp>, files: &[(String, Option<Stamp>)]) -> Option<u64> {
    let mut hasher = FxHasher::default();
    team?.hash(&mut hasher);
    for (name, stamp) in files {
        name.hash(&mut hasher);
        (*stamp)?.hash(&mut hasher);
    }
    Some(hasher.finish())
}
