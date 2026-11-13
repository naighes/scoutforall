//! The pinned teams: kept in `config.json` along with the other settings,
//! at the top of the list, each group by name.

#[cfg(test)]
mod tests {
    use crate::{
        settings::PinnedTeams,
        shapes::{listed::Unreadable, team::TeamEntry},
    };
    use uuid::Uuid;

    fn team(name: &str) -> TeamEntry {
        TeamEntry {
            name: name.to_string(),
            id: Uuid::new_v4(),
            ..TeamEntry::default()
        }
    }

    fn names(teams: &[Result<TeamEntry, Unreadable>]) -> Vec<String> {
        teams
            .iter()
            .map(|t| match t {
                Ok(t) => t.name.clone(),
                Err(u) => u.id.clone(),
            })
            .collect()
    }

    #[test]
    fn the_pinned_teams_come_first_each_group_by_name() {
        let [zeta, alfa, beta, gamma] = ["Zeta", "alfa", "Beta", "gamma"].map(team);
        let pins = PinnedTeams::default()
            .with(zeta.id, true)
            .with(gamma.id, true);
        let unreadable = Unreadable {
            id: "broken".to_string(),
            reason: None,
        };
        let sorted = pins.sorted(vec![
            Ok(alfa),
            Ok(zeta),
            Err(unreadable),
            Ok(beta),
            Ok(gamma),
        ]);
        assert_eq!(names(&sorted), ["gamma", "Zeta", "alfa", "Beta", "broken"]);
    }

    #[test]
    fn without_pins_the_teams_are_by_name() {
        let sorted = PinnedTeams::default().sorted(["b", "C", "a"].map(|n| Ok(team(n))).into());
        assert_eq!(names(&sorted), ["a", "b", "C"]);
    }

    #[test]
    fn the_teams_with_the_same_name_are_by_id() {
        let named = |id: u128, name: &str| {
            Ok(TeamEntry {
                id: Uuid::from_u128(id),
                ..team(name)
            })
        };
        let sorted = PinnedTeams::default().sorted(vec![
            named(3, "ok merkur"),
            named(2, "OK MERKUR"),
            named(4, "Alfa"),
            named(1, "OK Merkur"),
        ]);
        let ids: Vec<u128> = sorted
            .iter()
            .filter_map(|t| t.as_ref().ok())
            .map(|t| t.id.as_u128())
            .collect();
        assert_eq!(ids, [4, 1, 2, 3]);
    }

    #[test]
    fn a_team_unpinned_leaves_the_pins() {
        let id = Uuid::new_v4();
        let pins = PinnedTeams::default().with(id, true);
        assert!(pins.is_pinned(&id));
        let pins = pins.with(id, false);
        assert!(!pins.is_pinned(&id));
        assert_eq!(pins, PinnedTeams::default());
    }

    #[test]
    fn the_pins_keep_the_other_keys_and_are_left_out_when_none() {
        let id = Uuid::new_v4();
        let read: PinnedTeams =
            serde_json::from_str(&format!(r#"{{"pinned_teams":["{id}"],"language":"it"}}"#))
                .unwrap();
        assert!(read.is_pinned(&id));
        assert_eq!(
            serde_json::to_string(&PinnedTeams::default()).unwrap(),
            "{}"
        );
        let written = serde_json::to_value(&read).unwrap();
        assert_eq!(written["pinned_teams"][0], id.to_string());
    }

    /// Something unexpected in `config.json` loses the pins it can't read,
    /// not the settings.
    #[test]
    fn unreadable_pins_are_dropped() {
        let id = Uuid::new_v4();
        for (json, expected) in [
            (format!(r#"{{"pinned_teams":["{id}","nope",3]}}"#), 1),
            (r#"{"pinned_teams":"nope"}"#.to_string(), 0),
            (r#"{"pinned_teams":null}"#.to_string(), 0),
            ("{}".to_string(), 0),
        ] {
            let read: PinnedTeams = serde_json::from_str(&json).unwrap();
            assert_eq!(read.pinned_teams.len(), expected, "{json}");
        }
    }
}
