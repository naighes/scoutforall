use crate::shapes::enums::{ErrorTypeEnum, EvalEnum, EventTypeEnum, PhaseEnum, ZoneEnum};
use std::collections::HashMap;
use std::hash::Hash;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Metric {
    Positive,
    Efficiency,
}

impl Metric {
    fn score(&self, event: EventTypeEnum, eval: &EvalEnum) -> i32 {
        match (event, self) {
            // pass
            (EventTypeEnum::P, Metric::Positive) => match eval {
                EvalEnum::Perfect | EvalEnum::Positive => 1,
                _ => 0,
            },
            (EventTypeEnum::P, Metric::Efficiency) => match eval {
                EvalEnum::Perfect | EvalEnum::Positive => 1,
                EvalEnum::Error | EvalEnum::Over => -1,
                _ => 0,
            },
            // attack
            (EventTypeEnum::A, Metric::Efficiency) => match eval {
                EvalEnum::Perfect => 1,
                EvalEnum::Error | EvalEnum::Over => -1,
                _ => 0,
            },
            (EventTypeEnum::A, Metric::Positive) => match eval {
                EvalEnum::Perfect | EvalEnum::Positive => 1,
                _ => 0,
            },
            // dig
            (EventTypeEnum::D, Metric::Positive) => match eval {
                EvalEnum::Perfect | EvalEnum::Positive => 1,
                _ => 0,
            },
            (EventTypeEnum::D, Metric::Efficiency) => match eval {
                EvalEnum::Perfect | EvalEnum::Positive | EvalEnum::Over => 1,
                EvalEnum::Error => -1,
                _ => 0,
            },
            // serve
            (EventTypeEnum::S, Metric::Positive) => match eval {
                EvalEnum::Perfect | EvalEnum::Positive | EvalEnum::Over => 1,
                _ => 0,
            },
            (EventTypeEnum::S, Metric::Efficiency) => match eval {
                EvalEnum::Perfect | EvalEnum::Positive | EvalEnum::Over | EvalEnum::Exclamative => {
                    1
                }
                EvalEnum::Error => -1,
                _ => 0,
            },
            // block
            (EventTypeEnum::B, Metric::Positive) => match eval {
                EvalEnum::Perfect | EvalEnum::Positive => 1,
                _ => 0,
            },
            (EventTypeEnum::B, Metric::Efficiency) => match eval {
                EvalEnum::Perfect | EvalEnum::Positive => 1,
                EvalEnum::Error | EvalEnum::Over => -1,
                _ => 0,
            },
            // default fallback
            _ => 0,
        }
    }
}

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

    pub fn query(
        &self,
        event_type: Option<EventTypeEnum>,
        phase: Option<PhaseEnum>,
        rotation: Option<u8>,
        player: Option<Option<Uuid>>,
        zone: Option<Option<ZoneEnum>>,
        eval: Option<Option<EvalEnum>>,
    ) -> impl Iterator<Item = (&EventsStatsKey, &u32)> {
        self.0.iter().filter(move |(k, _)| {
            event_type.is_none_or(|et| k.event_type == et)
                && phase.is_none_or(|ph| k.phase == ph)
                && rotation.is_none_or(|r| k.rotation == r)
                && player.is_none_or(|p| p.is_none() || k.player == p)
                && zone.is_none_or(|z| z.is_none() || k.zone == z)
                && eval.is_none_or(|e| e.is_none() || k.eval == e)
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

    pub fn query(
        &self,
        phase: Option<PhaseEnum>,
        rotation: Option<u8>,
        player: Option<Uuid>,
        zone: Option<ZoneEnum>,
        eval: Option<EvalEnum>,
    ) -> impl Iterator<Item = (&CounterAttackStatsKey, &u32)> {
        self.0.iter().filter(move |(k, _)| {
            phase.is_none_or(|p| k.phase == p)
                && rotation.is_none_or(|r| k.rotation == r)
                && player.is_none_or(|pl| k.player == pl)
                && zone.is_none_or(|et| k.zone == et)
                && eval.is_none_or(|et| k.eval == et)
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FirstRallyStatsKey {
    pub rotation: u8,
    pub reception_eval: Option<EvalEnum>,
    pub finalizing_event_type: Option<EventTypeEnum>,
    pub finalizing_event_eval: Option<EvalEnum>,
}

#[derive(Debug, Clone)]
pub struct FirstRallyStats(pub HashMap<FirstRallyStatsKey, u32>);

impl FirstRallyStats {
    pub fn new() -> Self {
        FirstRallyStats(HashMap::new())
    }

    pub fn add(
        &mut self,
        rotation: u8,
        reception_eval: Option<EvalEnum>,
        finalizing_event_type: Option<EventTypeEnum>,
        finalizing_event_eval: Option<EvalEnum>,
    ) {
        let key = FirstRallyStatsKey {
            rotation,
            reception_eval,
            finalizing_event_type,
            finalizing_event_eval,
        };
        *self.0.entry(key).or_insert(0) += 1;
    }

    pub fn merge(&mut self, other: &FirstRallyStats) {
        for (k, v) in &other.0 {
            *self.0.entry(k.clone()).or_insert(0) += v;
        }
    }

    pub fn query(
        &self,
        rotation: Option<u8>,
        reception_eval: Option<EvalEnum>,
        finalizing_event_type: Option<EventTypeEnum>,
        finalizing_event_eval: Option<EvalEnum>,
    ) -> impl Iterator<Item = (&FirstRallyStatsKey, &u32)> {
        self.0.iter().filter(move |(k, _)| {
            rotation.is_none_or(|r| k.rotation == r)
                && reception_eval.is_none_or(|re| k.reception_eval == Some(re))
                && finalizing_event_type.is_none_or(|ft| k.finalizing_event_type == Some(ft))
                && finalizing_event_eval.is_none_or(|fe| k.finalizing_event_eval == Some(fe))
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

    #[allow(dead_code)]
    pub fn query(
        &self,
        phase: Option<PhaseEnum>,
        rotation: Option<u8>,
        player: Option<Uuid>,
        zone: Option<ZoneEnum>,
        eval: Option<EvalEnum>,
        prev_eval: Option<EvalEnum>,
    ) -> impl Iterator<Item = (&AttackStatsKey, &u32)> {
        self.0.iter().filter(move |(k, _)| {
            phase.is_none_or(|p| k.phase == p)
                && rotation.is_none_or(|r| k.rotation == r)
                && player.is_none_or(|pl| k.player == pl)
                && zone.is_none_or(|et| k.zone == et)
                && eval.is_none_or(|et| k.eval == et)
                && prev_eval.is_none_or(|et| k.prev_eval == et)
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DistributionsStatsKey {
    pub phase: PhaseEnum,
    pub rotation: u8,
    pub player: Uuid,
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
        player: Uuid,
        zone: ZoneEnum,
        prev_eval: EvalEnum,
        attack_eval: EvalEnum,
    ) {
        let key = DistributionsStatsKey {
            phase,
            rotation,
            player,
            zone,
            eval: prev_eval,
            attack_eval,
        };
        *self.0.entry(key).or_insert(0) += 1;
    }

    pub fn merge(&mut self, other: &DistributionStats) {
        for (k, v) in &other.0 {
            *self.0.entry(k.clone()).or_insert(0) += v;
        }
    }

    pub fn query(
        &self,
        phase: Option<PhaseEnum>,
        rotation: Option<u8>,
        player: Option<Uuid>,
        zone: Option<ZoneEnum>,
        eval: Option<EvalEnum>,
        attack_eval: Option<EvalEnum>,
    ) -> impl Iterator<Item = (&DistributionsStatsKey, &u32)> {
        self.0.iter().filter(move |(k, _)| {
            phase.is_none_or(|p| k.phase == p)
                && rotation.is_none_or(|r| k.rotation == r)
                && player.is_none_or(|p| k.player == p)
                && zone.is_none_or(|pl| k.zone == pl)
                && eval.is_none_or(|et| k.eval == et)
                && attack_eval.is_none_or(|et| k.attack_eval == et)
        })
    }

    pub fn zone_stats(
        &self,
        zone: ZoneEnum,
        phase: Option<PhaseEnum>,
        rotation: Option<u8>,
        player: Option<Uuid>,
        prev_eval_filter: Option<EvalEnum>,
    ) -> Option<(f64, f64)> {
        let mut total_balls = 0u32;
        let mut balls_in_zone = 0u32;
        let mut attacks_total = 0u32;
        let mut attacks_won = 0u32;
        for (key, count) in self.query(phase, rotation, player, None, prev_eval_filter, None) {
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
        if balls_in_zone > 0 {
            Some((zone_percentage, attack_success_percentage))
        } else {
            None
        }
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

    #[allow(dead_code)]
    pub fn query(
        &self,
        phase: Option<PhaseEnum>,
        rotation: Option<u8>,
        player: Option<Uuid>,
        error_type: Option<ErrorTypeEnum>,
    ) -> impl Iterator<Item = (&ErrorsStatsKey, &u32)> {
        self.0.iter().filter(move |(k, _)| {
            phase.is_none_or(|p| k.phase == p)
                && rotation.is_none_or(|r| k.rotation == r)
                && player.is_none_or(|pl| k.player == pl)
                && error_type.is_none_or(|et| k.error_type == et)
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

    #[allow(dead_code)]
    pub fn query(
        &self,
        phase: Option<PhaseEnum>,
        rotation: Option<u8>,
    ) -> impl Iterator<Item = (&OpponentsErrorKey, &u32)> {
        self.0.iter().filter(move |(k, _)| {
            phase.is_none_or(|p| k.phase == p) && rotation.is_none_or(|r| k.rotation == r)
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

    pub fn query(
        &self,
        phase: Option<PhaseEnum>,
        rotation: Option<u8>,
    ) -> impl Iterator<Item = (&PointsStatsKey, &u32)> {
        self.0.iter().filter(move |(k, _)| {
            phase.is_none_or(|p| k.phase == p) && rotation.is_none_or(|r| k.rotation == r)
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PossessionsStatsKey {
    pub phase: PhaseEnum,
    pub rotation: u8,
}

#[derive(Debug, Clone)]
pub struct CountStats(pub HashMap<PossessionsStatsKey, u32>);

impl CountStats {
    pub fn new() -> Self {
        CountStats(HashMap::new())
    }

    pub fn add(&mut self, phase: PhaseEnum, rotation: u8) {
        let key = PossessionsStatsKey { phase, rotation };
        *self.0.entry(key).or_insert(0) += 1;
    }

    pub fn merge(&mut self, other: &CountStats) {
        for (k, v) in &other.0 {
            *self.0.entry(k.clone()).or_insert(0) += v;
        }
    }

    pub fn query(
        &self,
        phase: Option<PhaseEnum>,
        rotation: Option<u8>,
    ) -> impl Iterator<Item = (&PossessionsStatsKey, &u32)> {
        self.0.iter().filter(move |(k, _)| {
            phase.is_none_or(|p| k.phase == p) && rotation.is_none_or(|r| k.rotation == r)
        })
    }
}

#[derive(Debug, Clone)]
pub struct Stats {
    pub events: EventsStats,
    pub distribution: DistributionStats,
    pub errors: ErrorsStats,
    pub opponent_errors: OpponentErrorsStats,
    pub possessions: CountStats,
    pub phases: CountStats,
    pub attack: AttackStats,
    pub counter_attack: CounterAttackStats,
    pub earned_points: PointsStats,
    pub scored_points: PointsStats,
    pub first_rally: FirstRallyStats,
}

impl Stats {
    pub fn new() -> Self {
        Stats {
            events: EventsStats::new(),
            distribution: DistributionStats::new(),
            errors: ErrorsStats::new(),
            opponent_errors: OpponentErrorsStats::new(),
            possessions: CountStats::new(),
            phases: CountStats::new(),
            attack: AttackStats::new(),
            counter_attack: CounterAttackStats::new(),
            earned_points: PointsStats::new(),
            scored_points: PointsStats::new(),
            first_rally: FirstRallyStats::new(),
        }
    }

    pub fn merge(&mut self, other: &Stats) {
        self.attack.merge(&other.attack);
        self.counter_attack.merge(&other.counter_attack);
        self.distribution.merge(&other.distribution);
        self.errors.merge(&other.errors);
        self.events.merge(&other.events);
        self.opponent_errors.merge(&other.opponent_errors);
        self.earned_points.merge(&other.earned_points);
        self.scored_points.merge(&other.scored_points);
        self.possessions.merge(&other.possessions);
        self.phases.merge(&other.phases);
        self.first_rally.merge(&other.first_rally);
    }

    #[allow(dead_code)]
    pub fn number_of_possessions_per_earned_point(
        &self,
        phase: Option<PhaseEnum>,
        rotation: Option<u8>,
    ) -> Option<(f64, u32, u32)> {
        let total: u32 = self
            .possessions
            .query(phase, rotation)
            .map(|(_, v)| *v)
            .sum();
        let count: u32 = self
            .earned_points
            .query(phase, rotation)
            .map(|(_, v)| *v)
            .sum();
        if count == 0 {
            None
        } else {
            Some((total as f64 / count as f64, total, count))
        }
    }

    pub fn number_of_phases_per_scored_point(
        &self,
        phase: Option<PhaseEnum>,
        rotation: Option<u8>,
    ) -> Option<(f64, u32, u32)> {
        let total: u32 = self.phases.query(phase, rotation).map(|(_, v)| *v).sum();
        let count: u32 = self
            .scored_points
            .query(phase, rotation)
            .map(|(_, v)| *v)
            .sum();
        if count == 0 {
            None
        } else {
            Some((total as f64 / count as f64, total, count))
        }
    }

    #[allow(dead_code)]
    pub fn attack_efficiency(
        &self,
        player: Option<Uuid>,
        phase: Option<PhaseEnum>,
        rotation: Option<u8>,
        zone: Option<ZoneEnum>,
        prev_eval: EvalEnum,
    ) -> Option<(f64, u32, i32)> {
        let mut count: i32 = 0;
        let mut total: u32 = 0;
        for (key, inc) in self
            .attack
            .query(phase, rotation, player, zone, None, Some(prev_eval))
        {
            total += *inc;

            let score = match key.eval {
                EvalEnum::Perfect => 1,
                EvalEnum::Error | EvalEnum::Over => -1,
                _ => 0,
            };
            count += score * (*inc as i32);
        }
        (total > 0).then_some(((count as f64) / (total as f64) * 100.0, total, count))
    }

    pub fn counter_attack_conversion_rate(
        &self,
        player: Option<Uuid>,
        phase: Option<PhaseEnum>,
        rotation: Option<u8>,
        zone: Option<ZoneEnum>,
    ) -> Option<(f64, u32, u32)> {
        let mut total: u32 = 0;
        let mut count: u32 = 0;
        for (key, incr) in self
            .counter_attack
            .query(phase, rotation, player, zone, None)
        {
            total += *incr;
            if key.eval == EvalEnum::Perfect {
                count += *incr;
            }
        }
        if total == 0 {
            None
        } else {
            Some((100.0 * count as f64 / total as f64, total, count))
        }
    }

    pub fn scored_points(
        &self,
        event_type: EventTypeEnum,
        player: Option<Uuid>,
        phase: Option<PhaseEnum>,
        rotation: Option<u8>,
        zone: Option<ZoneEnum>,
    ) -> Option<u32> {
        if event_type.provides_direct_points() {
            self.event_count(
                event_type,
                player,
                phase,
                rotation,
                zone,
                Some(EvalEnum::Perfect),
            )
        } else {
            None
        }
    }

    pub fn total_scored_points(
        &self,
        player: Option<Uuid>,
        phase: Option<PhaseEnum>,
        rotation: Option<u8>,
        zone: Option<ZoneEnum>,
    ) -> Option<u32> {
        let serve_points = self.scored_points(EventTypeEnum::S, player, phase, rotation, zone);
        let attack_points = self.scored_points(EventTypeEnum::A, player, phase, rotation, zone);
        let block_points = self.scored_points(EventTypeEnum::B, player, phase, rotation, zone);
        let total =
            serve_points.unwrap_or(0) + attack_points.unwrap_or(0) + block_points.unwrap_or(0);
        (total > 0).then_some(total)
    }

    pub fn errors(
        &self,
        event_type: EventTypeEnum,
        error_type: Option<ErrorTypeEnum>,
        player: Option<Uuid>,
        phase: Option<PhaseEnum>,
        rotation: Option<u8>,
        zone: Option<ZoneEnum>,
    ) -> Option<u32> {
        use EvalEnum::*;
        use EventTypeEnum::*;
        let evals = match event_type {
            A | B => vec![Error, Over],
            P | D | S => vec![Error],
            _ => vec![],
        };
        evals
            .into_iter()
            .filter(|eval| match error_type {
                None => true,
                Some(et) => event_type.error_type(Some(*eval)) == Some(et),
            })
            .filter_map(|eval| {
                self.event_count(event_type, player, phase, rotation, zone, Some(eval))
            })
            .reduce(|a, b| a + b)
    }

    pub fn total_errors(
        &self,
        error_type: Option<ErrorTypeEnum>,
        player: Option<Uuid>,
        phase: Option<PhaseEnum>,
        rotation: Option<u8>,
        zone: Option<ZoneEnum>,
    ) -> Option<u32> {
        let pass_errors = self.errors(EventTypeEnum::P, error_type, player, phase, rotation, zone);
        let attack_errors =
            self.errors(EventTypeEnum::A, error_type, player, phase, rotation, zone);
        let dig_errors = self.errors(EventTypeEnum::D, error_type, player, phase, rotation, zone);
        let serve_errors = self.errors(EventTypeEnum::S, error_type, player, phase, rotation, zone);
        let block_errors = self.errors(EventTypeEnum::B, error_type, player, phase, rotation, zone);
        let faults = self.event_count(EventTypeEnum::F, player, phase, rotation, zone, None);
        let total = pass_errors.unwrap_or(0)
            + attack_errors.unwrap_or(0)
            + dig_errors.unwrap_or(0)
            + serve_errors.unwrap_or(0)
            + block_errors.unwrap_or(0);
        let total = if error_type != Some(ErrorTypeEnum::Forced) {
            total
        } else {
            total + faults.unwrap_or(0)
        };
        (total > 0).then_some(total)
    }

    /// Counts the number of events matching the given filters.
    ///
    /// # Parameters
    /// - `event_type`: The type of event to evaluate (e.g. pass, attack, serve, ...).
    /// - `player`: Optional filter for a specific player id.
    /// - `phase`: Optional filter for a specific phase of play (side-out or break).
    /// - `rotation`: Optional filter for a specific team rotation index (0–5).
    /// - `zone`: Optional filter for the court zone where the event occurred.
    /// - `eval`: Optional. Filters by event evaluation (perfect, positive, error, etc.).
    ///
    /// # Returns
    /// - `Some(total)` containing the number of matching events, if greater than zero.
    /// - `None` if no matching events were found.
    /// ```
    pub fn event_count(
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

    /// Computes the "positiveness" score of a given event type under the specified filters.
    ///
    /// The positiveness score is computed by summing weighted evaluation scores
    /// (using the provided [`Metric`]) across all matching events, then normalizing
    /// by the total number of occurrences. The result is expressed as a percentage (0-100).
    ///
    /// # Parameters
    /// - `event_type`: The type of event to evaluate (e.g. pass, attack, serve, ...).
    /// - `player`: Optional filter for a specific player id.
    /// - `phase`: Optional filter for a specific phase of play (side-out or break).
    /// - `rotation`: Optional filter for a specific team rotation index (0–5).
    /// - `zone`: Optional filter for the court zone where the event occurred.
    /// - `metric`: The scoring system used to convert evaluations into weighted scores.
    ///
    /// # Returns
    /// - `Some((percentage, total, score))` if at least one event matched:
    ///   - `percentage`: The normalized positiveness score as a percentage (`f64`).
    ///   - `total`: The total number of matching events (`u32`).
    ///   - `score`: The accumulated weighted score (`i32`).
    /// - `None` if no matching events were found or if the score was zero.
    /// ```
    pub fn event_positiveness(
        &self,
        event_type: EventTypeEnum,
        player: Option<Uuid>,
        phase: Option<PhaseEnum>,
        rotation: Option<u8>,
        zone: Option<ZoneEnum>,
        metric: Metric,
    ) -> Option<(f64, u32, i32)> {
        let mut score: i32 = 0;
        let mut total: u32 = 0;
        for (key, incr) in self.events.query(
            Some(event_type),
            phase,
            rotation,
            Some(player),
            Some(zone),
            None,
        ) {
            if let Some(eval) = &key.eval {
                total += *incr;
                score += metric.score(event_type, eval) * (*incr as i32);
            }
        }
        (total > 0).then_some(((score as f64) / (total as f64) * 100.0, total, score))
    }

    pub fn sideout_first_rally_positiveness(
        &self,
        rotation: Option<u8>,
        reception_eval: Option<EvalEnum>,
        finalizing_event_type: Option<EventTypeEnum>,
        metric: Metric,
    ) -> Option<(f64, u32, i32)> {
        let mut count: i32 = 0;
        let mut total: u32 = 0;
        for (key, incr) in
            self.first_rally
                .query(rotation, reception_eval, finalizing_event_type, None)
        {
            total += *incr;
            let contrib = if let (Some(final_type), Some(final_eval)) = (
                key.finalizing_event_type,
                key.finalizing_event_eval.as_ref(),
            ) {
                // we have an attack
                metric.score(final_type, final_eval)
            } else if let Some(final_type) = key.finalizing_event_type {
                // no evaluation (e.g. OE, OS, F, etc)
                match final_type {
                    // direct point us
                    EventTypeEnum::OE => 1,
                    // direct point them
                    EventTypeEnum::OS | EventTypeEnum::F => {
                        if metric == Metric::Efficiency {
                            -1
                        } else {
                            0
                        }
                    }
                    // other neutral events
                    _ => 0,
                }
            } else if let Some(re_eval) = key.reception_eval.as_ref() {
                // fallback: use reception
                metric.score(EventTypeEnum::P, re_eval)
            } else {
                0
            };
            count += contrib * (*incr as i32);
        }
        if total > 0 {
            Some(((count as f64) / (total as f64) * 100.0, total, count))
        } else {
            None
        }
    }

    pub fn sideout_first_rally_count(
        &self,
        rotation: Option<u8>,
        reception_eval: Option<EvalEnum>,
        finalizing_event_type: Option<EventTypeEnum>,
        finalizing_event_eval: Option<EvalEnum>,
    ) -> Option<u32> {
        let mut total = 0;
        for (_key, incr) in self.first_rally.query(
            rotation,
            reception_eval,
            finalizing_event_type,
            finalizing_event_eval,
        ) {
            total += *incr;
        }
        if total > 0 {
            Some(total)
        } else {
            None
        }
    }

    pub fn sideout_first_rally_errors(
        &self,
        rotation: Option<u8>,
        reception_eval: Option<EvalEnum>,
        finalizing_event_type: Option<EventTypeEnum>,
        finalizing_event_eval: Option<EvalEnum>,
        error_type: Option<ErrorTypeEnum>,
    ) -> Option<u32> {
        let mut total_errors: u32 = 0;
        for (key, incr) in self.first_rally.query(
            rotation,
            reception_eval,
            finalizing_event_type,
            finalizing_event_eval,
        ) {
            if let Some(final_type) = key.finalizing_event_type {
                if let Some(err) = final_type.error_type(key.finalizing_event_eval) {
                    if error_type.is_none() || error_type == Some(err) {
                        total_errors += *incr;
                    }
                }
            }
        }
        if total_errors > 0 {
            Some(total_errors)
        } else {
            None
        }
    }

    /// Computes the percentage of a specific evaluation value within a given event type
    /// under the specified filters.
    ///
    /// The percentage is calculated as the ratio of events with the requested evaluation
    /// (`eval`) to the total number of matching events, expressed as a percentage.
    ///
    /// # Parameters
    /// - `event_type`: The type of event to evaluate (e.g. pass, attack, serve, ...).
    /// - `player`: Optional filter for a specific player id.
    /// - `phase`: Optional filter for a specific phase of play (side-out or break).
    /// - `rotation`: Optional filter for a specific team rotation (1–6).
    /// - `rotation`: Optional filter for a specific team rotation index (0–5).
    /// - `eval`: The evaluation value to measure (e.g. perfect pass, error, point, ...).
    ///
    /// # Returns
    /// - `Some((percentage, total, count))` if at least one event with the requested
    ///   evaluation was found:
    ///   - `percentage`: The proportion of `eval` events relative to total, as a percentage (`f64`).
    ///   - `total`: The total number of matching events (`u32`).
    ///   - `count`: The number of events that matched the requested evaluation (`i32`).
    /// - `None` if no events with the requested evaluation were found.
    /// ```
    pub fn event_percentage(
        &self,
        event_type: EventTypeEnum,
        player: Option<Uuid>,
        phase: Option<PhaseEnum>,
        rotation: Option<u8>,
        zone: Option<ZoneEnum>,
        eval: EvalEnum,
    ) -> Option<(f64, u32, i32)> {
        let mut count: i32 = 0;
        let mut total: u32 = 0;
        for (key, incr) in self.events.query(
            Some(event_type),
            phase,
            rotation,
            Some(player),
            Some(zone),
            None,
        ) {
            total += *incr;
            if key.eval.as_ref() == Some(&eval) {
                count += *incr as i32;
            }
        }
        (count > 0).then_some(((count as f64) / (total as f64) * 100.0, total, count))
    }
}
