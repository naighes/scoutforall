//! The keys of the scouting, shared by the apps with a keyboard (the TUI,
//! the desktop app): their defaults, the fixed ones, and the checks that
//! keep two shortcuts active at the same time from sharing a key. The keys
//! are in `config.json`, as the TUI always wrote them.

use {
    crokey::{
        crossterm::event::{KeyCode, KeyEvent, KeyModifiers},
        *,
    },
    scout4all_core::shapes::{
        enums::{EvalEnum, EventTypeEnum},
        technique::{ServeTypeEnum, SetTypeEnum},
    },
    serde::{Deserialize, Serialize},
    std::collections::{HashMap, HashSet, hash_map},
};

pub use crokey::{KeyCombination, KeyCombinationFormat, key};

/// Opens the settings, from home and while scouting.
pub const SETTINGS_KEY: KeyCombination = key!(f2);
/// Undoes the last scouting step: with shift, not to press it by mistake.
pub const UNDO_KEY: KeyCombination = key!(shift - u);
/// Leaves the scouting.
pub const BACK_KEY: KeyCombination = key!(esc);
/// Shows the statistics while scouting, and closes them.
pub const STATS_KEY: KeyCombination = key!(f3);

/// Move the cursor on the court where the ball went, while evaluating.
pub const ZONE_CURSOR_KEYS: [KeyCombination; 4] = [key!(left), key!(up), key!(down), key!(right)];
/// Chooses the zone under the cursor, or takes it back.
pub const ZONE_KEY: KeyCombination = key!(space);
/// Tells how many blocked, against our attack or on our block: the next
/// one each time, then none told again.
pub const BLOCKERS_KEY: KeyCombination = key!(tab);

/// Whether the key is one scouting keeps for itself all along (settings, undo,
/// back and statistics) or along with the evaluations (the court where the
/// ball went, how many blocked): it can't be assigned to anything.
pub fn is_reserved_key(ck: &KeyCombination) -> bool {
    [
        SETTINGS_KEY,
        UNDO_KEY,
        BACK_KEY,
        STATS_KEY,
        ZONE_KEY,
        BLOCKERS_KEY,
    ]
    .contains(ck)
        || ZONE_CURSOR_KEYS.contains(ck)
}

/// Whether the key is one scouting keeps for itself (settings, undo, back,
/// statistics and the digits choosing players, 0 the team for our fault): it
/// can't be assigned to events nor evaluations. Sets and serves can have
/// digits: they're chosen along with the evaluation, once the player is.
pub fn is_fixed_key(ck: &KeyCombination) -> bool {
    is_reserved_key(ck) || ck.as_letter().is_some_and(|c| c.is_ascii_digit())
}

/// Some terminals report shift along with the symbols it produces (e.g.
/// "shift-+" for "+"): the symbol alone is what users mean.
pub fn normalized(ck: KeyCombination) -> KeyCombination {
    match ck.codes {
        OneToThree::One(KeyCode::Char(c))
            if ck.modifiers == KeyModifiers::SHIFT && !c.is_alphabetic() =>
        {
            KeyCombination::one_key(KeyCode::Char(c), KeyModifiers::NONE)
        }
        _ => ck,
    }
}

/// The key combination of a key event, as it's looked up in the bindings.
pub fn combination_of(key: KeyEvent) -> KeyCombination {
    normalized(key.into())
}

/// A mapping from key combinations to actions.
///
/// Several key combinations can go to the same action.
#[derive(Clone, Deserialize, Serialize, Debug)]
pub struct KeyBindings<T>
where
    T: std::hash::Hash + Eq,
{
    #[serde(flatten)]
    default_bindings: HashMap<T, HashSet<KeyCombination>>,
}

/// A mapping from key combinations to actions for a specific screen.
#[derive(Debug)]
pub struct ActionsKeyBindings<T> {
    map: HashMap<KeyCombination, T>,
    combiner: Combiner,
}

