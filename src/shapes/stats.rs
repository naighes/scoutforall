use crate::shapes::enums::{ErrorTypeEnum, EvalEnum, EventTypeEnum, PhaseEnum, ZoneEnum};
use std::collections::HashMap;
use std::hash::Hash;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EventsStatsKey {
    pub event_type: EventTypeEnum,
    pub phase: PhaseEnum,
    pub rotation: u8,
    pub player: Option<Uuid>,
    pub zone: Option<ZoneEnum>,
    pub eval: Option<EvalEnum>,
}

#[derive(Debug, Clone)]
pub struct EventsStats(pub HashMap<EventsStatsKey, u32>);

impl EventsStats {
    pub fn new() -> Self {
        EventsStats(HashMap::new())
    }

    pub fn add(
        &mut self,
        event_type: EventTypeEnum,
        phase: PhaseEnum,
        rotation: u8,
        player: Option<Uuid>,
        zone: Option<ZoneEnum>,
        eval: Option<EvalEnum>,
    ) {
        let key = EventsStatsKey {
            event_type,
            phase,
            rotation,
            player,
            zone,
            eval,
        };
        *self.0.entry(key).or_insert(0) += 1;
    }

    pub fn merge(&mut self, other: &EventsStats) {
        for (k, v) in &other.0 {
            *self.0.entry(k.clone()).or_insert(0) += v;
        }
    }
    pub fn query<'a>(
        &'a self,
        event_type: Option<EventTypeEnum>,
        phase: Option<PhaseEnum>,
        rotation: Option<u8>,
        player: Option<Option<Uuid>>,
        zone: Option<Option<ZoneEnum>>,
        eval: Option<Option<EvalEnum>>,
    ) -> impl Iterator<Item = (&'a EventsStatsKey, &'a u32)> {
        self.0.iter().filter(move |(k, _)| {
            event_type.map_or(true, |et| k.event_type == et)
                && phase.map_or(true, |ph| k.phase == ph)
                && rotation.map_or(true, |r| k.rotation == r)
                && player.map_or(true, |p| p.is_none() || k.player == p)
                && zone.map_or(true, |z| z.is_none() || k.zone == z)
                && eval.map_or(true, |e| e.is_none() || k.eval == e)
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CounterAttackStatsKey {
    pub phase: PhaseEnum,
    pub rotation: u8,
    pub player: Uuid,
    pub zone: ZoneEnum,
    pub eval: EvalEnum,
}

#[derive(Debug, Clone)]
pub struct CounterAttackStats(pub HashMap<CounterAttackStatsKey, u32>);

impl CounterAttackStats {
    pub fn new() -> Self {
        CounterAttackStats(HashMap::new())
    }

    pub fn add(
        &mut self,
        phase: PhaseEnum,
        rotation: u8,
        player: Uuid,
        zone: ZoneEnum,
        eval: EvalEnum,
    ) {
        let key = CounterAttackStatsKey {
            phase,
            rotation,
            player,
            zone,
            eval,
        };
        *self.0.entry(key).or_insert(0) += 1;
    }

    pub fn merge(&mut self, other: &CounterAttackStats) {
        for (k, v) in &other.0 {
            *self.0.entry(k.clone()).or_insert(0) += v;
        }
    }

    pub fn query<'a>(
        &'a self,
        phase: Option<PhaseEnum>,
        rotation: Option<u8>,
        player: Option<Uuid>,
        zone: Option<ZoneEnum>,
        eval: Option<EvalEnum>,
    ) -> impl Iterator<Item = (&'a CounterAttackStatsKey, &'a u32)> {
        self.0.iter().filter(move |(k, _)| {
            phase.map_or(true, |p| k.phase == p)
                && rotation.map_or(true, |r| k.rotation == r)
                && player.map_or(true, |pl| k.player == pl)
                && zone.map_or(true, |et| k.zone == et)
                && eval.map_or(true, |et| k.eval == et)
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AttackStatsKey {
    pub phase: PhaseEnum,
    pub rotation: u8,
    pub player: Uuid,
    pub zone: ZoneEnum,
    pub eval: EvalEnum,
    pub prev_eval: EvalEnum,
}

#[derive(Debug, Clone)]
pub struct AttackStats(pub HashMap<AttackStatsKey, u32>);

impl AttackStats {
    pub fn new() -> Self {
        AttackStats(HashMap::new())
    }

    pub fn add(
        &mut self,
        phase: PhaseEnum,
        rotation: u8,
        player: Uuid,
        zone: ZoneEnum,
        eval: EvalEnum,
        prev_eval: EvalEnum,
    ) {
        let key = AttackStatsKey {
            phase,
            rotation,
            player,
            zone,
            eval,
            prev_eval,
        };
        *self.0.entry(key).or_insert(0) += 1;
    }

    pub fn merge(&mut self, other: &AttackStats) {
        for (k, v) in &other.0 {
            *self.0.entry(k.clone()).or_insert(0) += v;
        }
    }

    pub fn query<'a>(
        &'a self,
        phase: Option<PhaseEnum>,
        rotation: Option<u8>,
        player: Option<Uuid>,
        zone: Option<ZoneEnum>,
        eval: Option<EvalEnum>,
        prev_eval: Option<EvalEnum>,
    ) -> impl Iterator<Item = (&'a AttackStatsKey, &'a u32)> {
        self.0.iter().filter(move |(k, _)| {
            phase.map_or(true, |p| k.phase == p)
                && rotation.map_or(true, |r| k.rotation == r)
                && player.map_or(true, |pl| k.player == pl)
                && zone.map_or(true, |et| k.zone == et)
                && eval.map_or(true, |et| k.eval == et)
                && prev_eval.map_or(true, |et| k.prev_eval == et)
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DistributionsStatsKey {
    pub phase: PhaseEnum,
    pub rotation: u8,
    pub zone: ZoneEnum,
    pub eval: EvalEnum,
    pub attack_eval: EvalEnum,
}

#[derive(Debug, Clone)]
pub struct DistributionStats(pub HashMap<DistributionsStatsKey, u32>);

impl DistributionStats {
    pub fn new() -> Self {
        DistributionStats(HashMap::new())
    }

    pub fn add(
        &mut self,
        phase: PhaseEnum,
        rotation: u8,
        zone: ZoneEnum,
        eval: EvalEnum,
        attack_eval: EvalEnum,
    ) {
        let key = DistributionsStatsKey {
            phase,
            rotation,
            zone,
            eval,
            attack_eval,
        };
        *self.0.entry(key).or_insert(0) += 1;
    }

    pub fn merge(&mut self, other: &DistributionStats) {
        for (k, v) in &other.0 {
            *self.0.entry(k.clone()).or_insert(0) += v;
        }
    }

    pub fn query<'a>(
        &'a self,
        phase: Option<PhaseEnum>,
        rotation: Option<u8>,
        zone: Option<ZoneEnum>,
        eval: Option<EvalEnum>,
        attack_eval: Option<EvalEnum>,
    ) -> impl Iterator<Item = (&'a DistributionsStatsKey, &'a u32)> {
        self.0.iter().filter(move |(k, _)| {
            phase.map_or(true, |p| k.phase == p)
                && rotation.map_or(true, |r| k.rotation == r)
                && zone.map_or(true, |pl| k.zone == pl)
                && eval.map_or(true, |et| k.eval == et)
                && attack_eval.map_or(true, |et| k.attack_eval == et)
        })
    }

    pub fn zone_stats(
        &self,
        zone: ZoneEnum,
        phase: Option<PhaseEnum>,
        rotation: Option<u8>,
        prev_eval_filter: Option<EvalEnum>,
    ) -> (f64, f64) {
        let mut total_balls = 0u32;
        let mut balls_in_zone = 0u32;
        let mut attacks_total = 0u32;
        let mut attacks_won = 0u32;
        for (key, count) in self.query(phase, rotation, Some(zone), prev_eval_filter, None) {
            total_balls += count;
            if key.zone == zone {
                balls_in_zone += count;
                attacks_total += count;

                if key.attack_eval == EvalEnum::Perfect {
                    attacks_won += count;
                }
            }
        }
        let zone_percentage = if total_balls == 0 {
            0.0
        } else {
            balls_in_zone as f64 / total_balls as f64 * 100.0
        };
        let attack_success_percentage = if attacks_total == 0 {
            0.0
        } else {
            attacks_won as f64 / attacks_total as f64 * 100.0
        };
        (zone_percentage, attack_success_percentage)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ErrorsStatsKey {
    pub phase: PhaseEnum,
    pub rotation: u8,
    pub player: Uuid,
    pub error_type: ErrorTypeEnum,
}

#[derive(Debug, Clone)]
pub struct ErrorsStats(pub HashMap<ErrorsStatsKey, u32>);

impl ErrorsStats {
    pub fn new() -> Self {
        ErrorsStats(HashMap::new())
    }

    pub fn add(&mut self, phase: PhaseEnum, rotation: u8, player: Uuid, error_type: ErrorTypeEnum) {
        let key = ErrorsStatsKey {
            phase,
            rotation,
            player,
            error_type,
        };
        *self.0.entry(key).or_insert(0) += 1;
    }

    pub fn merge(&mut self, other: &ErrorsStats) {
        for (k, v) in &other.0 {
            *self.0.entry(k.clone()).or_insert(0) += v;
        }
    }

    pub fn query<'a>(
        &'a self,
        phase: Option<PhaseEnum>,
        rotation: Option<u8>,
        player: Option<Uuid>,
        error_type: Option<ErrorTypeEnum>,
    ) -> impl Iterator<Item = (&'a ErrorsStatsKey, &'a u32)> {
        self.0.iter().filter(move |(k, _)| {
            phase.map_or(true, |p| k.phase == p)
                && rotation.map_or(true, |r| k.rotation == r)
                && player.map_or(true, |pl| k.player == pl)
                && error_type.map_or(true, |et| k.error_type == et)
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct OpponentsErrorKey {
    pub phase: PhaseEnum,
    pub rotation: u8,
}

#[derive(Debug, Clone)]
pub struct OpponentErrorsStats(pub HashMap<OpponentsErrorKey, u32>);

impl OpponentErrorsStats {
    pub fn new() -> Self {
        OpponentErrorsStats(HashMap::new())
    }

    pub fn add(&mut self, phase: PhaseEnum, rotation: u8) {
        let key = OpponentsErrorKey { phase, rotation };
        *self.0.entry(key).or_insert(0) += 1;
    }

    pub fn merge(&mut self, other: &OpponentErrorsStats) {
        for (k, v) in &other.0 {
            *self.0.entry(k.clone()).or_insert(0) += v;
        }
    }

    pub fn query<'a>(
        &'a self,
        phase: Option<PhaseEnum>,
        rotation: Option<u8>,
    ) -> impl Iterator<Item = (&'a OpponentsErrorKey, &'a u32)> {
        self.0.iter().filter(move |(k, _)| {
            phase.map_or(true, |p| k.phase == p) && rotation.map_or(true, |r| k.rotation == r)
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PointsStatsKey {
    pub phase: PhaseEnum,
    pub rotation: u8,
}

#[derive(Debug, Clone)]
pub struct PointsStats(pub HashMap<PointsStatsKey, u32>);

impl PointsStats {
    pub fn new() -> Self {
        PointsStats(HashMap::new())
    }

    pub fn add(&mut self, phase: PhaseEnum, rotation: u8) {
        let key = PointsStatsKey { phase, rotation };
        *self.0.entry(key).or_insert(0) += 1;
    }

    pub fn merge(&mut self, other: &PointsStats) {
        for (k, v) in &other.0 {
            *self.0.entry(k.clone()).or_insert(0) += v;
        }
    }

    pub fn query<'a>(
        &'a self,
        phase: Option<PhaseEnum>,
        rotation: Option<u8>,
    ) -> impl Iterator<Item = (&'a PointsStatsKey, &'a u32)> {
        self.0.iter().filter(move |(k, _)| {
            phase.map_or(true, |p| k.phase == p) && rotation.map_or(true, |r| k.rotation == r)
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PossessionsStatsKey {
    pub phase: PhaseEnum,
    pub rotation: u8,
}

#[derive(Debug, Clone)]
pub struct PossessionsStats(pub HashMap<PossessionsStatsKey, u32>);

impl PossessionsStats {
    pub fn new() -> Self {
        PossessionsStats(HashMap::new())
    }

    pub fn add(&mut self, phase: PhaseEnum, rotation: u8) {
        let key = PossessionsStatsKey { phase, rotation };
        *self.0.entry(key).or_insert(0) += 1;
    }

    pub fn merge(&mut self, other: &PossessionsStats) {
        for (k, v) in &other.0 {
            *self.0.entry(k.clone()).or_insert(0) += v;
        }
    }

    pub fn query<'a>(
        &'a self,
        phase: Option<PhaseEnum>,
        rotation: Option<u8>,
    ) -> impl Iterator<Item = (&'a PossessionsStatsKey, &'a u32)> {
        self.0.iter().filter(move |(k, _)| {
            phase.map_or(true, |p| k.phase == p) && rotation.map_or(true, |r| k.rotation == r)
        })
    }
}

#[derive(Debug, Clone)]
pub struct Stats {
    pub events: EventsStats,
    pub distribution: DistributionStats,
    pub errors: ErrorsStats,
    pub opponent_errors: OpponentErrorsStats,
    pub possessions: PossessionsStats,
    pub attack: AttackStats,
    pub counter_attack: CounterAttackStats,
    pub points: PointsStats,
}

impl Stats {
    pub fn new() -> Self {
        Stats {
            events: EventsStats::new(),
            distribution: DistributionStats::new(),
            errors: ErrorsStats::new(),
            opponent_errors: OpponentErrorsStats::new(),
            possessions: PossessionsStats::new(),
            attack: AttackStats::new(),
            counter_attack: CounterAttackStats::new(),
            // TODO: rename to "earned points"
            points: PointsStats::new(),
        }
    }

    pub fn merge(&mut self, other: &Stats) {
        self.attack.merge(&other.attack);
        self.counter_attack.merge(&other.counter_attack);
        self.distribution.merge(&other.distribution);
        self.errors.merge(&other.errors);
        self.events.merge(&other.events);
        self.opponent_errors.merge(&other.opponent_errors);
        self.points.merge(&other.points);
        self.possessions.merge(&other.possessions);
    }

    pub fn sum_poss_and_points(
        &self,
        phase: Option<PhaseEnum>,
        rotation: Option<u8>,
    ) -> (u32, u32) {
        let total_poss: u32 = self
            .possessions
            .query(phase, rotation)
            .map(|(_, v)| *v)
            .sum();
        let total_pts: u32 = self.points.query(phase, rotation).map(|(_, v)| *v).sum();
        (total_poss, total_pts)
    }

    pub fn number_of_possessions_per_point(
        &self,
        phase: Option<PhaseEnum>,
        rotation: Option<u8>,
    ) -> Option<f64> {
        let (total_poss, total_pts) = self.sum_poss_and_points(phase, rotation);
        if total_pts == 0 {
            None
        } else {
            Some(total_poss as f64 / total_pts as f64)
        }
    }

    pub fn points_per_possession(
        &self,
        phase: Option<PhaseEnum>,
        rotation: Option<u8>,
    ) -> Option<f64> {
        let (total_poss, total_pts) = self.sum_poss_and_points(phase, rotation);
        if total_poss == 0 {
            None
        } else {
            Some(total_pts as f64 / total_poss as f64)
        }
    }

    pub fn attack_efficiency(
        &self,
        player: Option<Uuid>,
        phase: Option<PhaseEnum>,
        rotation: Option<u8>,
        zone: Option<ZoneEnum>,
        prev_eval: EvalEnum,
    ) -> Option<f64> {
        let mut score_sum: i32 = 0;
        let mut total: u32 = 0;
        for (key, count) in self
            .attack
            .query(phase, rotation, player, zone, None, Some(prev_eval))
        {
            total += *count;

            let score = match key.eval {
                EvalEnum::Perfect => 1,
                EvalEnum::Error | EvalEnum::Over => -1,
                _ => 0,
            };
            score_sum += score * (*count as i32);
        }
        (total > 0).then_some((score_sum as f64) / (total as f64) * 100.0)
    }

    pub fn counter_attack_conversion_rate(
        &self,
        player: Option<Uuid>,
        phase: Option<PhaseEnum>,
        rotation: Option<u8>,
        zone: Option<ZoneEnum>,
    ) -> Option<f64> {
        let mut total_attempts: u32 = 0;
        let mut points_from_counter: u32 = 0;

        for (key, count) in self
            .counter_attack
            .query(phase, rotation, player, zone, None)
        {
            total_attempts += *count;
            if key.eval == EvalEnum::Perfect {
                points_from_counter += *count;
            }
        }
        (total_attempts > 0).then_some(100.0 * points_from_counter as f64 / total_attempts as f64)
    }

    pub fn count_events(
        &self,
        event_type: EventTypeEnum,
        player: Option<Uuid>,
        phase: Option<PhaseEnum>,
        rotation: Option<u8>,
        zone: Option<ZoneEnum>,
        eval: Option<EvalEnum>,
    ) -> Option<u32> {
        let total: u32 = self
            .events
            .query(
                Some(event_type),
                phase,
                rotation,
                Some(player),
                Some(zone),
                Some(eval),
            )
            .map(|(_, count)| *count)
            .sum();

        (total > 0).then_some(total)
    }

    pub fn percentage_with_scoring<F>(
        &self,
        event_type: EventTypeEnum,
        player: Option<Uuid>,
        phase: Option<PhaseEnum>,
        rotation: Option<u8>,
        zone: Option<ZoneEnum>,
        score_fn: F,
    ) -> Option<f64>
    where
        F: Fn(&EvalEnum) -> i32,
    {
        let mut score_sum: i32 = 0;
        let mut total: u32 = 0;
        for (key, count) in self.events.query(
            Some(event_type),
            phase,
            rotation,
            Some(player),
            Some(zone),
            None,
        ) {
            if let Some(eval) = &key.eval {
                total += *count;
                score_sum += score_fn(eval) * (*count as i32);
            }
        }
        (total > 0).then_some((score_sum as f64) / (total as f64) * 100.0)
    }

    pub fn event_type_percentage(
        &self,
        event_type: EventTypeEnum,
        player: Option<Uuid>,
        phase: Option<PhaseEnum>,
        rotation: Option<u8>,
        zone: Option<ZoneEnum>,
        eval: EvalEnum,
    ) -> Option<f64> {
        let mut eval_count: u64 = 0;
        let mut total_count: u64 = 0;
        for (key, count) in self.events.query(
            Some(event_type),
            phase,
            rotation,
            Some(player),
            Some(zone),
            None,
        ) {
            total_count += *count as u64;
            if key.eval.as_ref() == Some(&eval) {
                eval_count += *count as u64;
            }
        }
        (total_count > 0).then_some(eval_count as f64 / total_count as f64 * 100.0)
    }

    pub fn positive_pass_percentage(
        &self,
        player: Option<Uuid>,
        phase: Option<PhaseEnum>,
        rotation: Option<u8>,
    ) -> Option<f64> {
        self.percentage_with_scoring(EventTypeEnum::P, player, phase, rotation, None, |eval| {
            match eval {
                EvalEnum::Perfect | EvalEnum::Positive => 1,
                _ => 0,
            }
        })
    }

    pub fn pass_efficiency_percentage(
        &self,
        player: Option<Uuid>,
        phase: Option<PhaseEnum>,
        rotation: Option<u8>,
    ) -> Option<f64> {
        self.percentage_with_scoring(EventTypeEnum::P, player, phase, rotation, None, |eval| {
            match eval {
                EvalEnum::Perfect | EvalEnum::Positive => 1,
                EvalEnum::Error | EvalEnum::Over => -1,
                _ => 0,
            }
        })
    }

    pub fn attack_efficiency_percentage(
        &self,
        player: Option<Uuid>,
        phase: Option<PhaseEnum>,
        rotation: Option<u8>,
        zone: Option<ZoneEnum>,
    ) -> Option<f64> {
        self.percentage_with_scoring(EventTypeEnum::A, player, phase, rotation, zone, |eval| {
            match eval {
                EvalEnum::Perfect => 1,
                EvalEnum::Error | EvalEnum::Over => -1,
                _ => 0,
            }
        })
    }

    pub fn positive_dig_percentage(
        &self,
        player: Option<Uuid>,
        phase: Option<PhaseEnum>,
        rotation: Option<u8>,
    ) -> Option<f64> {
        self.percentage_with_scoring(EventTypeEnum::D, player, phase, rotation, None, |eval| {
            match eval {
                EvalEnum::Perfect | EvalEnum::Positive => 1,
                _ => 0,
            }
        })
    }

    pub fn dig_efficiency_percentage(
        &self,
        player: Option<Uuid>,
        phase: Option<PhaseEnum>,
        rotation: Option<u8>,
    ) -> Option<f64> {
        self.percentage_with_scoring(EventTypeEnum::D, player, phase, rotation, None, |eval| {
            match eval {
                EvalEnum::Perfect | EvalEnum::Positive | EvalEnum::Over => 1,
                EvalEnum::Error => -1,
                _ => 0,
            }
        })
    }

    pub fn positive_serve_percentage(
        &self,
        player: Option<Uuid>,
        phase: Option<PhaseEnum>,
        rotation: Option<u8>,
    ) -> Option<f64> {
        self.percentage_with_scoring(EventTypeEnum::S, player, phase, rotation, None, |eval| {
            match eval {
                EvalEnum::Perfect | EvalEnum::Positive | &EvalEnum::Over => 1,
                _ => 0,
            }
        })
    }

    pub fn serve_efficiency_percentage(
        &self,
        player: Option<Uuid>,
        phase: Option<PhaseEnum>,
        rotation: Option<u8>,
    ) -> Option<f64> {
        self.percentage_with_scoring(EventTypeEnum::S, player, phase, rotation, None, |eval| {
            match eval {
                EvalEnum::Perfect
                | EvalEnum::Positive
                | &EvalEnum::Over
                | &EvalEnum::Exclamative => 1,
                EvalEnum::Error => -1,
                _ => 0,
            }
        })
    }

    pub fn positive_block_percentage(
        &self,
        player: Option<Uuid>,
        phase: Option<PhaseEnum>,
        rotation: Option<u8>,
    ) -> Option<f64> {
        self.percentage_with_scoring(EventTypeEnum::B, player, phase, rotation, None, |eval| {
            match eval {
                EvalEnum::Perfect | EvalEnum::Positive => 1,
                _ => 0,
            }
        })
    }

    pub fn block_efficiency_percentage(
        &self,
        player: Option<Uuid>,
        phase: Option<PhaseEnum>,
        rotation: Option<u8>,
    ) -> Option<f64> {
        self.percentage_with_scoring(EventTypeEnum::B, player, phase, rotation, None, |eval| {
            match eval {
                EvalEnum::Perfect | EvalEnum::Positive => 1,
                EvalEnum::Error | EvalEnum::Over => -1,
                _ => 0,
            }
        })
    }
}
