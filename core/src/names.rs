//! A name of a player in little room (the court, the start of a set):
//! no guess about which word is the surname, the user may have written
//! "Alessandro Magrini" or "Magrini Alessandro". Each long word is cut to
//! its first letters with a dot, the short ones (particles, short names)
//! stay as they are.

/// The letters kept of a long word.
const KEPT: usize = 3;
/// A word this long, or shorter, stays whole: "Di", "Ana", "Anna".
const WHOLE: usize = 4;

/// The name with its long words cut: "Ale. Mag.", "Di Bal. Chr.".
pub fn abbreviated(name: &str) -> String {
    name.split_whitespace()
        .map(|word| match word.chars().count() > WHOLE {
            true => format!("{}.", word.chars().take(KEPT).collect::<String>()),
            false => word.to_string(),
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// The name as it is if it's at most `room` letters, else abbreviated.
pub fn fitted(name: &str, room: usize) -> String {
    let name = name.trim();
    match name.chars().count() <= room {
        true => name.to_string(),
        false => abbreviated(name),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_long_word_is_cut_whatever_comes_first() {
        assert_eq!(abbreviated("Alessandro Magrini"), "Ale. Mag.");
        assert_eq!(abbreviated("Magrini Alessandro"), "Mag. Ale.");
        assert_eq!(abbreviated("Di Balsamo Christian"), "Di Bal. Chr.");
        assert_eq!(abbreviated("Anna Rossi"), "Anna Ros.");
        assert_eq!(abbreviated("Petar VIŠIC"), "Pet. VIŠ.");
        assert_eq!(
            abbreviated("  Gabriel Henrique  SILVA FRANÇA "),
            "Gab. Hen. SIL. FRA."
        );
        assert_eq!(abbreviated("佐藤 花子"), "佐藤 花子");
        assert_eq!(abbreviated(""), "");
    }

    #[test]
    fn a_name_that_fits_stays_whole() {
        assert_eq!(fitted("Anna Rossi", 12), "Anna Rossi");
        assert_eq!(fitted("Alessandro Magrini", 12), "Ale. Mag.");
    }
}