impl<'a, T> IntoIterator for &'a ActionsKeyBindings<T> {
    type Item = (&'a KeyCombination, &'a T);
    type IntoIter = hash_map::Iter<'a, KeyCombination, T>;
    fn into_iter(self) -> Self::IntoIter {
        self.map.iter()
    }
}

impl<T> ActionsKeyBindings<T> {
    pub fn empty() -> Self {
        Self {
            combiner: Combiner::default(),
            map: HashMap::new(),
        }
    }

    pub fn from(slice: Vec<(&T, &HashSet<KeyCombination>)>) -> Self
    where
        T: Clone + std::hash::Hash + Eq,
    {
        let mut skb = Self::empty();
        for (action, cks) in slice {
            for ck in cks {
                skb.set(action.to_owned(), *ck);
            }
        }
        skb
    }

    pub fn transform(&mut self, key: KeyEvent) -> Option<KeyCombination> {
        self.combiner.transform(key).map(normalized)
    }

    fn set<A: Into<T>>(&mut self, action: A, ck: KeyCombination) {
        let action_enum = action.into();
        self.map.entry(ck).or_insert(action_enum);
    }
    pub fn get(&self, key: KeyCombination) -> Option<&T> {
        self.map.get(&key)
    }
}

impl<T> Clone for ActionsKeyBindings<T>
where
    T: Clone,
{
    fn clone(&self) -> Self {
        Self {
            map: self.map.clone(),
            combiner: Combiner::default(),
        }
    }
}

impl<T> KeyBindings<T>
where
    T: std::hash::Hash + Eq,
{
    /// no key for anything
    pub fn empty() -> Self {
        Self {
            default_bindings: HashMap::new(),
        }
    }

    pub fn set<A: Into<T>>(&mut self, action: A, ck: KeyCombination) -> bool {
        self.default_bindings
            .entry(action.into())
            .or_default()
            .insert(ck)
    }

    /// binds the action to that key only, in place of the one it had
    pub fn assign<A: Into<T>>(&mut self, action: A, ck: KeyCombination) {
        self.default_bindings
            .insert(action.into(), HashSet::from([ck]));
    }

    /// the one key bound to the action, the first in order if there are more
    pub fn key_for(&self, action: &T) -> Option<KeyCombination> {
        self.default_bindings
            .get(action)
            .and_then(|cks| cks.iter().min().copied())
    }

    /// the key bound to the action with the shortest Display
    /// representation, if any
    pub fn shortest_key_for(&self, action: &T) -> Option<KeyCombination> {
        self.default_bindings.get(action).and_then(|cks| {
            cks.iter()
                .min_by_key(|ck| (ck.to_string().len(), **ck))
                .copied()
        })
    }

    /// unbinds the fixed keys (see [`is_fixed_key`]) from every action
    pub fn without_fixed_keys(self) -> Self {
        self.without_keys(is_fixed_key)
    }

    /// unbinds the keys matching the filter from every action
    pub fn without_keys(mut self, filter: impl Fn(&KeyCombination) -> bool) -> Self {
        for cks in self.default_bindings.values_mut() {
            cks.retain(|ck| !filter(ck));
        }
        self
    }

    /// leaves every action of `defaults` with exactly one key: the first of
    /// its keys, or its default one when it has none and the default one is
    /// still free
    pub fn one_key_each(mut self, defaults: &Self) -> Self
    where
        T: Clone,
    {
        for cks in self.default_bindings.values_mut() {
            if let Some(first) = cks.iter().min().copied() {
                *cks = HashSet::from([first]);
            }
        }
        for action in defaults.default_bindings.keys() {
            if self.key_for(action).is_some() {
                continue;
            }
            if let Some(ck) = defaults.key_for(action)
                && self.action_for(&ck).is_none()
            {
                self.assign(action.clone(), ck);
            }
        }
        self
    }

    /// the action bound to the key combination, if any
    pub fn action_for(&self, ck: &KeyCombination) -> Option<&T> {
        self.default_bindings
            .iter()
            .find(|(_, cks)| cks.contains(ck))
            .map(|(action, _)| action)
    }

    #[cfg(test)]
    pub fn keybindings_for(&self, action: &T) -> HashSet<KeyCombination> {
        self.default_bindings
            .get(action)
            .cloned()
            .unwrap_or_default()
    }

    /// build and return a map from actions to all the possible shortcuts
    pub fn reverse_map(&self) -> HashMap<T, HashSet<KeyCombination>>
    where
        T: Clone + std::hash::Hash + Eq,
    {
        self.default_bindings.clone()
    }

    pub fn slice(&self, actions: Vec<&T>) -> ActionsKeyBindings<T>
    where
        T: Clone + std::hash::Hash + Eq,
    {
        ActionsKeyBindings::from(
            actions
                .iter()
                .filter_map(|a| self.default_bindings.get(a).map(|cks| (*a, cks)))
                .collect(),
        )
    }
}

