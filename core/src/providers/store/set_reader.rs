use crate::{
    errors::AppError,
    legacy::folder::read_sets,
    localization::Language,
    providers::set_reader::SetReader,
    shapes::{r#match::MatchEntry, set::SetEntry},
    storage::{Store, layout},
};
use async_trait::async_trait;
use std::sync::Arc;

pub struct StoreSetReader {
    store: Arc<dyn Store>,
    language: Language,
}

impl StoreSetReader {
    pub fn new(store: Arc<dyn Store>, language: Language) -> Self {
        Self { store, language }
    }
}

#[async_trait]
impl SetReader for StoreSetReader {
    async fn read_all(&self, m: &MatchEntry) -> Result<Vec<SetEntry>, AppError> {
        read_sets(
            &*self.store,
            &layout::match_folder(&m.team.id, &m.id),
            self.language.labels(),
        )
        .await
    }
}
