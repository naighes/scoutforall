//! The statistics of a match.
//!
//! The statistics of a set are collected while its events are replayed
//! (`Snapshot::add_event`), as counters of the events keyed by their context
//! (phase, rotation, player, ...); the ones of a match merge the ones of its
//! sets. They're never saved: they're computed again each time a match is
//! loaded, so changing how they're counted needs no data migration.
//!
//! Every query takes a [`StatsFilter`] narrowing it to a phase, a rotation
//! (the index of the setter's position, 0 to 5) and a player.
use crate::shapes::enums::{
    ErrorTypeEnum, EvalEnum, EventTypeEnum, PhaseEnum, TeamSideEnum, ZoneEnum,
};
use std::collections::HashMap;
use std::hash::Hash;
use uuid::Uuid;

/// How the evaluations of a fundamental are scored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Metric {
    /// The share of the good evaluations (0% to 100%).
    Positive,
    /// The good evaluations minus the bad ones, over all of them
    /// (-100% to 100%).
    Efficiency,
}

impl Metric {
    /// +1 for a good evaluation, -1 for a bad one, 0 for a neutral one.
    fn score(&self, event_type: EventTypeEnum, eval: EvalEnum) -> i32 {
        use EvalEnum::*;
        use EventTypeEnum::*;
        let good = match (event_type, self) {
            // an ace, a serve putting them in trouble, or their reception
            // coming straight back (`/`)
            (S, _) => matches!(eval, Perfect | Positive | Over),
            // a kill; positiveness also counts an attack they couldn't
            // counter (`+`, we play the ball again) and an attack blocked
            // but covered (`!`, we play the ball again)
            (A, Metric::Positive) => matches!(eval, Perfect | Positive | Exclamative),
            (A, Metric::Efficiency) => eval == Perfect,
            // a perfect or good reception, dig or block (touch)
            (P | D | B, _) => matches!(eval, Perfect | Positive),
            _ => false,
        };
        let bad = match (event_type, self) {
            (_, Metric::Positive) => false,
            // a serve error (`/` is good for the serve)
            (S, Metric::Efficiency) => eval == Error,
            // an error, or the ball given to them: an attack blocked, a
            // block invasion, an overpass in reception or defense
            (P | A | D | B, Metric::Efficiency) => matches!(eval, Error | Over),
            _ => false,
        };
        match (good, bad) {
            (true, _) => 1,
            (_, true) => -1,
            _ => 0,
        }
    }
}

/// Narrows the statistics to a phase, a rotation and a player: `None` keeps
/// them all.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StatsFilter {
    pub phase: Option<PhaseEnum>,
    pub rotation: Option<u8>,
    pub player: Option<Uuid>,
}

impl StatsFilter {
    pub fn new(phase: Option<PhaseEnum>, rotation: Option<u8>, player: Option<Uuid>) -> Self {
        StatsFilter {
            phase,
            rotation,
            player,
        }
    }

    /// Whether an event with this context passes the filter: an event with
    /// no player (e.g. an opponent error) doesn't pass a player filter.
    fn matches(&self, phase: PhaseEnum, rotation: u8, player: Option<Uuid>) -> bool {
        self.phase.is_none_or(|p| p == phase)
            && self.rotation.is_none_or(|r| r == rotation)
            && self.player.is_none_or(|p| player == Some(p))
    }
}

/// How many times each key occurred.
#[derive(Debug, Clone)]
pub struct Counter<K>(HashMap<K, u32>);

impl<K> Default for Counter<K> {
    fn default() -> Self {
        Counter(HashMap::new())
    }
}

impl<K: Clone + Eq + Hash> Counter<K> {
    pub fn add(&mut self, key: K) {
        *self.0.entry(key).or_insert(0) += 1;
    }

    pub fn merge(&mut self, other: &Counter<K>) {
        for (k, v) in &other.0 {
            *self.0.entry(k.clone()).or_insert(0) += v;
        }
    }