/// What the scout tells while choosing the event, besides the event: the
/// inverted P1, a state kept until changed; how many receive their next
/// serve.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScoutingOptionEnum {
    InvertedP1,
    Reception,
}

impl ScoutingOptionEnum {
    pub const ALL: [ScoutingOptionEnum; 2] = [
        ScoutingOptionEnum::InvertedP1,
        ScoutingOptionEnum::Reception,
    ];
}

/// The keys the scout can change, as `config.json` has them: each kind on
/// its own (the events, the evaluations, the sets, the serves, the
/// options).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ScoutingKeys {
    #[serde(default)]
    pub scouting_keybindings: KeyBindings<EventTypeEnum>,
    #[serde(default)]
    pub eval_keybindings: KeyBindings<EvalEnum>,
    #[serde(default)]
    pub set_type_keybindings: KeyBindings<SetTypeEnum>,
    #[serde(default)]
    pub serve_type_keybindings: KeyBindings<ServeTypeEnum>,
    #[serde(default)]
    pub option_keybindings: KeyBindings<ScoutingOptionEnum>,
}

impl ScoutingKeys {
    /// One key for each shortcut, none of the fixed ones: fixed keys, and
    /// more keys for the same action, may come from older versions or
    /// hand-edited files. Sets and serves are chosen along with the
    /// evaluation: they lose the keys of the evaluations, and don't get the
    /// default ones taken by an evaluation (it could be bound to a letter
    /// before sets existed).
    pub fn normalized(self) -> ScoutingKeys {
        let eval_keybindings = self
            .eval_keybindings
            .without_fixed_keys()
            .one_key_each(&KeyBindings::default());
        let taken = |ck: &KeyCombination| eval_keybindings.action_for(ck).is_some();
        let reserved = |ck: &KeyCombination| is_reserved_key(ck) || taken(ck);
        ScoutingKeys {
            scouting_keybindings: self
                .scouting_keybindings
                .without_fixed_keys()
                .one_key_each(&KeyBindings::default()),
            set_type_keybindings: self
                .set_type_keybindings
                .without_keys(reserved)
                .one_key_each(&KeyBindings::default().without_keys(taken)),
            serve_type_keybindings: self
                .serve_type_keybindings
                .without_keys(reserved)
                .one_key_each(&KeyBindings::default().without_keys(taken)),
            eval_keybindings,
            option_keybindings: self.option_keybindings,
        }
    }

    /// The keys with `ck` bound to `action` only, in place of the one it
    /// had; or why it can't be.
    pub fn assigned<T: ScoutingShortcut>(
        &self,
        action: T,
        ck: KeyCombination,
    ) -> Result<ScoutingKeys, Conflict> {
        match conflict(self, action, &ck) {
            Some(conflict) => Err(conflict),
            None => {
                let mut bindings = T::bindings(self).clone();
                bindings.assign(action, ck);
                Ok(T::with_bindings(self, bindings))
            }
        }
    }
}

/// A shortcut of the scouting, of any kind.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum Shortcut {
    Event(EventTypeEnum),
    Eval(EvalEnum),
    Set(SetTypeEnum),
    Serve(ServeTypeEnum),
    Option(ScoutingOptionEnum),
}

