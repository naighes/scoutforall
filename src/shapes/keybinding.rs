use {
    crate::shapes::enums::ScreenActionEnum,
    crokey::*,
    serde::{Deserialize, Serialize},
    std::{
        collections::{hash_map, HashMap, HashSet},
        fmt,
    },
};

/// A mapping from key combinations to actions.
///
/// Several key combinations can go to the same action.
#[derive(Clone, Deserialize, Serialize)]
pub struct KeyBindings {
    #[serde(skip)]
    map: HashMap<KeyCombination, ScreenActionEnum>,
    #[serde(flatten)]
    default_bindings: HashMap<ScreenActionEnum, HashSet<KeyCombination>>,
}

impl Default for KeyBindings {
    fn default() -> Self {
        let mut bindings = Self {
            map: HashMap::default(),
            default_bindings: HashMap::new(),
        };
        bindings.set(ScreenActionEnum::Quit, key!(cmd - e));
        bindings.set(ScreenActionEnum::Quit, key!(ctrl - q));
        bindings.set(ScreenActionEnum::Quit, key!(q));
        bindings.set(ScreenActionEnum::Back, key!(esc));
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
        bindings.set(ScreenActionEnum::LanguageSettings, key!(s));
        bindings.set(ScreenActionEnum::KeybindingSettings, key!(b));
        bindings.set(ScreenActionEnum::OneLevelUp, key!(backspace));
        bindings.set(ScreenActionEnum::EnterDirectory, key!(space));
        bindings.set(ScreenActionEnum::ScrollDown, key!(down));
        bindings.set(ScreenActionEnum::ScrollUp, key!(up));
        bindings.set(ScreenActionEnum::ReportAnIssue, key!(i));
        bindings.set(ScreenActionEnum::Select, key!(enter));
        bindings.set(ScreenActionEnum::Reset, key!(r));
        bindings
    }
}

impl KeyBindings {
    pub fn empty() -> Self {
        Self {
            map: HashMap::default(),
            default_bindings: HashMap::new(),
        }
    }

    pub fn set<A: Into<ScreenActionEnum>>(&mut self, action: A, ck: KeyCombination) -> bool {
        self.default_bindings
            .entry(action.into())
            .or_default()
            .insert(ck)
    }

    fn set_to_map<A: Into<ScreenActionEnum>>(&mut self, action: A, ck: KeyCombination) {
        let action_enum = action.into();
        self.map.entry(ck).or_insert(action_enum);
    }

    pub fn remove<A: Into<ScreenActionEnum>>(&mut self, action: A, ck: KeyCombination) -> bool {
        if let Some(set) = self.default_bindings.get_mut(&action.into()) {
            set.remove(&ck)
        } else {
            false
        }
    }

    pub fn get(&self, key: KeyCombination) -> Option<&ScreenActionEnum> {
        self.map.get(&key)
    }

    /// return the key combination for the action matching the filter, choosing
    /// the one with the shortest Display representation.
    pub fn shortest_key_for(&self, action: &ScreenActionEnum) -> Option<(KeyCombination, String)> {
        let mut shortest: Option<(KeyCombination, String, ScreenActionEnum)> = None;
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

    pub fn keybindings_for(&self, action: &ScreenActionEnum) -> HashSet<KeyCombination> {
        self.default_bindings
            .get(action)
            .cloned()
            .unwrap_or_default()
    }

    /// build and return a map from actions to all the possible shortcuts
    pub fn reverse_map(&self) -> HashMap<ScreenActionEnum, HashSet<KeyCombination>> {
        self.default_bindings.clone()
    }

    pub fn slice(&self, actions: Vec<&ScreenActionEnum>) -> KeyBindings {
        let mut slice = KeyBindings {
            map: HashMap::new(),
            default_bindings: HashMap::new(),
        };
        for (action, cks) in &self.default_bindings {
            if actions.contains(&action) {
                cks.iter()
                    .for_each(|ck| slice.set_to_map(action.to_owned(), *ck));
            }
        }
        slice
    }
}

impl<'a> IntoIterator for &'a KeyBindings {
    type Item = (&'a KeyCombination, &'a ScreenActionEnum);
    type IntoIter = hash_map::Iter<'a, KeyCombination, ScreenActionEnum>;
    fn into_iter(self) -> Self::IntoIter {
        self.map.iter()
    }
}

impl fmt::Debug for KeyBindings {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut ds = f.debug_struct("KeyBindings");
        for (kc, action) in &self.map {
            ds.field(&kc.to_string(), &action);
        }
        ds.finish()
    }
}

#[test]
fn test_deserialize_keybindings() {
    #[derive(Deserialize)]
    struct Config {
        keybindings: KeyBindings,
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
