use crate::localization::{Describe, current_labels};
use serde::{Deserialize, Serialize};
use std::fmt;

/// The actions of the screens, bound to fixed keys.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ScreenActionEnum {
    Back,
    Confirm,
    Next,
    Edit,
    Export,
    Import,
    EditPlayer,
    NewPlayer,
    RemovePlayer,
    EditTeam,
    MatchList,
    MatchStats,
    New,
    Delete,
    Previous,
    PrintReport,
    Quit,
    Select,
    Up,
    Down,
    ReportAnIssue,
    Settings,
    OneLevelUp,
    EnterDirectory,
    ScrollUp,
    ScrollDown,
    Reset,
    Undo,
    EditEvents,
}

impl Describe for ScreenActionEnum {
    fn describe(self) -> (ScreenActionEnum, String) {
        use ScreenActionEnum::*;
        match self {
            Back => (Back, current_labels().back.to_string()),
            Confirm => (Confirm, current_labels().confirm.to_string()),
            Next => (Next, current_labels().next.to_string()),
            Edit => (Edit, current_labels().edit.to_string()),
            Export => (Export, current_labels().export.to_string()),
            Import => (Import, current_labels().import.to_string()),
            EditPlayer => (EditPlayer, current_labels().edit_player.to_string()),
            NewPlayer => (NewPlayer, current_labels().new_player.to_string()),
            RemovePlayer => (RemovePlayer, current_labels().remove_player.to_string()),
            EditTeam => (EditTeam, current_labels().edit_team.to_string()),
            MatchList => (MatchList, current_labels().match_list.to_string()),
            MatchStats => (MatchStats, current_labels().match_stats.to_string()),
            New => (New, current_labels().new.to_string()),
            Previous => (Previous, current_labels().previous.to_string()),
            PrintReport => (PrintReport, current_labels().print_report.to_string()),
            Quit => (Quit, current_labels().quit.to_string()),
            Select => (Select, current_labels().select.to_string()),
            Up => (Up, current_labels().previous.to_string()),
            Down => (Down, current_labels().next.to_string()),
            ReportAnIssue => (ReportAnIssue, current_labels().report_an_issue.to_string()),
            Settings => (Settings, current_labels().settings.to_string()),
            OneLevelUp => (OneLevelUp, current_labels().up_one_level.to_string()),
            EnterDirectory => (EnterDirectory, current_labels().enter_directory.to_string()),
            ScrollUp => (ScrollUp, current_labels().scroll_up.to_string()),
            ScrollDown => (ScrollDown, current_labels().scroll_down.to_string()),
            Delete => (Delete, current_labels().delete.to_string()),
            Reset => (Reset, current_labels().reset.to_string()),
            Undo => (Undo, current_labels().undo.to_string()),
            EditEvents => (EditEvents, current_labels().edit_events.to_string()),
        }
    }
}

impl fmt::Display for ScreenActionEnum {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use ScreenActionEnum::*;
        let label = match self {
            Back => "back",
            Confirm => "confirm",
            Next => "next",
            Edit => "edit",
            Export => "export",
            Import => "import",
            EditPlayer => "edit-player",
            NewPlayer => "new-player",
            RemovePlayer => "remove-player",
            EditTeam => "edit-team",
            MatchList => "match-list",
            MatchStats => "match-stats",
            New => "new",
            Previous => "previous",
            PrintReport => "print-report",
            Quit => "quit",
            Select => "select",
            Up => "up",
            Down => "down",
            ReportAnIssue => "report-an-issue",
            Settings => "settings",
            OneLevelUp => "one-level-up",
            EnterDirectory => "enter-directory",
            ScrollUp => "scroll-up",
            ScrollDown => "scroll-down",
            Delete => "delete",
            Reset => "reset",
            Undo => "undo",
            EditEvents => "edit-events",
        };
        write!(f, "{}", label)
    }
}