/// Why a key can't be assigned to a shortcut.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum Conflict {
    /// one the scouting keeps for itself (see [`is_fixed_key`])
    Reserved,
    /// bound to another shortcut active at the same time
    Taken(Shortcut),
}

/// Why `ck` can't be bound to `action`, if it can't: it's reserved, or
/// bound to another shortcut of the same kind or of a kind active at the
/// same time.
pub fn conflict<T: ScoutingShortcut>(
    keys: &ScoutingKeys,
    action: T,
    ck: &KeyCombination,
) -> Option<Conflict> {
    let other = T::bindings(keys)
        .action_for(ck)
        .filter(|other| **other != action)
        .map(|other| other.shortcut())
        .or_else(|| T::rival(keys, ck));
    match (T::is_reserved(ck), other) {
        (true, _) => Some(Conflict::Reserved),
        (false, other) => other.map(Conflict::Taken),
    }
}

/// What users can bind while scouting: events, evaluations, sets, serves
/// and options, each with its own bindings.
pub trait ScoutingShortcut: Copy + Eq + std::hash::Hash + Send + Sync + 'static {
    fn bindings(keys: &ScoutingKeys) -> &KeyBindings<Self>;
    fn with_bindings(keys: &ScoutingKeys, bindings: KeyBindings<Self>) -> ScoutingKeys;
    fn shortcut(self) -> Shortcut;

    /// Whether the key can't be assigned: the fixed keys (see
    /// [`is_fixed_key`]).
    fn is_reserved(ck: &KeyCombination) -> bool {
        is_fixed_key(ck)
    }

    /// The shortcut of another kind bound to the key and active at the same
    /// time, if any (e.g. an evaluation and a set): the key can't be shared.
    fn rival(_keys: &ScoutingKeys, _ck: &KeyCombination) -> Option<Shortcut> {
        None
    }
}

/// The shortcut bound to the key, if any.
fn bound_to<T: ScoutingShortcut>(
    bindings: &KeyBindings<T>,
    ck: &KeyCombination,
) -> Option<Shortcut> {
    bindings.action_for(ck).map(|action| action.shortcut())
}

impl ScoutingShortcut for EventTypeEnum {
    fn bindings(keys: &ScoutingKeys) -> &KeyBindings<Self> {
        &keys.scouting_keybindings
    }
    fn with_bindings(keys: &ScoutingKeys, bindings: KeyBindings<Self>) -> ScoutingKeys {
        ScoutingKeys {
            scouting_keybindings: bindings,
            ..keys.clone()
        }
    }
    fn shortcut(self) -> Shortcut {
        Shortcut::Event(self)
    }
    /// the options are told while choosing the event
    fn rival(keys: &ScoutingKeys, ck: &KeyCombination) -> Option<Shortcut> {
        bound_to(&keys.option_keybindings, ck)
    }
}

impl ScoutingShortcut for ScoutingOptionEnum {
    fn bindings(keys: &ScoutingKeys) -> &KeyBindings<Self> {
        &keys.option_keybindings
    }
    fn with_bindings(keys: &ScoutingKeys, bindings: KeyBindings<Self>) -> ScoutingKeys {
        ScoutingKeys {
            option_keybindings: bindings,
            ..keys.clone()
        }
    }
    fn shortcut(self) -> Shortcut {
        Shortcut::Option(self)
    }
    fn rival(keys: &ScoutingKeys, ck: &KeyCombination) -> Option<Shortcut> {
        bound_to(&keys.scouting_keybindings, ck)
    }
}

