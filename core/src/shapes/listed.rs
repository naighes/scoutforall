/// A team or match folder whose files couldn't be loaded: it's listed as
/// such, and its files are left as they are.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unreadable {
    /// the name of the folder
    pub id: String,
    /// why, when it's known: e.g. a set this version doesn't replay
    pub reason: Option<String>,
}

/// A team or match as listed: loaded, or unreadable.
pub type Listed<T> = Result<T, Unreadable>;
