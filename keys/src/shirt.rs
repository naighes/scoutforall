//! The player chosen by the shirt number (issue #69): the digits typed so
//! far keep the players whose number starts with them; one left is chosen
//! at once, more wait for another digit or for Enter, which takes the one
//! with exactly that number (3 among 3, 30 and 31).

/// What the digits typed so far choose among the numbers offered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pick {
    /// the player at this place among the numbers
    Chosen(usize),
    /// more than one starts with them: another digit, or Enter
    Waiting,
    /// no number starts with them
    Nobody,
}

/// Whether the number starts with the digits typed: none typed, none.
pub fn matches(number: u8, typed: &str) -> bool {
    !typed.is_empty() && number.to_string().starts_with(typed)
}

/// After a digit: the one player whose number starts with the digits, if
/// only one does.
pub fn after_digit(numbers: &[u8], typed: &str) -> Pick {
    let mut found = places(numbers, |n| matches(n, typed));
    match (found.next(), found.next()) {
        (Some(place), None) => Pick::Chosen(place),
        (Some(_), Some(_)) => Pick::Waiting,
        (None, _) => Pick::Nobody,
    }
}

/// After Enter: the one player with exactly that number. Two players with
/// the same number can't be told apart: still waiting, for the mouse.
pub fn after_enter(numbers: &[u8], typed: &str) -> Pick {
    let mut exact = places(numbers, |n| n.to_string() == typed);
    match (exact.next(), exact.next(), after_digit(numbers, typed)) {
        (Some(place), None, _) => Pick::Chosen(place),
        (_, _, Pick::Nobody) => Pick::Nobody,
        (_, _, Pick::Chosen(_) | Pick::Waiting) => Pick::Waiting,
    }
}

fn places<'a>(
    numbers: &'a [u8],
    keep: impl Fn(u8) -> bool + 'a,
) -> impl Iterator<Item = usize> + 'a {
    numbers
        .iter()
        .enumerate()
        .filter(move |(_, n)| keep(**n))
        .map(|(place, _)| place)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_digit_is_enough_when_no_other_number_starts_with_it() {
        let numbers = [1, 3, 7, 12, 30];
        assert_eq!(after_digit(&numbers, "7"), Pick::Chosen(2));
        // 1 and 12: one more digit, or Enter
        assert_eq!(after_digit(&numbers, "1"), Pick::Waiting);
        assert_eq!(after_digit(&numbers, "12"), Pick::Chosen(3));
        assert_eq!(after_enter(&numbers, "1"), Pick::Chosen(0));
        // 30 alone starts with 3, but 3 too: Enter for 3
        assert_eq!(after_digit(&numbers, "3"), Pick::Waiting);
        assert_eq!(after_enter(&numbers, "3"), Pick::Chosen(1));
        assert_eq!(after_digit(&numbers, "30"), Pick::Chosen(4));
    }

    #[test]
    fn enter_waits_without_the_exact_number() {
        // 30 and 31 but no 3
        let numbers = [30, 31, 5];
        assert_eq!(after_digit(&numbers, "3"), Pick::Waiting);
        assert_eq!(after_enter(&numbers, "3"), Pick::Waiting);
        assert_eq!(after_digit(&numbers, "31"), Pick::Chosen(1));
    }

    #[test]
    fn nobody_for_digits_no_number_starts_with() {
        let numbers = [1, 3, 30];
        assert_eq!(after_digit(&numbers, "4"), Pick::Nobody);
        assert_eq!(after_digit(&numbers, "35"), Pick::Nobody);
        assert_eq!(after_enter(&numbers, "4"), Pick::Nobody);
        assert_eq!(after_digit(&numbers, ""), Pick::Nobody);
        assert_eq!(after_digit(&[], "1"), Pick::Nobody);
    }

    #[test]
    fn the_shirt_zero() {
        let numbers = [0, 10];
        assert_eq!(after_digit(&numbers, "0"), Pick::Chosen(0));
        assert_eq!(after_digit(&numbers, "1"), Pick::Chosen(1));
    }

    #[test]
    fn two_players_with_the_same_number_are_left_to_the_mouse() {
        let numbers = [4, 4, 9];
        assert_eq!(after_digit(&numbers, "4"), Pick::Waiting);
        assert_eq!(after_enter(&numbers, "4"), Pick::Waiting);
    }
}