    /// The keys passing the filter, with their occurrences.
    pub fn iter<'a, F>(&'a self, filter: F) -> impl Iterator<Item = (&'a K, u32)>
    where
        F: Fn(&K) -> bool + 'a,
    {
        self.0
            .iter()
            .filter(move |(k, _)| filter(k))
            .map(|(k, v)| (k, *v))
    }

    /// The occurrences of the keys passing the filter.
    pub fn count<F: Fn(&K) -> bool>(&self, filter: F) -> u32 {
        self.0
            .iter()
            .filter(|(k, _)| filter(k))
            .map(|(_, v)| v)
            .sum()
    }
}

/// An event of the rally: every event but the substitutions and the libero
/// and setter changes.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EventKey {
    pub event_type: EventTypeEnum,
    pub phase: PhaseEnum,
    pub rotation: u8,
    /// `None` for the opponent events (`OS`, `OE`, `OF`)
    pub player: Option<Uuid>,
    /// the zone of an attack, when the role of the attacker tells it
    pub zone: Option<ZoneEnum>,
    /// `None` for the events with no evaluation (`F`, `OS`, `OE`, `FB`,
    /// `OF`)
    pub eval: Option<EvalEnum>,
}

/// An attack built by us (see `Snapshot::is_attack_setup`), with the zone
/// told by the role of the attacker.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AttackKey {
    pub phase: PhaseEnum,
    pub rotation: u8,
    pub player: Uuid,
    pub zone: ZoneEnum,
    pub eval: EvalEnum,
    /// the event before the attack: the reception, dig, serve, attack or
    /// block the attack was built on, or their freeball
    pub prev_type: EventTypeEnum,
    /// `None` after their freeball (`OF`)
    pub prev_eval: Option<EvalEnum>,
}

impl AttackKey {
    /// A counter-attack: an attack built on a dig, on their freeball, on
    /// their reception coming straight back (serve `/`), or on our attack or
    /// block they couldn't counter (attack `+` or `!`, block `+`), i.e.
    /// everything but the attack built on our reception (side out on first
    /// rally).
    pub fn is_counter_attack(&self) -> bool {
        matches!(
            self.prev_type,
            EventTypeEnum::S
                | EventTypeEnum::D
                | EventTypeEnum::OF
                | EventTypeEnum::A
                | EventTypeEnum::B
        )
    }

    /// An attack on their freeball.
    pub fn is_freeball_attack(&self) -> bool {
        self.prev_type == EventTypeEnum::OF
    }
}

/// A rally, with the phase and rotation it was played in and its winner.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RallyKey {
    pub phase: PhaseEnum,
    pub rotation: u8,
    pub winner: TeamSideEnum,
}

/// Their freeball (`OF`), with the phase and rotation it was played in and
/// whether we exploited it (see `Snapshot::record_freeball`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FreeballKey {
    pub phase: PhaseEnum,
    pub rotation: u8,
    pub exploited: bool,
}

/// The first rally of a side out, from their serve to the end of our first
/// attack (see `Snapshot::record_first_rally`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FirstRallyKey {
    pub rotation: u8,
    /// `None` when the rally ended before the reception (their serve
    /// error, their ace, our fault)
    pub reception_eval: Option<EvalEnum>,
    /// the event ending the first rally
    pub ending: EventTypeEnum,
    pub ending_eval: Option<EvalEnum>,
}

impl FirstRallyKey {
    fn winner(&self) -> Option<TeamSideEnum> {
        self.ending.rally_winner(self.ending_eval)
    }
}

/// The first rallies of the side outs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FirstRallySummary {
    pub total: u32,
    /// the rallies we won: their serve errors, our kills, their errors
    pub won: u32,
    /// the rallies we lost: their aces, our errors and faults
    pub lost: u32,
    /// our first attacks being blocked
    pub blocked: u32,
    pub errors: u32,
    pub unforced_errors: u32,
}