impl ScoutingShortcut for EvalEnum {
    fn bindings(keys: &ScoutingKeys) -> &KeyBindings<Self> {
        &keys.eval_keybindings
    }
    fn with_bindings(keys: &ScoutingKeys, bindings: KeyBindings<Self>) -> ScoutingKeys {
        ScoutingKeys {
            eval_keybindings: bindings,
            ..keys.clone()
        }
    }
    fn shortcut(self) -> Shortcut {
        Shortcut::Eval(self)
    }
    /// the sets and serves are chosen along with the evaluation
    fn rival(keys: &ScoutingKeys, ck: &KeyCombination) -> Option<Shortcut> {
        bound_to(&keys.set_type_keybindings, ck)
            .or_else(|| bound_to(&keys.serve_type_keybindings, ck))
    }
}

impl ScoutingShortcut for SetTypeEnum {
    fn bindings(keys: &ScoutingKeys) -> &KeyBindings<Self> {
        &keys.set_type_keybindings
    }
    fn with_bindings(keys: &ScoutingKeys, bindings: KeyBindings<Self>) -> ScoutingKeys {
        ScoutingKeys {
            set_type_keybindings: bindings,
            ..keys.clone()
        }
    }
    fn shortcut(self) -> Shortcut {
        Shortcut::Set(self)
    }
    fn is_reserved(ck: &KeyCombination) -> bool {
        is_reserved_key(ck)
    }
    fn rival(keys: &ScoutingKeys, ck: &KeyCombination) -> Option<Shortcut> {
        bound_to(&keys.eval_keybindings, ck)
    }
}

impl ScoutingShortcut for ServeTypeEnum {
    fn bindings(keys: &ScoutingKeys) -> &KeyBindings<Self> {
        &keys.serve_type_keybindings
    }
    fn with_bindings(keys: &ScoutingKeys, bindings: KeyBindings<Self>) -> ScoutingKeys {
        ScoutingKeys {
            serve_type_keybindings: bindings,
            ..keys.clone()
        }
    }
    fn shortcut(self) -> Shortcut {
        Shortcut::Serve(self)
    }
    fn is_reserved(ck: &KeyCombination) -> bool {
        is_reserved_key(ck)
    }
    fn rival(keys: &ScoutingKeys, ck: &KeyCombination) -> Option<Shortcut> {
        bound_to(&keys.eval_keybindings, ck)
    }
}

impl Default for KeyBindings<EventTypeEnum> {
    fn default() -> Self {
        let mut bindings = Self {
            default_bindings: HashMap::new(),
        };
        bindings.set(EventTypeEnum::S, key!(s));
        bindings.set(EventTypeEnum::P, key!(p));
        bindings.set(EventTypeEnum::A, key!(a));
        bindings.set(EventTypeEnum::D, key!(d));
        bindings.set(EventTypeEnum::B, key!(b));
        bindings.set(EventTypeEnum::F, key!(f));
        bindings.set(EventTypeEnum::R, key!(r));
        bindings.set(EventTypeEnum::OE, key!(shift - e));
        bindings.set(EventTypeEnum::OS, key!(shift - s));
        bindings.set(EventTypeEnum::FB, key!(x));
        bindings.set(EventTypeEnum::OF, key!(shift - x));
        bindings.set(EventTypeEnum::CL, key!(shift - l));
        bindings.set(EventTypeEnum::LR, key!(l));
        bindings.set(EventTypeEnum::LF, key!(k));
        bindings.set(EventTypeEnum::CS, key!(shift - c));
        bindings
    }
}
impl Default for KeyBindings<ScoutingOptionEnum> {
    fn default() -> Self {
        let mut bindings = Self {
            default_bindings: HashMap::new(),
        };
        bindings.set(ScoutingOptionEnum::InvertedP1, key!(i));
        bindings.set(ScoutingOptionEnum::Reception, key!(c));
        bindings
    }
}
impl Default for KeyBindings<EvalEnum> {
    fn default() -> Self {
        let mut bindings = Self {
            default_bindings: HashMap::new(),
        };
        for eval in EvalEnum::ALL {
            let symbol = eval.to_string().chars().next().unwrap_or_default();
            bindings.set(
                eval,
                KeyCombination::one_key(KeyCode::Char(symbol), KeyModifiers::NONE),
            );
        }
        bindings
    }
}

