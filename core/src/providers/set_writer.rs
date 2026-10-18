use crate::{
    errors::AppError,
    shapes::{enums::TeamSideEnum, r#match::MatchEntry, set::SetEntry, snapshot::EventEntry},
};
use async_trait::async_trait;
use uuid::Uuid;

#[async_trait]
pub trait SetWriter {
    async fn create(
        &self,
        m: &MatchEntry,
        set_number: u8,
        serving_team: TeamSideEnum,
        positions: [Uuid; 6],
        libero: Uuid,
        fallback_libero: Option<Uuid>,
        setter: Uuid,
        events: Vec<EventEntry>,
    ) -> Result<SetEntry, AppError>;

    async fn append_event(
        &self,
        m: &MatchEntry,
        set_number: u8,
        event: &EventEntry,
    ) -> Result<(), AppError>;

    async fn remove_last_event(
        &self,
        m: &MatchEntry,
        set_number: u8,
    ) -> Result<Option<EventEntry>, AppError>;
}