impl FirstRallySummary {
    pub fn won_percentage(&self) -> f64 {
        percentage(self.won as i32, self.total)
    }

    /// The rallies won minus the ones lost, over all of them.
    pub fn efficiency(&self) -> f64 {
        percentage(self.won as i32 - self.lost as i32, self.total)
    }
}

/// The attacks built by us, in a phase of the game (see
/// `Stats::attack_summary`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AttackSummary {
    pub total: u32,
    /// out or in the net
    pub errors: u32,
    pub blocked: u32,
    pub kills: u32,
}

impl AttackSummary {
    pub fn kill_percentage(&self) -> f64 {
        percentage(self.kills as i32, self.total)
    }

    /// The kills minus the errors and the attacks blocked, over all of them
    /// (as `Metric::Efficiency`).
    pub fn efficiency(&self) -> f64 {
        percentage(
            self.kills as i32 - self.errors as i32 - self.blocked as i32,
            self.total,
        )
    }
}

fn percentage(count: i32, total: u32) -> f64 {
    count as f64 / total as f64 * 100.0
}

#[derive(Debug, Clone, Default)]
pub struct Stats {
    pub events: Counter<EventKey>,
    pub attacks: Counter<AttackKey>,
    pub rallies: Counter<RallyKey>,
    pub first_rallies: Counter<FirstRallyKey>,
    pub freeballs: Counter<FreeballKey>,
}

impl Stats {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn merge(&mut self, other: &Stats) {
        self.events.merge(&other.events);
        self.attacks.merge(&other.attacks);
        self.rallies.merge(&other.rallies);
        self.first_rallies.merge(&other.first_rallies);
        self.freeballs.merge(&other.freeballs);
    }

