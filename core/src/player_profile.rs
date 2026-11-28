//! The numbers shown on a player's profile (TODO/011): those of one team's
//! season.

use crate::{
    errors::AppError,
    shapes::{
        enums::{EvalEnum, EventTypeEnum},
        r#match::MatchEntry,
        stats::StatsFilter,
    },
    training::totals::Totals,
};
use chrono::NaiveDate;
use uuid::Uuid;

/// Match, set and direct points for one player in the matches of a team.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PlayerSeasonStats {
    /// the matches with a set, also those the player didn't enter
    pub matches_scouted: u32,
    /// those where the player was on court at least once
    pub matches_played: u32,
    /// the sets where the player was on court at least once
    pub sets_played: u32,
    pub attack_points: u32,
    pub block_points: u32,
    pub serve_points: u32,
}

impl PlayerSeasonStats {
    /// The player's season in the matches of one team, each from its side
    /// (`SqliteMatchReader::read_scouted`).
    pub fn of(player: Uuid, matches: &[MatchEntry]) -> Result<Self, AppError> {
        matches
            .iter()
            .filter(|m| !m.sets.is_empty())
            .try_fold(Self::default(), |season, m| season.with(player, m))
    }

    fn with(self, player: Uuid, m: &MatchEntry) -> Result<Self, AppError> {
        let filter = StatsFilter::new(None, None, Some(player));
        let entered = m.sets.iter().try_fold(
            Self {
                matches_scouted: self.matches_scouted.saturating_add(1),
                ..self
            },
            |season, set| {
                let snapshot = set.compute_snapshot(m.max_substitutions)?;
                let points = |event_type| {
                    snapshot
                        .stats
                        .event_count(event_type, Some(EvalEnum::Perfect), &filter)
                };
                let on_court = snapshot
                    .current_lineup
                    .get_involved_players()
                    .contains(&player);
                Ok::<_, AppError>(Self {
                    sets_played: season.sets_played.saturating_add(u32::from(on_court)),
                    attack_points: season
                        .attack_points
                        .saturating_add(points(EventTypeEnum::A)),
                    block_points: season.block_points.saturating_add(points(EventTypeEnum::B)),
                    serve_points: season.serve_points.saturating_add(points(EventTypeEnum::S)),
                    ..season
                })
            },
        )?;
        let played = entered.sets_played > self.sets_played;
        Ok(Self {
            matches_played: entered.matches_played.saturating_add(u32::from(played)),
            ..entered
        })
    }

    pub fn points(&self) -> u32 {
        self.attack_points
            .saturating_add(self.block_points)
            .saturating_add(self.serve_points)
    }
}

/// Whether the team has had or plans a session: the profile shows the
/// attendance only then. The cancelled ones don't count.
pub fn has_training(totals: &Totals) -> bool {
    totals.planned > 0 || totals.completed > 0
}

/// The years completed on `today`, none before the birth.
pub fn age(birth: NaiveDate, today: NaiveDate) -> Option<u32> {
    today.years_since(birth)
}
