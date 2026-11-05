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

/// The names offered as `written` is written, as a search engine does:
/// the ones that begin with it first, then the ones with a word that does,
/// then the ones that have it inside; none for nothing written, nor the
/// one already written whole.
pub fn offered(names: &[String], written: &str) -> Vec<String> {
    let written = written.trim().to_lowercase();
    let rank = |name: &str| {
        let name = name.to_lowercase();
        match () {
            _ if name.starts_with(&written) => Some(0),
            _ if name.split_whitespace().any(|w| w.starts_with(&written)) => Some(1),
            _ if name.contains(&written) => Some(2),
            _ => None,
        }
    };
    let mut found: Vec<(u8, &String)> = match written.is_empty() {
        true => vec![],
        false => names
            .iter()
            .filter(|name| name.trim().to_lowercase() != written)
            .filter_map(|name| rank(name).map(|r| (r, name)))
            .collect(),
    };
    // stable: the same rank keeps the order given
    found.sort_by_key(|(r, _)| *r);
    found.into_iter().map(|(_, name)| name.clone()).collect()
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

    #[test]
    fn the_names_that_begin_as_written_come_first() {
        let names: Vec<String> = ["Club Arezzo", "Livorno", "Pallavolo Lucca", "Cortona Volley"]
            .map(String::from)
            .to_vec();
        assert_eq!(
            offered(&names, "l"),
            vec!["Livorno", "Pallavolo Lucca", "Club Arezzo", "Cortona Volley"]
        );
        assert_eq!(offered(&names, "LI"), vec!["Livorno"]);
        assert_eq!(offered(&names, "livorno"), Vec::<String>::new());
        assert_eq!(offered(&names, " "), Vec::<String>::new());
    }
}
