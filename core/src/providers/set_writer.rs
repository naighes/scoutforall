use crate::{
    domain::Side,
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

    /// Starts a set scouted on both sides, with no touches: `a` and `b` the
    /// starts of the sides (lineup, serving team as each sees it). A set
    /// saved and not removed: an error; a removed one comes back as this one.
    async fn create_both(&self, m: &MatchEntry, a: &SetEntry, b: &SetEntry)
    -> Result<(), AppError>;

    async fn append_event(
        &self,
        m: &MatchEntry,
        set_number: u8,
        event: &EventEntry,
    ) -> Result<(), AppError>;

    /// Appends a touch of `side` to a set scouted on both sides. A set
    /// scouted on A alone: an error.
    async fn append_touch(
        &self,
        m: &MatchEntry,
        set_number: u8,
        side: Side,
        event: &EventEntry,
    ) -> Result<(), AppError>;

    /// Removes the last touch of the set scouted on both sides, of
    /// whichever side, and tells it.
    async fn remove_last_touch(
        &self,
        m: &MatchEntry,
        set_number: u8,
    ) -> Result<Option<(Side, EventEntry)>, AppError>;

    async fn remove_last_event(
        &self,
        m: &MatchEntry,
        set_number: u8,
    ) -> Result<Option<EventEntry>, AppError>;

    /// Saves the set, its descriptor and its events, in place of the one
    /// saved. A set scouted on both sides: an error (its events of A alone
    /// would spoil it).
    async fn update(&self, m: &MatchEntry, set: &SetEntry) -> Result<(), AppError>;
    /// Saves the set in place of the one saved: `a` and `b` the starts of
    /// the sides (lineup, serving team as each sees it, their events
    /// aside), B's only for a set scouted on both sides; the touches in
    /// order, each in its place, a changed one updated in its row, the ones
    /// beyond removed or appended. Touches of B without its start, or that
    /// don't replay on both sides: an error, nothing saved.
    async fn update_sides(
        &self,
        m: &MatchEntry,
        a: &SetEntry,
        b: Option<&SetEntry>,
        touches: &[(Side, EventEntry)],
    ) -> Result<(), AppError>;
    /// Removes the set, if saved: its descriptor, then its events.
    async fn remove(&self, m: &MatchEntry, set_number: u8) -> Result<(), AppError>;
}
