//! The team list shows "name (classification/gender, year)". Up to v0.4.3 a
//! team without classification or gender was listed as "name (/, 2025)".

#[cfg(test)]
mod tests {
    use crate::tests::ui::{TeamFixture, Ui};

    const TEAM: &[TeamFixture] = &[TeamFixture {
        name: "Volley Club",
        players: &[],
        opponents: &[],
    }];

    #[tokio::test]
    async fn team_without_classification_nor_gender_has_no_slash() {
        let mut ui = Ui::start(TEAM, &[]).await;
        let rows = ui.rows_containing(&["Volley Club"]);
        assert_eq!(rows.len(), 1, "{rows:?}");
        assert!(rows[0].contains("Volley Club (2025)"), "{rows:?}");
        assert!(!rows[0].contains('/'), "{rows:?}");
    }
}