    fn events<'a>(
        &'a self,
        event_type: EventTypeEnum,
        filter: &'a StatsFilter,
    ) -> impl Iterator<Item = (&'a EventKey, u32)> {
        self.events.iter(move |k| {
            k.event_type == event_type && filter.matches(k.phase, k.rotation, k.player)
        })
    }

    /// The events of this type (with this evaluation, when given).
    pub fn event_count(
        &self,
        event_type: EventTypeEnum,
        eval: Option<EvalEnum>,
        filter: &StatsFilter,
    ) -> u32 {
        self.events(event_type, filter)
            .filter(|(k, _)| eval.is_none_or(|e| k.eval == Some(e)))
            .map(|(_, v)| v)
            .sum()
    }

    /// The share of the events of this type having this evaluation, or
    /// `None` when there are no events of this type.
    pub fn event_percentage(
        &self,
        event_type: EventTypeEnum,
        eval: EvalEnum,
        filter: &StatsFilter,
    ) -> Option<f64> {
        let total = self.event_count(event_type, None, filter);
        (total > 0).then(|| {
            percentage(
                self.event_count(event_type, Some(eval), filter) as i32,
                total,
            )
        })
    }

    /// The metric of the events of this type, or `None` when there are no
    /// events of this type.
    pub fn metric_percentage(
        &self,
        event_type: EventTypeEnum,
        metric: Metric,
        filter: &StatsFilter,
    ) -> Option<f64> {
        let (score, total) = self
            .events(event_type, filter)
            .filter_map(|(k, v)| k.eval.map(|eval| (metric.score(event_type, eval), v)))
            .fold((0, 0), |(score, total), (s, v)| {
                (score + s * v as i32, total + v)
            });
        (total > 0).then(|| percentage(score, total))
    }

    /// The errors made with this type of event (of this type of error, when
    /// given): see `EventTypeEnum::error_type`. A fault is an unforced error.
    pub fn errors(
        &self,
        event_type: EventTypeEnum,
        error_type: Option<ErrorTypeEnum>,
        filter: &StatsFilter,
    ) -> u32 {
        self.events(event_type, filter)
            .filter(|(k, _)| match k.event_type.error_type(k.eval) {
                Some(et) => error_type.is_none_or(|e| e == et),
                None => false,
            })
            .map(|(_, v)| v)
            .sum()
    }

    /// The errors made with every type of event, faults included.
    pub fn total_errors(&self, error_type: Option<ErrorTypeEnum>, filter: &StatsFilter) -> u32 {
        EventTypeEnum::ALL
            .iter()
            .map(|et| self.errors(*et, error_type, filter))
            .sum()
    }

    /// The points we scored with a serve, an attack or a block (aces, kills,
    /// block points): the points they gave away (`OE`) aren't ours.
    pub fn points(&self, filter: &StatsFilter) -> u32 {
        EventTypeEnum::ALL
            .iter()
            .filter(|et| et.provides_direct_points())
            .map(|et| self.event_count(*et, Some(EvalEnum::Perfect), filter))
            .sum()
    }

    /// The points scored minus the errors made, or `None` when there are
    /// neither.
    pub fn won_minus_lost(&self, filter: &StatsFilter) -> Option<i32> {
        let points = self.points(filter);
        let errors = self.total_errors(None, filter);
        (points + errors > 0).then_some(points as i32 - errors as i32)
    }

    /// How many rallies it takes us to win a point: in break, the serves
    /// per break point; in side out, the receptions per side out. Returns
    /// `(rallies per point, points, rallies)`, or `None` with no points won.
    pub fn phases_per_point(&self, filter: &StatsFilter) -> Option<(f64, u32, u32)> {
        let matches = |k: &RallyKey| filter.matches(k.phase, k.rotation, None);
        let rallies = self.rallies.count(matches);
        let won = self
            .rallies
            .count(|k| matches(k) && k.winner == TeamSideEnum::Us);
        (won > 0).then(|| (rallies as f64 / won as f64, won, rallies))
    }

    /// The rallies we won: in break, the break points over our serves; in
    /// side out, the side outs over their serves. Returns `(percentage, won,
    /// rallies)`, or `None` with no rallies.
    pub fn rallies_won(&self, filter: &StatsFilter) -> Option<(f64, u32, u32)> {
        let matches = |k: &RallyKey| filter.matches(k.phase, k.rotation, None);
        let rallies = self.rallies.count(matches);
        let won = self
            .rallies
            .count(|k| matches(k) && k.winner == TeamSideEnum::Us);
        (rallies > 0).then(|| (percentage(won as i32, rallies), won, rallies))
    }

    fn attacks<'a>(
        &'a self,
        filter: &'a StatsFilter,
    ) -> impl Iterator<Item = (&'a AttackKey, u32)> {
        self.attacks
            .iter(move |k| filter.matches(k.phase, k.rotation, Some(k.player)))
    }

    /// The attacks built by us on our reception (`counter_attack` false) or
    /// in counter-attack (see `AttackKey::is_counter_attack`), or `None` when
    /// there are none. Only the attacks with a zone are counted: see
    /// `Snapshot::record_stats`.
    pub fn attack_summary(
        &self,
        counter_attack: bool,
        filter: &StatsFilter,
    ) -> Option<AttackSummary> {
        Self::summarize(
            self.attacks(filter)
                .filter(|(k, _)| k.is_counter_attack() == counter_attack),
        )
    }

    /// The attacks built by us from this zone: all of them, or only the
    /// ones on our reception (`counter_attack` false) or in counter-attack;
    /// `None` when there are none.
    pub fn zone_attacks(
        &self,
        zone: ZoneEnum,
        counter_attack: Option<bool>,
        filter: &StatsFilter,
    ) -> Option<AttackSummary> {
        Self::summarize(self.attacks(filter).filter(|(k, _)| {
            k.zone == zone && counter_attack.is_none_or(|c| k.is_counter_attack() == c)
        }))
    }

    fn summarize<'a>(attacks: impl Iterator<Item = (&'a AttackKey, u32)>) -> Option<AttackSummary> {
        let summary = attacks.fold(AttackSummary::default(), |s, (k, v)| {
            let (errors, blocked, kills) = match k.eval {
                EvalEnum::Error => (v, 0, 0),
                EvalEnum::Over => (0, v, 0),
                EvalEnum::Perfect => (0, 0, v),
                _ => (0, 0, 0),
            };
            AttackSummary {
                total: s.total + v,
                errors: s.errors + errors,
                blocked: s.blocked + blocked,
                kills: s.kills + kills,
            }
        });
        (summary.total > 0).then_some(summary)
    }

    /// The share of the counter-attacks from this zone that were kills:
    /// `(percentage, counter-attacks, kills)`, or `None` with no
    /// counter-attacks from the zone.
    pub fn counter_attack_conversion(
        &self,
        zone: ZoneEnum,
        filter: &StatsFilter,
    ) -> Option<(f64, u32, u32)> {
        let (total, kills) = self
            .attacks(filter)
            .filter(|(k, _)| k.zone == zone && k.is_counter_attack())
            .fold((0, 0), |(total, kills), (k, v)| {
                (
                    total + v,
                    kills + if k.eval == EvalEnum::Perfect { v } else { 0 },
                )
            });
        (total > 0).then(|| (percentage(kills as i32, total), total, kills))
    }

    /// The distribution of the attacks: the share of the attacks set to
    /// this zone, and the share of those that were kills. `None` when no
    /// attack was set to the zone.
    pub fn zone_distribution(&self, zone: ZoneEnum, filter: &StatsFilter) -> Option<(f64, f64)> {
        let (total, in_zone, kills) =
            self.attacks(filter)
                .fold((0, 0, 0), |(total, in_zone, kills), (k, v)| {
                    let here = if k.zone == zone { v } else { 0 };
                    let kill = if k.eval == EvalEnum::Perfect { here } else { 0 };
                    (total + v, in_zone + here, kills + kill)
                });
        (in_zone > 0).then(|| {
            (
                percentage(in_zone as i32, total),
                percentage(kills as i32, in_zone),
            )
        })
    }

    /// The first rallies of the side outs (after a reception with this
    /// evaluation, when given), or `None` when there are none. The phase is
    /// always side out, and they have no player: a player filter leaves none.
    pub fn first_rallies(
        &self,
        reception_eval: Option<EvalEnum>,
        filter: &StatsFilter,
    ) -> Option<FirstRallySummary> {
        let mut summary = FirstRallySummary::default();
        for (k, v) in self.first_rallies.iter(|k| {
            filter.matches(PhaseEnum::SideOut, k.rotation, None)
                && reception_eval.is_none_or(|e| k.reception_eval == Some(e))
        }) {
            summary.total += v;
            match k.winner() {
                Some(TeamSideEnum::Us) => summary.won += v,
                Some(TeamSideEnum::Them) => summary.lost += v,
                None => {}
            }
            if (k.ending, k.ending_eval) == (EventTypeEnum::A, Some(EvalEnum::Over)) {
                summary.blocked += v;
            }
            match k.ending.error_type(k.ending_eval) {
                Some(ErrorTypeEnum::Unforced) => {
                    summary.errors += v;
                    summary.unforced_errors += v;
                }
                Some(ErrorTypeEnum::Forced) => summary.errors += v,
                None => {}
            }
        }
        (summary.total > 0).then_some(summary)
    }

    /// Their freeballs we exploited: `(percentage, freeballs, exploited)`,
    /// or `None` with no freeballs. They have no player: a player filter
    /// leaves none.
    pub fn freeball_exploitation(&self, filter: &StatsFilter) -> Option<(f64, u32, u32)> {
        let matches = |k: &FreeballKey| filter.matches(k.phase, k.rotation, None);
        let total = self.freeballs.count(matches);
        let exploited = self.freeballs.count(|k| matches(k) && k.exploited);
        (total > 0).then(|| (percentage(exploited as i32, total), total, exploited))
    }
}