/// Every set and serve is bound to its code, e.g. "a" for the high set.
fn bound_to_codes<T>(all: &[T]) -> KeyBindings<T>
where
    T: Copy + std::hash::Hash + Eq + std::fmt::Display,
{
    let mut bindings = KeyBindings {
        default_bindings: HashMap::new(),
    };
    for item in all {
        let code = item.to_string().chars().next().unwrap_or_default();
        bindings.set(
            *item,
            KeyCombination::one_key(KeyCode::Char(code.to_ascii_lowercase()), KeyModifiers::NONE),
        );
    }
    bindings
}
impl Default for KeyBindings<SetTypeEnum> {
    /// the double C (CC) on "d", as its first letter is the C's
    fn default() -> Self {
        let mut bindings = bound_to_codes(&SetTypeEnum::ALL);
        bindings.assign(SetTypeEnum::DoubleC, key!(d));
        bindings
    }
}
impl Default for KeyBindings<ServeTypeEnum> {
    fn default() -> Self {
        bound_to_codes(&ServeTypeEnum::ALL)
    }
}

#[test]
fn test_normalized_symbols() {
    use crokey::crossterm::event::{KeyCode::Char, KeyModifiers as M};
    let shift_plus = KeyCombination::one_key(Char('+'), M::SHIFT);
    assert_eq!(normalized(shift_plus), key!('+'));
    // shift still matters on letters and with other modifiers
    assert_eq!(normalized(key!(shift - e)), key!(shift - e));
    let ctrl_shift_plus = KeyCombination::one_key(Char('+'), M::SHIFT | M::CONTROL);
    assert_eq!(normalized(ctrl_shift_plus), ctrl_shift_plus);
}

#[test]
fn test_fixed_keys() {
    for ck in [
        key!(shift - u),
        key!(esc),
        key!(f2),
        key!(f3),
        key!('0'),
        key!('9'),
    ] {
        assert!(is_fixed_key(&ck), "{ck}");
    }
    for ck in [key!(a), key!(u), key!(f4), key!('+'), key!(ctrl - '1')] {
        assert!(!is_fixed_key(&ck), "{ck}");
    }
}

#[test]
fn test_without_fixed_keys_and_action_for() {
    let mut kb = KeyBindings::<EventTypeEnum>::default();
    kb.set(EventTypeEnum::A, key!(shift - u));
    kb.set(EventTypeEnum::A, key!('3'));
    let kb = kb.without_fixed_keys();
    assert_eq!(
        kb.keybindings_for(&EventTypeEnum::A),
        HashSet::from([key!(a)])
    );
    assert_eq!(kb.action_for(&key!(b)), Some(&EventTypeEnum::B));
    assert_eq!(kb.action_for(&key!(shift - u)), None);
}

#[test]
fn test_one_key_each() {
    let defaults = KeyBindings::<EventTypeEnum>::default();
    let mut kb = KeyBindings::<EventTypeEnum>::default();
    // more keys, as older versions allowed: only the first one is kept
    kb.set(EventTypeEnum::A, key!(x));
    // no key at all: the default one comes back
    kb.default_bindings.insert(EventTypeEnum::S, HashSet::new());
    // no key, and the default one is taken: nothing to do
    kb.default_bindings.insert(EventTypeEnum::P, HashSet::new());
    kb.assign(EventTypeEnum::D, key!(p));
    let kb = kb.one_key_each(&defaults);
    assert_eq!(
        kb.keybindings_for(&EventTypeEnum::A),
        HashSet::from([key!(a)])
    );
    assert_eq!(
        kb.keybindings_for(&EventTypeEnum::S),
        HashSet::from([key!(s)])
    );
    assert_eq!(kb.key_for(&EventTypeEnum::P), None);
    assert_eq!(kb.key_for(&EventTypeEnum::D), Some(key!(p)));
}

#[test]
fn test_assign_replaces_the_key() {
    let mut kb = KeyBindings::<EvalEnum>::default();
    kb.assign(EvalEnum::Perfect, key!(z));
    assert_eq!(
        kb.keybindings_for(&EvalEnum::Perfect),
        HashSet::from([key!(z)])
    );
}

