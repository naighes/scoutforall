//! The bands of the figures by level and gender, and the grade of a value.
#[cfg(test)]
mod tests {
    use crate::benchmark::{Band, Measure, band, grade};
    use crate::shapes::enums::{EventTypeEnum, GenderEnum, TeamClassificationEnum};
    use GenderEnum::*;
    use Measure::*;
    use TeamClassificationEnum::*;

    const LEVELS: [TeamClassificationEnum; 6] = [
        TopInternational,
        HighNational,
        NationalMidLevel,
        NationalLowLevel,
        Regional,
        LocalDivision,
    ];
    const MEASURES: [Measure; 6] = [Reception, Attack, Serve, Block, SideOut, BreakPoint];

    fn pair(b: Band) -> (f64, f64) {
        (b.low, b.high)
    }

    /// The high national level is the middle half of the MEVZA League
    /// 2021/22 teams.
    #[test]
    fn high_national_is_the_mevza_league() {
        let men = |m| pair(band(HighNational, Men, m));
        assert_eq!(men(Reception), (30.0, 47.0));
        assert_eq!(men(Attack), (21.0, 37.0));
        assert_eq!(men(Serve), (5.0, 23.0));
        assert_eq!(men(Block), (-17.0, 14.0));
        assert_eq!(men(SideOut), (61.0, 73.0));
        assert_eq!(men(BreakPoint), (27.0, 39.0));
        let women = |m| pair(band(HighNational, Women, m));
        assert_eq!(women(Reception), (22.0, 44.0));
        assert_eq!(women(Attack), (14.0, 28.0));
        assert_eq!(women(Serve), (17.0, 41.0));
        assert_eq!(women(Block), (-15.0, 22.0));
        assert_eq!(women(SideOut), (53.0, 63.0));
        assert_eq!(women(BreakPoint), (37.0, 47.0));
    }

    /// Reception, attack and serve: each level below asks less, with a
    /// band as wide (it moves, it doesn't shrink).
    #[test]
    fn a_lower_level_asks_less() {
        for gender in [Men, Women] {
            for measure in [Reception, Attack, Serve] {
                let bands: Vec<Band> = LEVELS.iter().map(|l| band(*l, gender, measure)).collect();
                for pair in bands.windows(2) {
                    let [upper, lower] = pair else { continue };
                    assert!(
                        lower.low < upper.low && lower.high < upper.high,
                        "{gender:?} {measure:?}: {bands:?}"
                    );
                    assert!(
                        ((upper.high - upper.low) - (lower.high - lower.low)).abs() <= 1.0,
                        "{gender:?} {measure:?}: {bands:?}"
                    );
                }
            }
        }
    }

    /// Block, side out and break point: no table by level, the same band.
    #[test]
    fn the_same_band_at_every_level_without_a_table() {
        for gender in [Men, Women] {
            for measure in [Block, SideOut, BreakPoint] {
                let high = band(HighNational, gender, measure);
                for level in LEVELS {
                    assert_eq!(band(level, gender, measure), high, "{gender:?} {measure:?}");
                }
            }
        }
    }

    /// Every band goes up, and every value gets a grade from 0 to 1.
    #[test]
    fn every_band_is_well_formed() {
        for gender in [Men, Women] {
            for measure in MEASURES {
                for level in LEVELS {
                    let b = band(level, gender, measure);
                    assert!(b.low < b.high, "{level:?} {gender:?} {measure:?}");
                    for value in [-100.0, b.low, b.high, 100.0] {
                        let g = b.grade(value);
                        assert!((0.0..=1.0).contains(&g));
                    }
                }
            }
        }
    }

    /// Red half a band below, yellow in the middle, green half a band
    /// above, in between along the way.
    #[test]
    fn the_grade_of_a_value() {
        let b = band(HighNational, Men, Reception);
        assert_eq!(b.grade(21.5), 0.0);
        assert_eq!(b.grade(-100.0), 0.0);
        assert_eq!(b.grade(30.0), 0.25);
        assert_eq!(b.grade(38.5), 0.5);
        assert_eq!(b.grade(47.0), 0.75);
        assert_eq!(b.grade(55.5), 1.0);
        assert_eq!(b.grade(100.0), 1.0);
        // the higher the value, the better
        let grades: Vec<f64> = (-20..80).map(|v| b.grade(f64::from(v))).collect();
        assert!(grades.windows(2).all(|g| g[0] <= g[1]));
    }

    /// The same value is better for a lower level.
    #[test]
    fn the_same_value_by_level() {
        let at = |level| grade(Some(level), Some(Men), Attack, 20.0);
        assert_eq!(at(TopInternational), Some(0.0));
        assert!(at(HighNational) < at(Regional));
        assert_eq!(at(LocalDivision), Some(1.0));
    }

    /// No grade without the level or the gender.
    #[test]
    fn no_grade_without_level_or_gender() {
        assert_eq!(grade(None, Some(Men), Serve, 10.0), None);
        assert_eq!(grade(Some(Regional), None, Serve, 10.0), None);
        assert!(grade(Some(Regional), Some(Women), Serve, 10.0).is_some());
    }

    #[test]
    fn the_measure_of_each_fundamental() {
        use EventTypeEnum::*;
        let measures: Vec<Option<Measure>> = [P, A, S, B, D, F, OS]
            .into_iter()
            .map(Measure::of)
            .collect();
        assert_eq!(
            measures,
            vec![
                Some(Reception),
                Some(Attack),
                Some(Serve),
                Some(Block),
                None,
                None,
                None
            ]
        );
    }
}
