//! The season totals shown on a player's profile (TODO/011, step 3).

use crate::{
    analysis,
    shapes::{
        enums::{EvalEnum, EventTypeEnum},
        r#match::MatchEntry,
        stats::StatsFilter,
    },
};
use uuid::Uuid;

/// Match, set and direct points for one player in the matches of a team.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PlayerSeasonStats {
    /// Scouted matches in the team's season, including matches the player
    /// didn't enter.
    pub matches_scouted: u32,
    /// Matches where the player was on court at least once.
    pub matches_played: u32,
    /// Sets where the player was on court at least once.
    pub sets_played: u32,
    pub attack_points: u32,
    pub block_points: u32,
    pub serve_points: u32,
}

impl PlayerSeasonStats {
    /// Counts a player's season totals from the readable matches of one
    /// team. `matches_scouted` comes from the team's match list so unreadable
    /// scouted matches remain in the denominator.
    pub fn of(player: Uuid, matches: &[MatchEntry], matches_scouted: usize) -> Self {
        let mut result = Self {
            matches_scouted: u32::try_from(matches_scouted).unwrap_or(u32::MAX),
            ..Self::default()
        };
        for m in matches {
            let mut entered = false;
            for set in &m.sets {
                if set
                    .compute_snapshot(m.max_substitutions)
                    .is_ok_and(|snapshot| {
                        snapshot
                            .current_lineup
                            .get_involved_players()
                            .contains(&player)
                    })
                {
                    result.sets_played = result.sets_played.saturating_add(1);
                    entered = true;
                }
            }
            if entered {
                result.matches_played = result.matches_played.saturating_add(1);
            }

            if let Ok(stats) = analysis::match_stats(m) {
                let filter = StatsFilter::new(None, None, Some(player));
                result.attack_points = result.attack_points.saturating_add(stats.event_count(
                    EventTypeEnum::A,
                    Some(EvalEnum::Perfect),
                    &filter,
                ));
                result.block_points = result.block_points.saturating_add(stats.event_count(
                    EventTypeEnum::B,
                    Some(EvalEnum::Perfect),
                    &filter,
                ));
                result.serve_points = result.serve_points.saturating_add(stats.event_count(
                    EventTypeEnum::S,
                    Some(EvalEnum::Perfect),
                    &filter,
                ));
            }
        }
        result
    }

    pub fn points(&self) -> u32 {
        self.attack_points
            .saturating_add(self.block_points)
            .saturating_add(self.serve_points)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shapes::{
        enums::TeamSideEnum, player::PlayerEntry, set::SetEntry, snapshot::EventEntry,
        team::TeamEntry,
    };
    use chrono::{DateTime, Utc};

    #[test]
    fn counts_sets_entered_and_points_by_fundamental() {
        let players = std::array::from_fn(|i| Uuid::from_u128(i as u128 + 1));
        let mut team = TeamEntry {
            players: players
                .iter()
                .enumerate()
                .map(|(i, id)| PlayerEntry {
                    id: *id,
                    name: format!("Player {}", i + 1),
                    number: i as u8 + 1,
                    ..PlayerEntry::default()
                })
                .collect(),
            ..TeamEntry::default()
        };
        team.id = Uuid::from_u128(20);
        let player = players[0];
        let mut set =
            SetEntry::new(1, TeamSideEnum::Us, players, players[5], None, players[0]).unwrap();
        set.events.push(EventEntry {
            timestamp: DateTime::<Utc>::UNIX_EPOCH,
            event_type: EventTypeEnum::S,
            player: Some(player),
            eval: Some(EvalEnum::Perfect),
            target_player: None,
            technique: None,
            flags: Default::default(),
        });
        let m = MatchEntry {
            opponent: "Other Club".into(),
            date: "2026-11-25T18:00:00+01:00".parse().unwrap(),
            id: "match".into(),
            team,
            home: true,
            max_substitutions: 6,
            notes: String::new(),
            roster: players.to_vec(),
            sets: vec![set],
            source: Default::default(),
            video_clock: false,
        };

        let stats = PlayerSeasonStats::of(player, &[m], 2);
        assert_eq!(stats.matches_scouted, 2);
        assert_eq!(stats.matches_played, 1);
        assert_eq!(stats.sets_played, 1);
        assert_eq!(stats.points(), 1);
        assert_eq!(
            (stats.attack_points, stats.block_points, stats.serve_points),
            (0, 0, 1)
        );
    }

    #[test]
    fn roster_presence_alone_is_not_match_participation() {
        let players = std::array::from_fn(|i| Uuid::from_u128(i as u128 + 1));
        let team = TeamEntry {
            players: players
                .iter()
                .map(|id| PlayerEntry {
                    id: *id,
                    ..PlayerEntry::default()
                })
                .collect(),
            ..TeamEntry::default()
        };
        let set =
            SetEntry::new(1, TeamSideEnum::Us, players, players[5], None, players[0]).unwrap();
        let m = MatchEntry {
            opponent: "Other Club".into(),
            date: "2026-11-25T18:00:00+01:00".parse().unwrap(),
            id: "match".into(),
            team,
            home: true,
            max_substitutions: 6,
            notes: String::new(),
            roster: players.to_vec(),
            sets: vec![set],
            source: Default::default(),
            video_clock: false,
        };

        let stats = PlayerSeasonStats::of(Uuid::from_u128(99), &[m], 1);
        assert_eq!(stats.matches_played, 0);
        assert_eq!(stats.sets_played, 0);
    }
}