#[test]
fn test_sets_on_keys_of_their_own() {
    let kb = KeyBindings::<SetTypeEnum>::default();
    assert_eq!(kb.key_for(&SetTypeEnum::C), Some(key!(c)));
    assert_eq!(kb.key_for(&SetTypeEnum::DoubleC), Some(key!(d)));
    let keys: HashSet<_> = SetTypeEnum::ALL
        .iter()
        .filter_map(|set| kb.key_for(set))
        .collect();
    assert_eq!(keys.len(), SetTypeEnum::ALL.len());
}

#[test]
fn test_without_keys() {
    let kb = KeyBindings::<SetTypeEnum>::default().without_keys(|ck| *ck == key!(a));
    assert_eq!(kb.key_for(&SetTypeEnum::High), None);
    assert_eq!(kb.key_for(&SetTypeEnum::Super), Some(key!(s)));
    // the digits are fixed for events and evaluations, not for sets
    let kb = KeyBindings::<SetTypeEnum>::default().without_keys(is_reserved_key);
    assert_eq!(kb.key_for(&SetTypeEnum::Pipe), Some(key!('0')));
    let kb = KeyBindings::<SetTypeEnum>::default().without_fixed_keys();
    assert_eq!(kb.key_for(&SetTypeEnum::Pipe), None);
}

#[test]
fn test_reserved_and_fixed_keys() {
    // the zone keys too: active along with the evaluations and serves
    for ck in [
        key!(shift - u),
        key!(esc),
        key!(f2),
        key!(space),
        key!(up),
        key!(left),
    ] {
        assert!(is_reserved_key(&ck), "{ck}");
    }
    for ck in [key!('0'), key!('9'), key!(a), key!(u)] {
        assert!(!is_reserved_key(&ck), "{ck}");
    }
}

#[test]
fn test_normalized_keys() {
    let mut keys = ScoutingKeys::default();
    // a fixed key, and an evaluation on the key of a set
    keys.scouting_keybindings
        .assign(EventTypeEnum::A, key!('3'));
    keys.eval_keybindings.assign(EvalEnum::Perfect, key!(q));
    let keys = keys.normalized();
    assert_eq!(
        keys.scouting_keybindings.key_for(&EventTypeEnum::A),
        Some(key!(a))
    );
    assert_eq!(
        keys.eval_keybindings.key_for(&EvalEnum::Perfect),
        Some(key!(q))
    );
    assert_eq!(keys.set_type_keybindings.key_for(&SetTypeEnum::Quick), None);
}

#[test]
fn test_assigned_and_conflicts() {
    let keys = ScoutingKeys::default();
    assert_eq!(
        keys.assigned(EventTypeEnum::A, key!(f2)).err(),
        Some(Conflict::Reserved)
    );
    assert_eq!(
        keys.assigned(EventTypeEnum::A, key!(b)).err(),
        Some(Conflict::Taken(Shortcut::Event(EventTypeEnum::B)))
    );
    // an option while choosing the event
    assert_eq!(
        keys.assigned(EventTypeEnum::A, key!(i)).err(),
        Some(Conflict::Taken(Shortcut::Option(
            ScoutingOptionEnum::InvertedP1
        )))
    );
    // a set along with the evaluations
    assert_eq!(
        keys.assigned(SetTypeEnum::High, key!('#')).err(),
        Some(Conflict::Taken(Shortcut::Eval(EvalEnum::Perfect)))
    );
    // a digit for a set, not for an event
    assert!(keys.assigned(SetTypeEnum::High, key!('7')).is_err());
    assert_eq!(
        keys.assigned(SetTypeEnum::High, key!(z))
            .ok()
            .and_then(|k| k.set_type_keybindings.key_for(&SetTypeEnum::High)),
        Some(key!(z))
    );
    // its own key again
    assert!(keys.assigned(EventTypeEnum::A, key!(a)).is_ok());
}
