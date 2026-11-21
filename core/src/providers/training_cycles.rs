use crate::{
    errors::AppError,
    training::cycles::{Child, Cycle, CycleInput, Level},
};
use async_trait::async_trait;
use chrono::NaiveDate;
use uuid::Uuid;

#[async_trait]
pub trait TrainingCycles {
    /// The cycles of the level of a team not removed, by start.
    async fn cycles(&self, team_id: &Uuid, level: Level) -> Result<Vec<Cycle>, AppError>;

    async fn cycle(&self, level: Level, id: &Uuid) -> Result<Cycle, AppError>;

    /// The cycles of the level whose window holds the date: what the form
    /// of a session or a cycle proposes (at most one, as they don't overlap).
    async fn containing(
        &self,
        team_id: &Uuid,
        level: Level,
        date: NaiveDate,
    ) -> Result<Vec<Cycle>, AppError>;

    /// What the cycle holds, by start: cycles of the level below, sessions
    /// for a microcycle.
    async fn children(&self, level: Level, id: &Uuid) -> Result<Vec<Child>, AppError>;

    /// What the cycle could still hold: the free ones of its team, below it,
    /// that fit its window ("Add").
    async fn free_children(&self, level: Level, id: &Uuid) -> Result<Vec<Child>, AppError>;

    /// A new cycle of the team, holding `children` (free, of the team, in
    /// the window: ids of the level below, of sessions for a microcycle):
    /// that is how a cycle is made from below. The window must be inside its
    /// parent's and overlap no other cycle of the level.
    async fn create_cycle(
        &self,
        team_id: &Uuid,
        level: Level,
        input: CycleInput,
        children: Vec<Uuid>,
    ) -> Result<Cycle, AppError>;

    /// The cycle as the input says. A child left outside the new window
    /// stops everything (`ChildrenOutside` says which), unless `detach`:
    /// then they are set free and the cycle is saved.
    async fn update_cycle(
        &self,
        level: Level,
        id: &Uuid,
        input: CycleInput,
        detach: bool,
    ) -> Result<Cycle, AppError>;

    /// Adds free children to the cycle; each in its window, or nothing is
    /// written.
    async fn add_children(
        &self,
        level: Level,
        id: &Uuid,
        children: Vec<Uuid>,
    ) -> Result<(), AppError>;

    /// Marks the cycle removed (D7). Its children go too (all the levels
    /// below, and the sessions) or, with `with_children` false, are set free.
    async fn remove_cycle(
        &self,
        level: Level,
        id: &Uuid,
        with_children: bool,
    ) -> Result<(), AppError>;
}
