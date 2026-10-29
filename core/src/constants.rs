pub const MATCH_DESCRIPTOR_FILE_NAME: &str = "match.json";
pub const TEAM_DESCRIPTOR_FILE_NAME: &str = "team.json";
pub const UPLOAD_QUEUE_FILE_NAME: &str = "upload_queue.json";
pub const DEFAULT_SET_TARGET_SCORE: u8 = 25;
pub const TIE_BREAK_SET_TARGET_SCORE: u8 = 15;
// scores at which the partials are recorded, when a team is ahead
pub const DEFAULT_SET_PARTIALS: &[u8] = &[8, 16, 21];
pub const TIE_BREAK_SET_PARTIALS: &[u8] = &[5, 10];
// substitutions allowed per set for new matches
pub const DEFAULT_MAX_SUBSTITUTIONS: u8 = 8;
// substitutions allowed per set before the 2026 rules, for the matches saved
// without the number
pub const LEGACY_MAX_SUBSTITUTIONS: u8 = 6;
// the players on the score sheet of a match: at most 14, and at least six
// and the libero to start a set
pub const MAX_ROSTER: usize = 14;
pub const MIN_ROSTER: usize = 7;
/// More players on the score sheet need a libero among them.
pub const MAX_ROSTER_WITHOUT_LIBERO: usize = 12;
pub const DEFAULT_LANGUAGE: &str = "en";
