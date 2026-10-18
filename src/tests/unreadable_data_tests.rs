//! Damaged files: a team or a match that can't be loaded is listed as
//! unreadable, and doesn't hide the others. Up to v0.3.0 a single damaged team
//! hid all the teams, a damaged match all the matches of its team, and a match
//! whose sets didn't replay disappeared without a word.

#[cfg(test)]
mod tests {
    use crate::{
        localization::current_labels,
        tests::ui::{TeamFixture, Ui},
    };
    use crokey::crossterm::event::KeyCode;
    use std::{
        fs::OpenOptions,
        io::Write,
        path::{Path, PathBuf},
    };

    const TEAM: &[TeamFixture] = &[TeamFixture {
        name: "Volley Club",
        players: &["Anna", "Bea", "Carla", "Dora", "Elsa", "Fede", "Gina"],
        opponents: &["Cortona Volley", "Arezzo Volley", "Siena Volley"],
    }];

    const TEAMS: &[TeamFixture] = &[
        TeamFixture {
            name: "Volley Club",
            players: &[],
            opponents: &[],
        },
        TeamFixture {
            name: "Damaged Club",
            players: &[],
            opponents: &[],
        },
    ];

    fn folders(path: &Path) -> Vec<PathBuf> {
        std::fs::read_dir(path)
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.is_dir())
            .collect()
    }

    fn folder_name(path: &Path) -> String {
        path.file_name().unwrap().to_string_lossy().into_owned()
    }

    /// The folder of the team with that name.
    fn team_folder(base: &Path, name: &str) -> PathBuf {
        folders(base)
            .into_iter()
            .find(|t| {
                std::fs::read_to_string(t.join("team.json"))
                    .unwrap()
                    .contains(name)
            })
            .unwrap()
    }

    /// The folder of the match against that opponent, e.g. "cortona".
    fn match_folder(base: &Path, opponent: &str) -> PathBuf {
        folders(base)
            .iter()
            .flat_map(|t| folders(t))
            .find(|m| folder_name(m).contains(opponent))
            .unwrap()
    }

    fn append(path: &Path, content: &str) {
        let mut file = OpenOptions::new().append(true).open(path).unwrap();
        file.write_all(content.as_bytes()).unwrap();
    }

    /// Opens the match list of the team, its files changed by `edit`.
    async fn match_list(edit: impl FnOnce(&Path)) -> Ui {
        let mut ui = Ui::start_with_files(TEAM, edit).await;
        ui.type_keys(&[KeyCode::Enter, KeyCode::Char('m')]).await;
        assert!(
            ui.screen().contains(current_labels().match_list),
            "not on the match list: {}",
            ui.screen()
        );
        ui
    }

    /// Two readable matches, and the damaged one.
    fn assert_only_cortona_unreadable(ui: &mut Ui, cortona: &str) {
        let rows = ui.rows_containing(&["Arezzo", "Siena", cortona]);
        assert_eq!(rows.len(), 3, "{rows:?}");
        let unreadable: Vec<_> = rows
            .iter()
            .filter(|r| r.contains(current_labels().unreadable))
            .collect();
        assert_eq!(unreadable.len(), 1, "{rows:?}");
        assert!(unreadable[0].contains(cortona), "{rows:?}");
    }

    #[tokio::test]
    async fn damaged_team_does_not_hide_the_others() {
        let mut damaged = String::new();
        let mut ui = Ui::start_with_files(TEAMS, |base| {
            let folder = team_folder(base, "Damaged Club");
            std::fs::write(folder.join("team.json"), "{ \"name\": ").unwrap();
            damaged = folder_name(&folder);
        })
        .await;
        let rows = ui.rows_containing(&["Volley Club", &damaged]);
        assert_eq!(rows.len(), 2, "{rows:?}");
        assert!(
            rows.iter()
                .any(|r| r.contains(&damaged) && r.contains(current_labels().unreadable)),
            "{rows:?}"
        );
    }

    #[tokio::test]
    async fn damaged_match_descriptor() {
        let mut cortona = String::new();
        let mut ui = match_list(|base| {
            let folder = match_folder(base, "cortona");
            std::fs::write(folder.join("match.json"), "{ \"opponent\": ").unwrap();
            cortona = folder_name(&folder);
        })
        .await;
        assert_only_cortona_unreadable(&mut ui, &cortona);
    }

    /// Without the row the set would replay to something else.
    #[tokio::test]
    async fn damaged_event_row() {
        let mut cortona = String::new();
        let mut ui = match_list(|base| {
            let folder = match_folder(base, "cortona");
            append(&folder.join("set_1.csv"), "2025-09-01T18:00:00Z,OE,,,\n");
            append(&folder.join("set_1.csv"), "not,an,event\n");
            cortona = folder_name(&folder);
        })
        .await;
        assert_only_cortona_unreadable(&mut ui, &cortona);
    }

    #[tokio::test]
    async fn set_that_does_not_replay() {
        let mut cortona = String::new();
        let mut ui = match_list(|base| {
            let folder = match_folder(base, "cortona");
            // a player who isn't on court is substituted
            append(
                &folder.join("set_1.csv"),
                "2025-09-01T18:00:00Z,R,0c5a7b9e-1c1b-4d7e-9a43-2f6f2b1d1a11,,\
                 9d0f3c55-55c4-4a1b-8c43-0e8a8d9c2b22\n",
            );
            cortona = folder_name(&folder);
        })
        .await;
        assert_only_cortona_unreadable(&mut ui, &cortona);
    }

    /// A set edited by hand breaking the order of the events: their error
    /// gives us the serve, so they can't score an ace.
    #[tokio::test]
    async fn set_edited_by_hand_with_an_event_out_of_order() {
        let mut cortona = String::new();
        let mut ui = match_list(|base| {
            let folder = match_folder(base, "cortona");
            append(&folder.join("set_1.csv"), "2025-09-01T18:00:00Z,OE,,,\n");
            append(&folder.join("set_1.csv"), "2025-09-01T18:00:01Z,OS,,,\n");
            cortona = folder_name(&folder);
        })
        .await;
        assert_only_cortona_unreadable(&mut ui, &cortona);
    }

    /// A set edited by hand giving an evaluation to their error.
    #[tokio::test]
    async fn set_edited_by_hand_with_an_unexpected_evaluation() {
        let mut cortona = String::new();
        let mut ui = match_list(|base| {
            let folder = match_folder(base, "cortona");
            append(&folder.join("set_1.csv"), "2025-09-01T18:00:00Z,OE,,#,\n");
            cortona = folder_name(&folder);
        })
        .await;
        assert_only_cortona_unreadable(&mut ui, &cortona);
    }

    /// An unreadable match can only be exported, e.g. to send it and get it
    /// fixed.
    #[tokio::test]
    async fn unreadable_match_can_only_be_exported() {
        let mut ui = match_list(|base| {
            let folder = match_folder(base, "cortona");
            std::fs::write(folder.join("match.json"), "").unwrap();
        })
        .await;
        // unreadable matches are listed last
        ui.type_keys(&[KeyCode::Down, KeyCode::Down]).await;
        let selected = ui.selected().unwrap();
        assert!(selected.contains(current_labels().unreadable), "{selected}");
        let footer = ui.footer();
        assert!(footer.contains(current_labels().export), "{footer}");
        assert!(!footer.contains(current_labels().match_stats), "{footer}");
        assert!(!footer.contains(current_labels().print_report), "{footer}");
        // nothing to scout
        ui.press(KeyCode::Enter).await;
        assert!(ui.screen().contains(current_labels().match_list));
    }
}
