use {
    crate::shapes::enums::{EvalEnum, EventTypeEnum, ScreenActionEnum, WithDesc},
    crokey::{
        crossterm::event::{KeyCode, KeyEvent, KeyModifiers},
        *,
    },
    serde::{Deserialize, Serialize},
    std::collections::{HashMap, HashSet, hash_map},
};

static NAVIGATION_KEYBINDINGS: std::sync::LazyLock<KeyBindings<ScreenActionEnum>> =
    std::sync::LazyLock::new(KeyBindings::default);

/// The keys moving through the screens: unlike the scouting ones, they can't
/// be customized.
pub fn navigation_keybindings() -> &'static KeyBindings<ScreenActionEnum> {
    &NAVIGATION_KEYBINDINGS
}

/// Opens the settings, from home and while scouting.
pub const SETTINGS_KEY: KeyCombination = key!(f2);
/// Undoes the last scouting step.
pub const UNDO_KEY: KeyCombination = key!(u);
/// Leaves the scouting.
pub const BACK_KEY: KeyCombination = key!(esc);

/// Whether the key is one scouting keeps for itself (settings, undo, back and
/// the digits choosing players): it can't be assigned to events nor
/// evaluations.
pub fn is_fixed_key(ck: &KeyCombination) -> bool {
    [SETTINGS_KEY, UNDO_KEY, BACK_KEY].contains(ck)
        || ck.as_letter().is_some_and(|c| c.is_ascii_digit())
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
    pub fn set<A: Into<T>>(&mut self, action: A, ck: KeyCombination) -> bool {
        self.default_bindings
            .entry(action.into())
            .or_default()
            .insert(ck)
    }

    pub fn remove<A: Into<T>>(&mut self, action: A, ck: KeyCombination) -> bool {
        if let Some(set) = self.default_bindings.get_mut(&action.into()) {
            set.remove(&ck)
        } else {
            false
        }
    }

    /// return the key combination for the action matching the filter, choosing
    /// the one with the shortest Display representation.
    pub fn shortest_key_for(&self, action: &T) -> Option<(KeyCombination, String)>
    where
        T: Clone + std::hash::Hash + Eq + WithDesc<T>,
    {
        let mut shortest: Option<(KeyCombination, String, T)> = None;
        if let Some(cks) = self.default_bindings.get(action) {
            for ck in cks {
                let s = ck.to_string();
                match &shortest {
                    Some(previous) if previous.1.len() < s.len() => {}
                    _ => {
                        shortest = Some((*ck, s, action.to_owned()));
                    }
                }
            }
            shortest.map(|o| (o.0, o.2.with_desc().1))
        } else {
            None
        }
    }

    /// unbinds the fixed keys (see [`is_fixed_key`]) from every action
    pub fn without_fixed_keys(mut self) -> Self {
        for cks in self.default_bindings.values_mut() {
            cks.retain(|ck| !is_fixed_key(ck));
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

impl Default for KeyBindings<ScreenActionEnum> {
    fn default() -> Self {
        let mut bindings = Self {
            default_bindings: HashMap::new(),
        };
        bindings.set(ScreenActionEnum::Quit, key!(cmd - e));
        bindings.set(ScreenActionEnum::Quit, key!(ctrl - q));
        bindings.set(ScreenActionEnum::Quit, key!(q));
        bindings.set(ScreenActionEnum::Back, BACK_KEY);
        bindings.set(ScreenActionEnum::Edit, key!(e));
        bindings.set(ScreenActionEnum::Delete, key!(d));
        bindings.set(ScreenActionEnum::EditPlayer, key!(enter));
        bindings.set(ScreenActionEnum::EditTeam, key!(e));
        bindings.set(ScreenActionEnum::Confirm, key!(enter));
        bindings.set(ScreenActionEnum::RemovePlayer, key!(r));
        bindings.set(ScreenActionEnum::Export, key!(s));
        bindings.set(ScreenActionEnum::Import, key!(i));
        bindings.set(ScreenActionEnum::New, key!(n));
        bindings.set(ScreenActionEnum::NewPlayer, key!(n));
        bindings.set(ScreenActionEnum::MatchList, key!(m));
        bindings.set(ScreenActionEnum::MatchStats, key!(space));
        bindings.set(ScreenActionEnum::PrintReport, key!(p));
        bindings.set(ScreenActionEnum::Next, key!(tab));
        bindings.set(ScreenActionEnum::Next, key!(right));
        bindings.set(ScreenActionEnum::Down, key!(down));
        bindings.set(ScreenActionEnum::Previous, key!(shift - backtab));
        bindings.set(ScreenActionEnum::Previous, key!(left));
        bindings.set(ScreenActionEnum::Up, key!(up));
        bindings.set(ScreenActionEnum::Settings, SETTINGS_KEY);
        bindings.set(ScreenActionEnum::OneLevelUp, key!(backspace));
        bindings.set(ScreenActionEnum::EnterDirectory, key!(space));
        bindings.set(ScreenActionEnum::ScrollDown, key!(down));
        bindings.set(ScreenActionEnum::ScrollUp, key!(up));
        bindings.set(ScreenActionEnum::ReportAnIssue, key!(i));
        bindings.set(ScreenActionEnum::Select, key!(enter));
        bindings.set(ScreenActionEnum::Reset, key!(r));
        bindings.set(ScreenActionEnum::Undo, UNDO_KEY);
        bindings
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
        bindings.set(EventTypeEnum::CL, key!(shift - l));
        bindings.set(EventTypeEnum::CS, key!(shift - c));
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

#[test]
fn test_deserialize_keybindings() {
    #[derive(Deserialize)]
    struct Config {
        keybindings: KeyBindings<ScreenActionEnum>,
    }
    let json = r#"
    {
        "keybindings": {
            "previous": ["shift-tab"],
            "quit": ["q","ctrl-q","cmd-e"]
        }
    }
    "#;
    let conf = serde_json::from_str::<Config>(json).unwrap();
    assert_eq!(
        conf.keybindings.shortest_key_for(&ScreenActionEnum::Back),
        None,
    );
    assert_eq!(
        conf.keybindings.shortest_key_for(&ScreenActionEnum::Quit),
        Some((key!(q), "quit".into()))
    );
    assert_eq!(
        conf.keybindings
            .shortest_key_for(&ScreenActionEnum::Previous),
        Some((key!(shift - tab), "previous".into()))
    );
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
    for ck in [key!(u), key!(esc), key!(f2), key!('0'), key!('9')] {
        assert!(is_fixed_key(&ck), "{ck}");
    }
    for ck in [
        key!(a),
        key!(shift - u),
        key!(f3),
        key!('+'),
        key!(ctrl - '1'),
    ] {
        assert!(!is_fixed_key(&ck), "{ck}");
    }
}

#[test]
fn test_without_fixed_keys_and_action_for() {
    let mut kb = KeyBindings::<EventTypeEnum>::default();
    kb.set(EventTypeEnum::A, key!(u));
    kb.set(EventTypeEnum::A, key!('3'));
    let kb = kb.without_fixed_keys();
    assert_eq!(
        kb.keybindings_for(&EventTypeEnum::A),
        HashSet::from([key!(a)])
    );
    assert_eq!(kb.action_for(&key!(b)), Some(&EventTypeEnum::B));
    assert_eq!(kb.action_for(&key!(u)), None);
}
