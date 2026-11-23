//! The training of a team: the plan screen (the sessions today; the
//! calendar, the cycles and the attendance in the next steps), the sheet of
//! a session with its activities and its attendance.

mod calendar;
mod cycles;
mod forms;
pub(crate) use forms::clash_warning;
mod history;
mod presence;
mod roll;
mod scope;
pub mod screen;
mod series;
mod session;
